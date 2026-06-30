use crate::{
    auth::User,
    config::{AppConfig, PolicyConfig},
    error::ensure_http_success,
};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
use std::collections::BTreeMap;
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub platform: String,
    pub os: String,
    pub os_name: String,
    pub os_version: String,
    pub os_build: Option<String>,
    pub kernel_version: Option<String>,
    pub family: String,
    pub arch: String,
    pub username: String,
    pub timezone: String,
    pub locale: String,
    pub shell: Option<String>,
    pub current_dir: Option<PathBuf>,
    pub executable_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRegisterRequest {
    pub device_id: String,
    pub device_name: String,
    pub client_version: String,
    pub protocol_version: String,
    pub user: User,
    pub system: SystemInfo,
    pub capabilities: Vec<String>,
    pub policy_summary: PolicySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySummary {
    pub allowed_roots: Vec<String>,
    pub shell_exec_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRegisterResponse {
    pub device_id: String,
    pub server_device_id: Option<u64>,
    pub status: String,
    pub display_name: Option<String>,
    pub ws_url: Option<String>,
    pub connection_token: Option<String>,
    pub connection_token_expires_at: Option<String>,
    pub server_time: Option<String>,
    pub min_client_version: Option<String>,
    pub latest_client_version: Option<String>,
    pub policy: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct DeviceClient {
    http: Client,
    base_url: String,
}

impl DeviceClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub async fn register(
        &self,
        token: &str,
        request: &DeviceRegisterRequest,
    ) -> Result<DeviceRegisterResponse> {
        let response = self
            .http
            .post(format!("{}/api/v1/agent/devices/register", self.base_url))
            .bearer_auth(token)
            .json(request)
            .send()
            .await
            .context("failed to register device")?;

        let response = ensure_http_success(response, "device registration").await?;

        response
            .json()
            .await
            .context("failed to parse device register response")
    }
}

impl DeviceRegisterRequest {
    pub fn from_config(config: &AppConfig, user: User) -> Self {
        let system = collect_system_info();
        Self {
            device_id: config.client.device_id.clone(),
            device_name: system.hostname.clone(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: "1.0".to_string(),
            user,
            system,
            capabilities: default_capabilities(),
            policy_summary: PolicySummary::from_policy(&config.policy),
        }
    }
}

impl PolicySummary {
    pub fn from_policy(policy: &PolicyConfig) -> Self {
        Self {
            allowed_roots: policy
                .allowed_roots
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            shell_exec_enabled: policy.shell_exec_enabled,
        }
    }
}

pub fn collect_system_info() -> SystemInfo {
    let details = collect_os_details();
    SystemInfo {
        hostname: hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string()),
        platform: std::env::consts::OS.to_string(),
        os: std::env::consts::OS.to_string(),
        os_name: details.os_name,
        os_version: details.os_version,
        os_build: details.os_build,
        kernel_version: details.kernel_version,
        family: std::env::consts::FAMILY.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        username: whoami::username(),
        timezone: chrono::Local::now().offset().to_string(),
        locale: std::env::var("LANG").unwrap_or_else(|_| "unknown".to_string()),
        shell: current_shell(),
        current_dir: std::env::current_dir().ok(),
        executable_path: std::env::current_exe().ok(),
    }
}

#[derive(Debug)]
struct OsDetails {
    os_name: String,
    os_version: String,
    os_build: Option<String>,
    kernel_version: Option<String>,
}

fn collect_os_details() -> OsDetails {
    #[cfg(target_os = "macos")]
    {
        let product_name =
            command_output("sw_vers", &["-productName"]).unwrap_or_else(|| "macOS".to_string());
        let product_version = command_output("sw_vers", &["-productVersion"])
            .unwrap_or_else(|| "unknown".to_string());
        let build = command_output("sw_vers", &["-buildVersion"]);
        return OsDetails {
            os_name: product_name,
            os_version: product_version,
            os_build: build,
            kernel_version: command_output("uname", &["-r"]),
        };
    }

    #[cfg(target_os = "windows")]
    {
        return OsDetails {
            os_name: "Windows".to_string(),
            os_version: command_output("cmd", &["/C", "ver"])
                .unwrap_or_else(|| "unknown".to_string()),
            os_build: None,
            kernel_version: None,
        };
    }

    #[cfg(target_os = "linux")]
    {
        let os_release = read_os_release();
        let os_name = os_release
            .get("PRETTY_NAME")
            .or_else(|| os_release.get("NAME"))
            .cloned()
            .unwrap_or_else(|| "Linux".to_string());
        let os_version = os_release
            .get("VERSION_ID")
            .or_else(|| os_release.get("VERSION"))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        return OsDetails {
            os_name,
            os_version,
            os_build: os_release.get("BUILD_ID").cloned(),
            kernel_version: command_output("uname", &["-r"]),
        };
    }

    #[allow(unreachable_code)]
    OsDetails {
        os_name: std::env::consts::OS.to_string(),
        os_version: "unknown".to_string(),
        os_build: None,
        kernel_version: command_output("uname", &["-r"]),
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(target_os = "linux")]
fn read_os_release() -> BTreeMap<String, String> {
    let Ok(raw) = std::fs::read_to_string("/etc/os-release") else {
        return BTreeMap::new();
    };
    raw.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.trim_matches('"').to_string()))
        })
        .collect()
}

fn current_shell() -> Option<String> {
    std::env::var("SHELL")
        .ok()
        .or_else(|| std::env::var("COMSPEC").ok())
}

pub fn default_capabilities() -> Vec<String> {
    [
        "system.info",
        "fs.stat",
        "fs.list",
        "fs.read",
        "fs.write",
        "fs.mkdir",
        "fs.move",
        "fs.delete",
        "fs.search",
        "process.exec",
        "process.cancel",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
