use async_trait::async_trait;
use ptc_domain::OutputFormat;
use ptc_engine::{AiProvider, AiRequest, AiResponse, EngineError};
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};
use uuid::Uuid;

const OPENAI_BASE_URL: &str = "https://api.openai.com/v1/";
const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434/";

#[derive(Clone)]
pub enum ConfiguredProvider {
    Mock(MockProvider),
    OpenAi(OpenAiProvider),
    Ollama(OllamaProvider),
    Codex(CodexProvider),
}

impl ConfiguredProvider {
    pub fn new(kind: &str, model: &str, base_url: Option<&str>) -> Result<Self, EngineError> {
        match kind {
            "mock" => Ok(Self::Mock(MockProvider)),
            "openai" => Ok(Self::OpenAi(OpenAiProvider::new(model, base_url)?)),
            "ollama" => Ok(Self::Ollama(OllamaProvider::new(model, base_url)?)),
            "codex" if base_url.is_none() => Ok(Self::Codex(CodexProvider::new(model)?)),
            "codex" => Err(EngineError::Provider(
                "Codex does not use a base URL".to_owned(),
            )),
            _ => Err(EngineError::Provider(format!("unknown provider `{kind}`"))),
        }
    }
}

#[async_trait]
impl AiProvider for ConfiguredProvider {
    fn id(&self) -> &str {
        match self {
            Self::Mock(provider) => provider.id(),
            Self::OpenAi(provider) => provider.id(),
            Self::Ollama(provider) => provider.id(),
            Self::Codex(provider) => provider.id(),
        }
    }

    fn is_remote(&self) -> bool {
        match self {
            Self::Mock(provider) => provider.is_remote(),
            Self::OpenAi(provider) => provider.is_remote(),
            Self::Ollama(provider) => provider.is_remote(),
            Self::Codex(provider) => provider.is_remote(),
        }
    }

    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError> {
        match self {
            Self::Mock(provider) => provider.complete(request).await,
            Self::OpenAi(provider) => provider.complete(request).await,
            Self::Ollama(provider) => provider.complete(request).await,
            Self::Codex(provider) => provider.complete(request).await,
        }
    }
}

#[derive(Clone)]
pub struct OpenAiProvider {
    client: Client,
    endpoint: Url,
    model: String,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(model: &str, base_url: Option<&str>) -> Result<Self, EngineError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| EngineError::Provider("OPENAI_API_KEY is not set".to_owned()))?;
        Self::with_key(model, base_url, api_key)
    }

    fn with_key(model: &str, base_url: Option<&str>, api_key: String) -> Result<Self, EngineError> {
        if api_key.trim().is_empty() {
            return Err(EngineError::Provider("OPENAI_API_KEY is empty".to_owned()));
        }
        let base = validated_base_url(base_url.unwrap_or(OPENAI_BASE_URL), true)?;
        Ok(Self {
            client: http_client()?,
            endpoint: base
                .join("chat/completions")
                .map_err(|_| invalid_endpoint())?,
            model: validated_model(model)?,
            api_key,
        })
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn id(&self) -> &str {
        "openai"
    }
    fn is_remote(&self) -> bool {
        true
    }

    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError> {
        let mut body = json!({
            "model": self.model,
            "messages": [{"role":"user", "content":request.prompt}],
            "stream": false
        });
        if request.output_format == OutputFormat::Json {
            body["response_format"] = json!({"type":"json_object"});
        }
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| transport_error("OpenAI-compatible", &error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error("OpenAI-compatible", status));
        }
        let body: Value = response.json().await.map_err(|_| {
            EngineError::Provider("OpenAI-compatible response was not valid JSON".to_owned())
        })?;
        let content = body
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| {
                EngineError::Provider(
                    "OpenAI-compatible response had no message content".to_owned(),
                )
            })?;
        Ok(AiResponse {
            provider: self.id().to_owned(),
            model: body.get("model").and_then(Value::as_str).map(str::to_owned),
            content: content.to_owned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    client: Client,
    endpoint: Url,
    model: String,
}

impl OllamaProvider {
    pub fn new(model: &str, base_url: Option<&str>) -> Result<Self, EngineError> {
        let base = validated_base_url(base_url.unwrap_or(OLLAMA_BASE_URL), false)?;
        if !is_loopback(&base) {
            return Err(EngineError::Provider(
                "Ollama endpoint must be loopback for local mode".to_owned(),
            ));
        }
        Ok(Self {
            client: http_client()?,
            endpoint: base.join("api/chat").map_err(|_| invalid_endpoint())?,
            model: validated_model(model)?,
        })
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }
    fn is_remote(&self) -> bool {
        false
    }

    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError> {
        let mut body = json!({
            "model": self.model,
            "messages": [{"role":"user", "content":request.prompt}],
            "stream": false
        });
        if request.output_format == OutputFormat::Json {
            body["format"] = json!("json");
        }
        let response = self
            .client
            .post(self.endpoint.clone())
            .json(&body)
            .send()
            .await
            .map_err(|error| transport_error("Ollama", &error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error("Ollama", status));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| EngineError::Provider("Ollama response was not valid JSON".to_owned()))?;
        let content = body
            .pointer("/message/content")
            .and_then(Value::as_str)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| {
                EngineError::Provider("Ollama response had no message content".to_owned())
            })?;
        Ok(AiResponse {
            provider: self.id().to_owned(),
            model: body.get("model").and_then(Value::as_str).map(str::to_owned),
            content: content.to_owned(),
        })
    }
}

/// Runs a local Codex CLI signed in with ChatGPT. The CLI manages its own session;
/// PTConductor never reads or copies its cached credentials.
#[derive(Clone)]
pub struct CodexProvider {
    model: String,
    executable: PathBuf,
}

impl CodexProvider {
    pub fn new(model: &str) -> Result<Self, EngineError> {
        Ok(Self {
            model: validated_model(model)?,
            executable: PathBuf::from("codex"),
        })
    }

    async fn check_chatgpt_login(&self) -> Result<(), EngineError> {
        let status = timeout(
            Duration::from_secs(15),
            Command::new(&self.executable)
                .args(["login", "status"])
                .env_remove("OPENAI_API_KEY")
                .env_remove("CODEX_API_KEY")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| EngineError::Provider("Codex login check timed out".to_owned()))?
        .map_err(|_| {
            EngineError::Provider(
                "Codex CLI not found; install it and run `codex login`".to_owned(),
            )
        })?;
        let details = format!(
            "{} {}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        let details = details.to_ascii_lowercase();
        if !status.status.success()
            || !details.contains("chatgpt")
            || details.contains("api key")
            || details.contains("api-key")
            || details.contains("apikey")
        {
            return Err(EngineError::Provider(
                "Codex must be signed in with ChatGPT, not an API key; run `codex login status` and `codex login`".to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl AiProvider for CodexProvider {
    fn id(&self) -> &str {
        "codex"
    }

    fn is_remote(&self) -> bool {
        true
    }

    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError> {
        let instruction = if request.output_format == OutputFormat::Json {
            "Return only one valid JSON object. Do not use tools, execute commands, or access files.\n\n"
        } else {
            "Do not use tools, execute commands, or access files. Answer using only the supplied prompt.\n\n"
        };
        let prompt = format!("{instruction}{}", request.prompt);
        if prompt.len() > 256 * 1024 {
            return Err(EngineError::Provider(
                "Codex prompt exceeds 256 KiB".to_owned(),
            ));
        }
        self.check_chatgpt_login().await?;
        let working_dir = CodexWorkingDirectory::new()?;
        let content = timeout(
            Duration::from_secs(240),
            self.invoke(&working_dir.0, &prompt),
        )
        .await
        .map_err(|_| EngineError::Provider("Codex run timed out".to_owned()))??;
        Ok(AiResponse {
            provider: self.id().to_owned(),
            model: Some(self.model.clone()),
            content,
        })
    }
}

impl CodexProvider {
    async fn invoke(&self, working_dir: &PathBuf, prompt: &str) -> Result<String, EngineError> {
        let mut child = Command::new(&self.executable)
            .args(["--ask-for-approval", "never", "exec"])
            .args([
                "--sandbox",
                "read-only",
                "--ephemeral",
                "--ignore-user-config",
                "--skip-git-repo-check",
                "--model",
                &self.model,
                "-",
            ])
            .current_dir(working_dir)
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| EngineError::Provider("could not launch Codex CLI".to_owned()))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::Provider("could not open Codex prompt input".to_owned()))?;
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|_| EngineError::Provider("could not send prompt to Codex CLI".to_owned()))?;
        drop(stdin);
        let mut output = Vec::new();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EngineError::Provider("could not read Codex output".to_owned()))?;
        stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut output)
            .await
            .map_err(|_| EngineError::Provider("could not read Codex output".to_owned()))?;
        if output.len() > 1024 * 1024 {
            return Err(EngineError::Provider(
                "Codex output exceeds 1 MiB".to_owned(),
            ));
        }
        let status = child
            .wait()
            .await
            .map_err(|_| EngineError::Provider("Codex CLI process failed".to_owned()))?;
        if !status.success() {
            return Err(EngineError::Provider(
                "Codex run failed; check your ChatGPT login and Codex usage limits".to_owned(),
            ));
        }
        let content = String::from_utf8(output)
            .map_err(|_| EngineError::Provider("Codex output was not UTF-8".to_owned()))?;
        if content.trim().is_empty() {
            return Err(EngineError::Provider("Codex returned no output".to_owned()));
        }
        Ok(content)
    }
}

struct CodexWorkingDirectory(PathBuf);

impl CodexWorkingDirectory {
    fn new() -> Result<Self, EngineError> {
        let path = std::env::temp_dir().join(format!("ptconductor-codex-{}", Uuid::new_v4()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|_| {
            EngineError::Provider("could not create isolated Codex working directory".to_owned())
        })?;
        Ok(Self(path))
    }
}

impl Drop for CodexWorkingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn http_client() -> Result<Client, EngineError> {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| EngineError::Provider("could not initialize HTTP client".to_owned()))
}

fn validated_model(model: &str) -> Result<String, EngineError> {
    if model.trim().is_empty() || model.chars().any(char::is_whitespace) {
        return Err(EngineError::Provider("a model name is required".to_owned()));
    }
    Ok(model.to_owned())
}

fn validated_base_url(value: &str, requires_tls: bool) -> Result<Url, EngineError> {
    let mut url = Url::parse(value).map_err(|_| invalid_endpoint())?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.scheme(), "http" | "https")
        || (requires_tls && url.scheme() == "http" && !is_loopback(&url))
    {
        return Err(invalid_endpoint());
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

fn is_loopback(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    )
}

fn invalid_endpoint() -> EngineError {
    EngineError::Provider(
        "invalid provider base URL (OpenAI-compatible keys require HTTPS except loopback)"
            .to_owned(),
    )
}

fn transport_error(provider: &str, error: &reqwest::Error) -> EngineError {
    let reason = if error.is_timeout() {
        "request timed out"
    } else if error.is_connect() {
        "connection failed"
    } else {
        "request failed"
    };
    EngineError::Provider(format!("{provider} {reason}"))
}

fn status_error(provider: &str, status: reqwest::StatusCode) -> EngineError {
    let description = match status.as_u16() {
        401 | 403 => "authentication or access denied",
        404 => "endpoint or model not found",
        429 => "rate limit or quota exceeded",
        500..=599 => "server error",
        _ => "request rejected",
    };
    EngineError::Provider(format!(
        "{provider} {description} (HTTP {})",
        status.as_u16()
    ))
}

/// Deterministic provider used for local development and engine tests.
#[derive(Debug, Clone, Default)]
pub struct MockProvider;

#[async_trait]
impl AiProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn is_remote(&self) -> bool {
        false
    }

    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError> {
        Ok(AiResponse {
            provider: self.id().to_owned(),
            model: Some("deterministic-mock".to_owned()),
            content: if request.output_format == OutputFormat::Json {
                json!({"workflow":request.workflow_id,"step":request.step_id,"analysis":"deterministic mock output"}).to_string()
            } else {
                format!(
                    "Mock analysis for workflow `{}` step `{}`:\n\n{}",
                    request.workflow_id, request.step_id, request.prompt
                )
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn fake_http(body: &str, status: &str) -> (String, thread::JoinHandle<Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/v1/", listener.local_addr().unwrap());
        let body = body.to_owned();
        let status = status.to_owned();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut data = Vec::new();
            let header_end = loop {
                let mut buffer = [0u8; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                data.extend_from_slice(&buffer[..count]);
                if let Some(index) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let header = String::from_utf8_lossy(&data[..header_end]);
            let content_length: usize = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.trim().parse().ok())
                })
                .unwrap();
            while data.len() - header_end < content_length {
                let mut buffer = [0u8; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                data.extend_from_slice(&buffer[..count]);
            }
            let request: Value =
                serde_json::from_slice(&data[header_end..header_end + content_length]).unwrap();
            let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(reply.as_bytes()).unwrap();
            request
        });
        (address, handle)
    }

    fn request() -> AiRequest {
        AiRequest {
            workflow_id: "fingerprint".to_owned(),
            step_id: "analyze".to_owned(),
            prompt: "Return JSON observations".to_owned(),
            output_format: OutputFormat::Json,
        }
    }

    #[tokio::test]
    async fn openai_sends_json_mode_and_parses_response() {
        let (url, server) = fake_http(
            r#"{"model":"test","choices":[{"message":{"content":"{\"technologies\":[]}"}}]}"#,
            "200 OK",
        );
        let provider =
            OpenAiProvider::with_key("test-model", Some(&url), "test-only-key".to_owned()).unwrap();
        let result = provider.complete(request()).await.unwrap();
        assert_eq!(result.content, r#"{"technologies":[]}"#);
        let sent = server.join().unwrap();
        assert_eq!(sent["model"], "test-model");
        assert_eq!(sent["response_format"]["type"], "json_object");
        assert_eq!(sent["messages"][0]["content"], "Return JSON observations");
    }

    #[tokio::test]
    async fn ollama_sends_non_streaming_json_request() {
        let (url, server) = fake_http(
            r#"{"model":"local","message":{"content":"{\"technologies\":[]}"}}"#,
            "200 OK",
        );
        let provider = OllamaProvider::new("local", Some(&url)).unwrap();
        let result = provider.complete(request()).await.unwrap();
        assert_eq!(result.content, r#"{"technologies":[]}"#);
        let sent = server.join().unwrap();
        assert_eq!(sent["format"], "json");
        assert_eq!(sent["stream"], false);
    }

    #[tokio::test]
    async fn errors_do_not_echo_response_body() {
        let (url, server) = fake_http(
            r#"{"error":"test-only-key private prompt"}"#,
            "401 Unauthorized",
        );
        let provider =
            OpenAiProvider::with_key("test-model", Some(&url), "test-only-key".to_owned()).unwrap();
        let error = provider.complete(request()).await.unwrap_err().to_string();
        server.join().unwrap();
        assert!(error.contains("HTTP 401"));
        assert!(!error.contains("test-only-key"));
        assert!(!error.contains("private prompt"));
    }

    #[test]
    fn rejects_non_tls_key_destination_and_nonlocal_ollama() {
        assert!(
            OpenAiProvider::with_key("test", Some("http://example.com/v1"), "key".to_owned())
                .is_err()
        );
        assert!(OllamaProvider::new("test", Some("http://example.com/")).is_err());
        assert!(
            ConfiguredProvider::new("codex", "gpt-6-sol", Some("https://example.com")).is_err()
        );
    }

    #[cfg(unix)]
    fn fake_codex(login_message: &str) -> (CodexWorkingDirectory, CodexProvider) {
        use std::os::unix::fs::PermissionsExt;

        let fixture = CodexWorkingDirectory::new().unwrap();
        let executable = fixture.0.join("codex");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = login ]; then\n  printf '%s\\n' '{login_message}'\n  exit 0\nfi\nprintf '%s\\n' \"$@\" > \"{args}\"\ncat > \"{prompt}\"\nprintf '%s\\n' '{{\"technologies\":[]}}'\n",
            args = fixture.0.join("args").display(),
            prompt = fixture.0.join("prompt").display(),
        );
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        (
            fixture,
            CodexProvider {
                model: "gpt-6-sol".to_owned(),
                executable,
            },
        )
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn codex_uses_chatgpt_login_and_passes_prompt_via_stdin() {
        let (fixture, provider) = fake_codex("Logged in using ChatGPT");
        let response = provider.complete(request()).await.unwrap();
        assert_eq!(response.provider, "codex");
        assert_eq!(response.model.as_deref(), Some("gpt-6-sol"));
        assert_eq!(response.content.trim(), r#"{"technologies":[]}"#);
        let args = fs::read_to_string(fixture.0.join("args")).unwrap();
        for required in [
            "read-only",
            "never",
            "--ephemeral",
            "--ignore-user-config",
            "-",
        ] {
            assert!(args.lines().any(|argument| argument == required));
        }
        assert!(!args.contains("Return JSON observations"));
        let prompt = fs::read_to_string(fixture.0.join("prompt")).unwrap();
        assert!(prompt.contains("Return only one valid JSON object"));
        assert!(prompt.contains("Return JSON observations"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn codex_rejects_api_key_authentication() {
        let (fixture, provider) = fake_codex("Logged in using an API key");
        let error = provider.complete(request()).await.unwrap_err().to_string();
        assert!(error.contains("signed in with ChatGPT"));
        assert!(!fixture.0.join("args").exists());
    }
}
