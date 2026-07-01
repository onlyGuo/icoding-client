use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_API_BASE_URL: &str = "https://apilite.icoding.ink";
const DEFAULT_WS_URL: &str = "wss://apilite.icoding.ink/api/v1/agent/ws";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub client: ClientConfig,
    pub policy: PolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub api_base_url: String,
    pub ws_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub device_id: String,
    pub auto_start: bool,
    pub start_minimized: bool,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub allowed_roots: Vec<PathBuf>,
    pub blocked_paths: Vec<PathBuf>,
    #[serde(default = "default_shell_exec_enabled")]
    pub shell_exec_enabled: bool,
    pub max_file_read_bytes: u64,
    pub max_file_write_bytes: u64,
    pub max_command_output_bytes: u64,
    pub default_command_timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub config_file: PathBuf,
    pub session_file: PathBuf,
    pub token_file: PathBuf,
}

impl AppConfig {
    pub fn load_or_create(paths: &AppPaths) -> Result<Self> {
        paths.ensure()?;

        if paths.config_file.exists() {
            let raw = fs::read_to_string(&paths.config_file).with_context(|| {
                format!("failed to read config {}", paths.config_file.display())
            })?;
            let mut config: AppConfig = toml::from_str(&raw).with_context(|| {
                format!("failed to parse config {}", paths.config_file.display())
            })?;
            let mut needs_save = false;
            if contains_legacy_approval_settings(&raw) {
                config.policy.shell_exec_enabled = true;
                needs_save = true;
            }
            needs_save |= config.normalize_server_endpoints();
            if config.client.device_id.trim().is_empty() {
                config.client.device_id = new_device_id();
                needs_save = true;
            }
            if needs_save {
                config.save(paths)?;
            }
            return Ok(config);
        }

        let config = AppConfig::default();
        config.save(paths)?;
        Ok(config)
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        paths.ensure()?;
        let raw = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(&paths.config_file, raw)
            .with_context(|| format!("failed to write config {}", paths.config_file.display()))?;
        Ok(())
    }

    pub fn reset_server_endpoints(&mut self) {
        self.server.api_base_url = DEFAULT_API_BASE_URL.to_string();
        self.server.ws_url = DEFAULT_WS_URL.to_string();
    }

    pub fn normalize_server_endpoints(&mut self) -> bool {
        let api_base_url = self.server.api_base_url.trim().trim_end_matches('/');
        let ws_url = self.server.ws_url.trim();

        if api_base_url == DEFAULT_API_BASE_URL && ws_url == DEFAULT_WS_URL {
            return false;
        }

        self.reset_server_endpoints();
        true
    }

    pub fn apply_server_override(
        &mut self,
        _api_base_url: Option<String>,
        _ws_url: Option<String>,
    ) {
        self.reset_server_endpoints();
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                api_base_url: DEFAULT_API_BASE_URL.to_string(),
                ws_url: DEFAULT_WS_URL.to_string(),
            },
            client: ClientConfig {
                device_id: new_device_id(),
                auto_start: true,
                start_minimized: true,
                log_level: "info".to_string(),
            },
            policy: PolicyConfig::default(),
        }
    }
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allowed_roots: default_allowed_roots(),
            blocked_paths: default_blocked_paths(),
            shell_exec_enabled: default_shell_exec_enabled(),
            max_file_read_bytes: 1_048_576,
            max_file_write_bytes: 1_048_576,
            max_command_output_bytes: 10_485_760,
            default_command_timeout_seconds: 300,
        }
    }
}

fn default_shell_exec_enabled() -> bool {
    true
}

fn contains_legacy_approval_settings(raw: &str) -> bool {
    raw.lines().any(|line| {
        line.split_once('=')
            .map(|(key, _)| key.trim())
            .is_some_and(|key| {
                matches!(
                    key,
                    "approval_required_for_shell" | "approval_required_for_delete"
                )
            })
    })
}

impl AppPaths {
    pub fn resolve() -> Result<Self> {
        if let Ok(home) = std::env::var("ICODING_CLIENT_HOME") {
            let root = PathBuf::from(home);
            let config_dir = root.join("config");
            let data_dir = root.join("data");
            let log_dir = root.join("logs");
            return Ok(Self {
                config_file: config_dir.join("config.toml"),
                session_file: data_dir.join("session.json"),
                token_file: data_dir.join("session.token"),
                log_dir,
                config_dir,
                data_dir,
            });
        }

        let dirs = ProjectDirs::from("com", "icoding", "icoding-client")
            .context("failed to resolve application directories")?;
        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_dir().to_path_buf();
        let log_dir = dirs.data_local_dir().join("logs");
        Ok(Self {
            config_file: config_dir.join("config.toml"),
            session_file: data_dir.join("session.json"),
            token_file: data_dir.join("session.token"),
            config_dir,
            data_dir,
            log_dir,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        create_dir(&self.config_dir)?;
        create_dir(&self.data_dir)?;
        create_dir(&self.log_dir)?;
        Ok(())
    }
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

fn new_device_id() -> String {
    format!("dev_{}", uuid::Uuid::new_v4().simple())
}

fn default_allowed_roots() -> Vec<PathBuf> {
    std::env::current_dir()
        .map(|path| vec![path])
        .unwrap_or_default()
}

fn default_blocked_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        paths.push(home.join(".ssh"));
        paths.push(home.join(".gnupg"));
        paths.push(home.join("Library/Keychains"));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths() -> AppPaths {
        let root = std::env::temp_dir().join(format!(
            "icoding-client-config-test-{}",
            uuid::Uuid::new_v4()
        ));
        let config_dir = root.join("config");
        let data_dir = root.join("data");
        AppPaths {
            config_file: config_dir.join("config.toml"),
            session_file: data_dir.join("session.json"),
            token_file: data_dir.join("session.token"),
            log_dir: root.join("logs"),
            config_dir,
            data_dir,
        }
    }

    #[test]
    fn command_execution_is_enabled_by_default() {
        assert!(PolicyConfig::default().shell_exec_enabled);
    }

    #[test]
    fn default_server_uses_icoding_lite_endpoint() {
        let config = AppConfig::default();
        assert_eq!(config.server.api_base_url, DEFAULT_API_BASE_URL);
        assert_eq!(config.server.ws_url, DEFAULT_WS_URL);
    }

    #[test]
    fn legacy_approval_settings_migrate_to_enabled_commands() -> Result<()> {
        let paths = test_paths();
        paths.ensure()?;
        let mut config = AppConfig::default();
        config.policy.shell_exec_enabled = false;
        let mut raw = toml::to_string_pretty(&config)?;
        raw.push_str("approval_required_for_shell = true\napproval_required_for_delete = true\n");
        fs::write(&paths.config_file, raw)?;

        let migrated = AppConfig::load_or_create(&paths)?;
        let saved = fs::read_to_string(&paths.config_file)?;

        assert!(migrated.policy.shell_exec_enabled);
        assert!(!saved.contains("approval_required_for_shell"));
        assert!(!saved.contains("approval_required_for_delete"));

        if let Some(root) = paths.config_dir.parent() {
            let _ = fs::remove_dir_all(root);
        }
        Ok(())
    }

    #[test]
    fn non_default_server_addresses_are_migrated_to_icoding_lite() -> Result<()> {
        let paths = test_paths();
        paths.ensure()?;
        let mut config = AppConfig::default();
        config.server.api_base_url = "https://old-api.example.test".to_string();
        config.server.ws_url = "wss://old-api.example.test/api/v1/agent/ws".to_string();
        fs::write(&paths.config_file, toml::to_string_pretty(&config)?)?;

        let migrated = AppConfig::load_or_create(&paths)?;

        assert_eq!(migrated.server.api_base_url, DEFAULT_API_BASE_URL);
        assert_eq!(migrated.server.ws_url, DEFAULT_WS_URL);

        if let Some(root) = paths.config_dir.parent() {
            let _ = fs::remove_dir_all(root);
        }
        Ok(())
    }
}
