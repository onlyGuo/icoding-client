use crate::{config::AppPaths, error::ensure_http_success};
use anyhow::{Context, Result, bail};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub mobile: Option<String>,
    #[serde(default)]
    pub nicker: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserSessionCache {
    pub user: User,
}

#[derive(Debug, Clone)]
pub enum LoginTarget {
    Email(String),
    Mobile(String),
}

#[derive(Debug, Clone)]
pub struct AuthClient {
    http: Client,
    base_url: String,
}

pub struct SessionStore {
    paths: AppPaths,
}

impl AuthClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .cookie_store(true)
            .build()
            .context("failed to create HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    pub async fn current_user(&self, token: Option<&str>) -> Result<Option<User>> {
        let url = self.url("/api/v1/user");
        let mut request = self.http.get(url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .context("failed to request current user")?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Ok(None);
        }
        let response = ensure_http_success(response, "request current user").await?;
        let bytes = response
            .bytes()
            .await
            .context("failed to read user response")?;
        if bytes.is_empty() || bytes.as_ref() == b"null" {
            return Ok(None);
        }
        serde_json::from_slice(&bytes).context("failed to parse current user")
    }

    pub async fn initialize_session(&self) -> Result<()> {
        let url = self.url("/api/v1/user");
        let response = self
            .http
            .get(url)
            .send()
            .await
            .context("failed to initialize login session")?;
        ensure_http_success(response, "initialize login session")
            .await
            .map(|_| ())
    }

    pub async fn send_verification_code(&self, target: &LoginTarget) -> Result<()> {
        let url = self.url("/api/v1/user/sendVerificationCode");
        let body = match target {
            LoginTarget::Email(email) => {
                serde_json::json!({ "type": "email", "email": email })
            }
            LoginTarget::Mobile(mobile) => {
                serde_json::json!({ "type": "mobile", "mobile": mobile })
            }
        };
        let response = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("failed to send verification code")?;
        ensure_http_success(response, "send verification code")
            .await
            .map(|_| ())
    }

    pub async fn verify_code(&self, target: &LoginTarget, code: &str) -> Result<LoginResponse> {
        let url = self.url("/api/v1/user/verify");
        let body = match target {
            LoginTarget::Email(email) => {
                serde_json::json!({ "type": "email", "email": email, "code": code })
            }
            LoginTarget::Mobile(mobile) => {
                serde_json::json!({ "type": "mobile", "mobile": mobile, "code": code })
            }
        };
        let response = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("failed to verify code")?;
        let response = ensure_http_success(response, "verify login code").await?;
        response
            .json()
            .await
            .context("failed to parse login response")
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

impl LoginTarget {
    pub fn from_parts(email: Option<String>, mobile: Option<String>) -> Result<Self> {
        match (email, mobile) {
            (Some(email), None) => Ok(LoginTarget::Email(email)),
            (None, Some(mobile)) => Ok(LoginTarget::Mobile(mobile)),
            _ => bail!("provide exactly one of --email or --mobile"),
        }
    }
}

impl SessionStore {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn load(&self) -> Result<Option<Session>> {
        if !self.paths.session_file.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.paths.session_file).with_context(|| {
            format!(
                "failed to read session {}",
                self.paths.session_file.display()
            )
        })?;
        if let Ok(session) = serde_json::from_str::<Session>(&raw) {
            self.save(&session)?;
            return Ok(Some(session));
        }

        let cache: UserSessionCache =
            serde_json::from_str(&raw).context("failed to parse session user cache")?;
        let Some(token) = load_token(&self.paths)? else {
            return Ok(None);
        };
        Ok(Some(Session {
            token,
            user: cache.user,
        }))
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        self.paths.ensure()?;
        save_token(&self.paths, &session.token)?;
        let raw = serde_json::to_string_pretty(&UserSessionCache {
            user: session.user.clone(),
        })
        .context("failed to serialize session user cache")?;
        fs::write(&self.paths.session_file, raw).with_context(|| {
            format!(
                "failed to write session {}",
                self.paths.session_file.display()
            )
        })?;
        restrict_file_permissions(&self.paths.session_file)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        clear_token(&self.paths)?;
        if self.paths.session_file.exists() {
            fs::remove_file(&self.paths.session_file).with_context(|| {
                format!(
                    "failed to remove session {}",
                    self.paths.session_file.display()
                )
            })?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn restrict_file_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn load_token(paths: &AppPaths) -> Result<Option<String>> {
    if !paths.token_file.exists() {
        return Ok(None);
    }
    let token = fs::read_to_string(&paths.token_file)
        .with_context(|| format!("failed to read token {}", paths.token_file.display()))?;
    let token = token.trim();
    if token.is_empty() {
        Ok(None)
    } else {
        Ok(Some(token.to_string()))
    }
}

fn save_token(paths: &AppPaths, token: &str) -> Result<()> {
    fs::write(&paths.token_file, token)
        .with_context(|| format!("failed to write token {}", paths.token_file.display()))?;
    restrict_file_permissions(&paths.token_file)
}

fn clear_token(paths: &AppPaths) -> Result<()> {
    if paths.token_file.exists() {
        fs::remove_file(&paths.token_file)
            .with_context(|| format!("failed to remove token {}", paths.token_file.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_paths() -> AppPaths {
        let root =
            std::env::temp_dir().join(format!("icoding-client-auth-test-{}", uuid::Uuid::new_v4()));
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

    fn test_session() -> Session {
        Session {
            token: "local-test-token".to_string(),
            user: User {
                id: 42,
                email: Some("test@example.com".to_string()),
                mobile: None,
                nicker: Some("tester".to_string()),
                avatar: None,
            },
        }
    }

    #[test]
    fn session_round_trip_uses_local_token_file() -> Result<()> {
        let paths = test_paths();
        let store = SessionStore::new(paths.clone());
        let session = test_session();

        store.save(&session)?;

        assert_eq!(fs::read_to_string(&paths.token_file)?, session.token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&paths.token_file)?.permissions().mode() & 0o777,
                0o600
            );
        }
        let loaded = store.load()?.expect("saved session should load");
        assert_eq!(loaded.token, session.token);
        assert_eq!(loaded.user.id, session.user.id);

        store.clear()?;
        assert!(!paths.token_file.exists());
        assert!(!paths.session_file.exists());
        let _ = fs::remove_dir_all(root_path(&paths));
        Ok(())
    }

    #[test]
    fn legacy_embedded_token_is_migrated_to_local_token_file() -> Result<()> {
        let paths = test_paths();
        paths.ensure()?;
        let session = test_session();
        fs::write(&paths.session_file, serde_json::to_string(&session)?)?;

        let loaded = SessionStore::new(paths.clone())
            .load()?
            .expect("legacy session should load");

        assert_eq!(loaded.token, session.token);
        assert_eq!(fs::read_to_string(&paths.token_file)?, session.token);
        let cached: UserSessionCache =
            serde_json::from_str(&fs::read_to_string(&paths.session_file)?)?;
        assert_eq!(cached.user.id, session.user.id);

        let _ = fs::remove_dir_all(root_path(&paths));
        Ok(())
    }

    fn root_path(paths: &AppPaths) -> PathBuf {
        paths
            .data_dir
            .parent()
            .expect("test data directory should have a parent")
            .to_path_buf()
    }
}
