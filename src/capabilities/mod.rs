pub mod fs;
pub mod process;
pub mod system;

use crate::{config::AppConfig, policy::Policy};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct CapabilityDispatcher {
    fs: fs::FsCapabilities,
    process: process::ProcessCapabilities,
    system: system::SystemCapabilities,
}

#[derive(Debug)]
pub struct CapabilityOutput {
    pub result: Value,
    pub duration_ms: u128,
}

impl CapabilityDispatcher {
    pub fn new(config: AppConfig) -> Self {
        let policy = Policy::new(config.policy.clone());
        Self {
            fs: fs::FsCapabilities::new(policy.clone()),
            process: process::ProcessCapabilities::new(policy),
            system: system::SystemCapabilities::new(config),
        }
    }

    pub async fn dispatch(&self, capability: &str, params: Value) -> Result<CapabilityOutput> {
        let started = Instant::now();
        let result = match capability {
            "system.info" => self.system.info().context("system.info failed")?,
            "fs.stat" => self.fs.stat(params).context("fs.stat failed")?,
            "fs.list" => self.fs.list(params).context("fs.list failed")?,
            "fs.read" => self.fs.read(params).context("fs.read failed")?,
            "fs.write" => self.fs.write(params).context("fs.write failed")?,
            "fs.mkdir" => self.fs.mkdir(params).context("fs.mkdir failed")?,
            "fs.move" => self.fs.move_path(params).context("fs.move failed")?,
            "fs.delete" => self.fs.delete(params).context("fs.delete failed")?,
            "fs.search" => self.fs.search(params).context("fs.search failed")?,
            "process.exec" => self
                .process
                .exec(params)
                .await
                .context("process.exec failed")?,
            other => bail!("unsupported capability: {other}"),
        };
        Ok(CapabilityOutput {
            result,
            duration_ms: started.elapsed().as_millis(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{code, message};
    use serde_json::json;

    #[tokio::test]
    async fn list_error_reports_missing_path_and_reason() {
        let config = AppConfig::default();
        let missing = std::env::current_dir()
            .expect("current directory should be available")
            .join(format!("missing-{}", uuid::Uuid::new_v4()));

        let error = CapabilityDispatcher::new(config)
            .dispatch("fs.list", json!({ "path": missing }))
            .await
            .expect_err("listing a missing path should fail");
        let rendered = message(&error);

        assert_eq!(code(&error), "NOT_FOUND");
        assert!(rendered.contains("fs.list failed"));
        assert!(rendered.contains(&missing.display().to_string()));
        assert!(rendered.contains("No such file") || rendered.contains("not found"));
    }

    #[tokio::test]
    async fn invalid_parameters_report_the_serde_reason() {
        let error = CapabilityDispatcher::new(AppConfig::default())
            .dispatch("fs.list", json!({ "recursive": true }))
            .await
            .expect_err("missing path should fail parameter parsing");
        let rendered = message(&error);

        assert_eq!(code(&error), "INVALID_ARGUMENT");
        assert!(rendered.contains("invalid fs.list parameters"));
        assert!(rendered.contains("missing field `path`"));
    }
}
