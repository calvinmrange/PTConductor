//! Bounded subprocess I/O shared by the local collectors and Codex adapter.
use crate::EngineError;
use std::{
    io,
    process::{Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

async fn read_bounded(reader: impl AsyncRead + Unpin, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(io::Error::other("process output limit exceeded"));
    }
    Ok(bytes)
}

pub async fn run_bounded(
    mut command: Command,
    input: &[u8],
    duration: Duration,
    limit: usize,
) -> Result<Output, EngineError> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            EngineError::Provider(
                "could not launch process; check executable installation and PATH".into(),
            )
        })?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let result = tokio::time::timeout(duration, async {
        let (_, stdout, stderr, status) = tokio::try_join!(
            async {
                stdin.write_all(input).await?;
                drop(stdin);
                Ok::<_, io::Error>(())
            },
            read_bounded(stdout, limit),
            read_bounded(stderr, limit),
            child.wait()
        )?;
        Ok::<_, io::Error>(Output {
            status,
            stdout,
            stderr,
        })
    })
    .await;
    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(_)) => {
            let _ = child.kill().await;
            Err(EngineError::Provider(
                "process I/O failed or output limit exceeded".into(),
            ))
        }
        Err(_) => {
            let _ = child.kill().await;
            Err(EngineError::Provider(
                "process timed out; reduce the workload or check connectivity".into(),
            ))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stops_a_timed_out_process() {
        let mut command = Command::new("sleep");
        command.arg("10");
        let error = run_bounded(command, b"", Duration::from_millis(30), 1024)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn bounds_output_and_omits_child_content_from_errors() {
        let mut command = Command::new("printf");
        command.arg("private-output-too-long");
        let error = run_bounded(command, b"", Duration::from_secs(2), 4)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("output limit"));
        assert!(!error.to_string().contains("private-output"));
    }
}
