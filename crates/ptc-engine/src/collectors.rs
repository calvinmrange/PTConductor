use crate::{process::run_bounded, EngineError};
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{net::IpAddr, time::Duration};
use tokio::process::Command;

fn invalid(reason: &str) -> EngineError {
    EngineError::InvalidWorkflow(reason.into())
}

pub fn validate_target(target: &str) -> Result<(), EngineError> {
    if target.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if target.is_empty()
        || target.len() > 253
        || target
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        || !target.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    {
        return Err(invalid(
            "Nmap target must be one IP address or hostname, without options, ranges, or CIDR",
        ));
    }
    Ok(())
}

pub fn validate_ports(ports: &str) -> Result<(), EngineError> {
    let mut count = 0u32;
    for part in ports.split(',') {
        let range: Vec<_> = part.split('-').collect();
        if range.is_empty() || range.len() > 2 {
            return Err(invalid("invalid Nmap TCP ports"));
        }
        let parse = |s: &str| s.parse::<u16>().ok().filter(|v| *v > 0);
        let start = parse(range[0]).ok_or_else(|| invalid("ports must be between 1 and 65535"))?;
        let end = if range.len() == 2 {
            parse(range[1]).ok_or_else(|| invalid("invalid port range"))?
        } else {
            start
        };
        if end < start {
            return Err(invalid("port range is reversed"));
        }
        count += u32::from(end) - u32::from(start) + 1;
        if count > 1024 {
            return Err(invalid(
                "MVP scans are limited to 1024 TCP ports per target",
            ));
        }
    }
    Ok(())
}

pub fn validate_url(target: &str) -> Result<Url, EngineError> {
    let remainder = target
        .strip_prefix("https://")
        .or_else(|| target.strip_prefix("http://"));
    if remainder.is_none_or(|v| v.is_empty() || v.starts_with('/') || v.starts_with('\\'))
        || target.chars().any(char::is_whitespace)
        || target.contains('\\')
    {
        return Err(invalid("expected an HTTP or HTTPS URL with a host"));
    }
    let url = Url::parse(target).map_err(|_| invalid("expected an HTTP or HTTPS URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "HTTP target must have a host and no credentials or fragment",
        ));
    }
    Ok(url)
}

pub async fn nmap_scan(target: &str, ports: &str) -> Result<Value, EngineError> {
    validate_target(target)?;
    validate_ports(ports)?;
    let args = nmap_args(target, ports);
    let mut command = Command::new("nmap");
    command.args(&args);
    let output = run_bounded(command, b"", Duration::from_secs(75), 192 * 1024).await
        .map_err(|_| EngineError::Provider("Nmap failed to start, timed out, or exceeded the output limit; install nmap and check the target and selected ports".into()))?;
    if !output.status.success() {
        return Err(EngineError::Provider(
            "Nmap exited unsuccessfully; check target resolution and selected ports".into(),
        ));
    }
    let xml = String::from_utf8(output.stdout).map_err(|_| invalid("Nmap output was not UTF-8"))?;
    if !xml.contains("<nmaprun")
        || !xml.contains("</nmaprun>")
        || xml.contains("exit=\"error\"")
        || xml.contains("timedout=\"true\"")
    {
        return Err(EngineError::Provider("Nmap did not complete a valid XML scan; check connectivity or reduce the port selection".into()));
    }
    Ok(json!({"collector":"nmap","target":target,"ports":ports,"arguments":args,"xml":xml}))
}

fn nmap_args(target: &str, ports: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-sT",
        "-sV",
        "--version-light",
        "-Pn",
        "-T3",
        "--host-timeout",
        "60s",
        "-p",
        ports,
        "-oX",
        "-",
    ]
    .iter()
    .map(|v| (*v).into())
    .collect();
    if target.parse::<std::net::Ipv6Addr>().is_ok() {
        args.push("-6".into());
    }
    args.push(target.into());
    args
}

pub async fn security_headers(target: &str) -> Result<Value, EngineError> {
    let url = validate_url(target)?;
    let response = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| invalid("could not initialize HTTP client"))?
        .get(url.clone())
        .header("User-Agent", "PTConductor/0.1 security-header-assessment")
        .send()
        .await
        .map_err(|_| {
            EngineError::Provider(
                "HTTP request failed; check the URL, DNS, connectivity, and TLS certificate".into(),
            )
        })?;
    // Preserve only relevant security headers, never session cookies or authentication values.
    let names = [
        "strict-transport-security",
        "content-security-policy",
        "content-security-policy-report-only",
        "x-content-type-options",
        "x-frame-options",
        "referrer-policy",
        "permissions-policy",
        "cross-origin-opener-policy",
        "cross-origin-resource-policy",
        "cross-origin-embedder-policy",
        "server",
        "x-powered-by",
    ];
    let mut headers = serde_json::Map::new();
    for name in names {
        let values: Vec<_> = response
            .headers()
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap_or("[non-UTF8 header]").to_owned())
            .collect();
        if !values.is_empty() {
            headers.insert(name.into(), json!(values));
        }
    }
    Ok(
        json!({"collector":"http_headers","url":url.as_str(),"status":response.status().as_u16(),"https":url.scheme()=="https", "redirectFollowed":false,"headers":headers,"note":"One GET response; body discarded. Cookies and authentication headers omitted. Absence is an observation, not proof of exploitability."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_command_injection_and_scan_expansion() {
        for target in [
            "-iL",
            "127.0.0.1;id",
            "10.0.0.0/8",
            "127.0.0.1-2",
            "host other",
            "$(id)",
            "",
            "https://host",
        ] {
            assert!(validate_target(target).is_err(), "{target}");
        }
        for ports in ["-p80", "0", "65536", "80;id", "1-65535", "90-80"] {
            assert!(validate_ports(ports).is_err(), "{ports}");
        }
        assert!(validate_target("127.0.0.1").is_ok());
        assert!(validate_target("::1").is_ok());
        assert!(validate_ports("22,80,443,8000-8010").is_ok());
        assert_eq!(nmap_args("localhost", "80").last().unwrap(), "localhost");
    }
    #[test]
    fn rejects_http_credentials_and_non_http_urls() {
        for url in [
            "file:///etc/passwd",
            "https://user:pass@host",
            "https:///",
            "ftp://host",
        ] {
            assert!(validate_url(url).is_err());
        }
    }
}
