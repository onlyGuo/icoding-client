use crate::policy::Policy;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::{process::Command, time};

#[derive(Debug, Clone)]
pub struct ProcessCapabilities {
    policy: Policy,
}

#[derive(Debug, Deserialize)]
struct ExecParams {
    command: String,
    cwd: PathBuf,
    timeout_seconds: Option<u64>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default = "default_shell")]
    shell: String,
}

impl ProcessCapabilities {
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    pub async fn exec(&self, params: Value) -> Result<Value> {
        let params: ExecParams =
            serde_json::from_value(params).context("invalid process.exec parameters")?;
        let cwd = self.policy.check_shell_exec(&params.cwd)?;
        if params.command.trim().is_empty() {
            bail!("command is empty");
        }

        let timeout_seconds = params
            .timeout_seconds
            .unwrap_or(self.policy.config().default_command_timeout_seconds)
            .min(self.policy.config().default_command_timeout_seconds);

        let mut command = shell_command(&params.shell, &params.command)?;
        command.current_dir(&cwd);
        for (key, value) in params.env {
            command.env(key, value);
        }

        let output = time::timeout(Duration::from_secs(timeout_seconds), command.output())
            .await
            .with_context(|| format!("command timed out after {timeout_seconds} seconds"))?
            .with_context(|| format!("failed to execute command in {}", cwd.display()))?;

        let stdout_truncated =
            output.stdout.len() as u64 > self.policy.config().max_command_output_bytes;
        let stderr_truncated =
            output.stderr.len() as u64 > self.policy.config().max_command_output_bytes;
        let stdout = truncate_utf8_lossy(
            &output.stdout,
            self.policy.config().max_command_output_bytes as usize,
        );
        let stderr = truncate_utf8_lossy(
            &output.stderr,
            self.policy.config().max_command_output_bytes as usize,
        );

        Ok(json!({
            "command": params.command,
            "cwd": cwd,
            "exit_code": output.status.code(),
            "success": output.status.success(),
            "stdout": stdout,
            "stderr": stderr,
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated
        }))
    }
}

fn shell_command(shell: &str, command: &str) -> Result<Command> {
    if shell != "default" {
        bail!("only default shell is supported in this build");
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        Ok(cmd)
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(command);
        Ok(cmd)
    }
}

fn truncate_utf8_lossy(bytes: &[u8], limit: usize) -> String {
    let end = bytes.len().min(limit);
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

fn default_shell() -> String {
    "default".to_string()
}
