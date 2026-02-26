pub mod config;
pub mod registry;

use std::process::Stdio;

use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::instance::SshHostConfig;

#[derive(Debug, Clone)]
pub struct SshSession {
    config: SshHostConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Error)]
pub enum SshError {
    #[error("ssh spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("invalid host config: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, SshError>;

impl SshSession {
    pub async fn connect(config: &SshHostConfig) -> Result<Self> {
        if config.host.trim().is_empty() {
            return Err(SshError::InvalidConfig("host is empty".to_string()));
        }
        Ok(Self {
            config: config.clone(),
        })
    }

    pub async fn exec(&self, cmd: &str) -> Result<ExecResult> {
        let output = self.run_ssh(&[cmd]).await?;
        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    pub async fn sftp_read(&self, path: &str) -> Result<Vec<u8>> {
        let escaped = shell_escape(path);
        let result = self.exec(&format!("cat {escaped}")).await?;
        Ok(result.stdout.into_bytes())
    }

    pub async fn sftp_write(&self, path: &str, content: &[u8]) -> Result<()> {
        let escaped = shell_escape(path);
        let command = format!("mkdir -p \"$(dirname {escaped})\" && cat > {escaped}");
        let destination = format!("{}@{}", self.config.username, self.config.host);

        let mut child = Command::new("ssh")
            .args(self.common_ssh_args())
            .arg(destination)
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(content).await?;
        }
        let _ = child.wait().await?;
        Ok(())
    }

    fn common_ssh_args(&self) -> Vec<String> {
        let mut args = vec!["-p".to_string(), self.config.port.to_string()];
        if let Some(key_path) = &self.config.key_path {
            if !key_path.trim().is_empty() {
                args.push("-i".to_string());
                args.push(key_path.clone());
            }
        }
        args
    }

    async fn run_ssh(&self, remote_args: &[&str]) -> Result<std::process::Output> {
        let destination = if self.config.username.trim().is_empty() {
            self.config.host.clone()
        } else {
            format!("{}@{}", self.config.username, self.config.host)
        };
        let mut cmd = Command::new("ssh");
        cmd.args(self.common_ssh_args()).arg(destination);
        for arg in remote_args {
            cmd.arg(arg);
        }
        Ok(cmd.output().await?)
    }
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_rejects_empty_host() {
        let cfg = SshHostConfig {
            id: "ssh:bad".to_string(),
            label: "Bad".to_string(),
            host: String::new(),
            port: 22,
            username: "ubuntu".to_string(),
            auth_method: "key".to_string(),
            key_path: None,
            password: None,
        };
        let result = SshSession::connect(&cfg).await;
        assert!(result.is_err());
    }
}
