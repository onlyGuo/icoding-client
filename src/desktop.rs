#![cfg(any(target_os = "macos", target_os = "windows"))]

use crate::{
    auth::{AuthClient, LoginTarget, Session, SessionStore, User},
    config::{AppConfig, AppPaths},
    device::{DeviceClient, DeviceRegisterRequest},
    permissions::{
        full_disk_access_status, open_full_disk_access_settings, require_startup_permissions,
    },
    ws::AgentWsClient,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{
    AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tokio::task::JoinHandle;
use tracing::{error, warn};

#[derive(Clone)]
struct DesktopState {
    paths: AppPaths,
    config: Arc<Mutex<AppConfig>>,
    agent_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[derive(Debug, Serialize)]
struct UiStatus {
    logged_in: bool,
    user: Option<User>,
    server: UiServerConfig,
    device_id: String,
    policy: UiPolicyConfig,
    agent_running: bool,
    auto_start_enabled: bool,
    permissions: UiPermissionStatus,
}

#[derive(Debug, Serialize)]
struct UiPermissionStatus {
    full_disk_access_required: bool,
    full_disk_access_granted: bool,
    detail: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiServerConfig {
    api_base_url: String,
    ws_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UiPolicyConfig {
    allowed_roots: Vec<String>,
    shell_exec_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendCodeRequest {
    login_type: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyCodeRequest {
    login_type: String,
    value: String,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerConfigRequest {
    api_base_url: String,
    ws_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyConfigRequest {
    allowed_roots: Vec<String>,
    shell_exec_enabled: bool,
}

pub fn run_desktop(paths: AppPaths, config: AppConfig) -> Result<()> {
    let state = DesktopState {
        paths,
        config: Arc::new(Mutex::new(config)),
        agent_task: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--desktop"]),
        ))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            update_server_config,
            update_policy_config,
            request_full_disk_access,
            send_code,
            verify_code,
            start_agent,
            logout
        ])
        .setup(|app| {
            ensure_main_window(app.handle())?;
            setup_tray(app.handle())?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = start_agent_from_handle(&handle).await {
                    warn!(?error, "agent did not start automatically");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("failed to run desktop app")
}

#[tauri::command]
async fn get_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> Result<UiStatus, String> {
    let config = state.config.lock().map_err(display_error)?.clone();
    let session = SessionStore::new(state.paths.clone())
        .load()
        .map_err(display_error)?;
    let auto_start_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let permissions = full_disk_access_status();
    Ok(UiStatus {
        logged_in: session.is_some(),
        user: session.map(|session| session.user),
        server: UiServerConfig {
            api_base_url: config.server.api_base_url,
            ws_url: config.server.ws_url,
        },
        device_id: config.client.device_id,
        policy: UiPolicyConfig {
            allowed_roots: config
                .policy
                .allowed_roots
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            shell_exec_enabled: config.policy.shell_exec_enabled,
        },
        agent_running: state
            .agent_task
            .lock()
            .map_err(display_error)?
            .as_ref()
            .is_some_and(|task| !task.is_finished()),
        auto_start_enabled,
        permissions: UiPermissionStatus {
            full_disk_access_required: permissions.required,
            full_disk_access_granted: permissions.granted,
            detail: permissions.detail,
        },
    })
}

#[tauri::command]
fn request_full_disk_access() -> Result<(), String> {
    open_full_disk_access_settings().map_err(display_error)
}

#[tauri::command]
async fn update_server_config(
    state: State<'_, DesktopState>,
    request: ServerConfigRequest,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(display_error)?;
    config.server.api_base_url = request.api_base_url.trim_end_matches('/').to_string();
    config.server.ws_url = request.ws_url;
    config.save(&state.paths).map_err(display_error)
}

#[tauri::command]
async fn update_policy_config<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: PolicyConfigRequest,
) -> Result<(), String> {
    let was_running = state
        .agent_task
        .lock()
        .map_err(display_error)?
        .as_ref()
        .is_some_and(|task| !task.is_finished());

    {
        let mut config = state.config.lock().map_err(display_error)?;
        config.policy.allowed_roots =
            canonicalize_roots(request.allowed_roots).map_err(display_error)?;
        config.policy.shell_exec_enabled = request.shell_exec_enabled;
        config.save(&state.paths).map_err(display_error)?;
    }

    if was_running {
        stop_agent(&state).map_err(display_error)?;
        start_agent_from_handle(&app).await.map_err(display_error)?;
    }

    Ok(())
}

#[tauri::command]
async fn send_code(state: State<'_, DesktopState>, request: SendCodeRequest) -> Result<(), String> {
    let config = state.config.lock().map_err(display_error)?.clone();
    let target = login_target(request.login_type, request.value).map_err(display_error)?;
    let auth = AuthClient::new(config.server.api_base_url).map_err(display_error)?;
    auth.initialize_session().await.map_err(display_error)?;
    auth.send_verification_code(&target)
        .await
        .map_err(display_error)
}

#[tauri::command]
async fn verify_code<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: VerifyCodeRequest,
) -> Result<User, String> {
    let config = state.config.lock().map_err(display_error)?.clone();
    let target = login_target(request.login_type, request.value).map_err(display_error)?;
    let auth = AuthClient::new(config.server.api_base_url).map_err(display_error)?;
    let login = auth
        .verify_code(&target, &request.code)
        .await
        .map_err(display_error)?;
    let session = Session {
        token: login.token,
        user: login.user.clone(),
    };
    SessionStore::new(state.paths.clone())
        .save(&session)
        .map_err(display_error)?;
    if config.client.auto_start {
        let _ = app.autolaunch().enable();
    }
    if full_disk_access_status().granted {
        start_agent_from_handle(&app).await.map_err(display_error)?;
    } else {
        open_full_disk_access_settings().map_err(display_error)?;
        warn!("login succeeded; agent is waiting for Full Disk Access before starting");
    }
    Ok(login.user)
}

#[tauri::command]
async fn start_agent<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    start_agent_from_handle(&app).await.map_err(display_error)
}

#[tauri::command]
async fn logout<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    stop_agent(&state).map_err(display_error)?;
    SessionStore::new(state.paths.clone())
        .clear()
        .map_err(display_error)?;
    let _ = app.autolaunch().disable();
    show_main_window(&app).map_err(display_error)
}

async fn start_agent_from_handle<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    require_startup_permissions(true)?;
    let state = app.state::<DesktopState>();
    if state
        .agent_task
        .lock()
        .map_err(|_| anyhow::anyhow!("agent state lock poisoned"))?
        .as_ref()
        .is_some_and(|task| !task.is_finished())
    {
        return Ok(());
    }

    let config = state
        .config
        .lock()
        .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
        .clone();
    let session = SessionStore::new(state.paths.clone())
        .load()?
        .context("not logged in")?;

    let handle = tokio::spawn(async move {
        let mut ws_url = None;
        let mut connection_token = None;
        let register_request = DeviceRegisterRequest::from_config(&config, session.user.clone());
        match DeviceClient::new(config.server.api_base_url.clone())
            .register(&session.token, &register_request)
            .await
        {
            Ok(response) => {
                ws_url = response.ws_url.filter(|url| !url.trim().is_empty());
                connection_token = response
                    .connection_token
                    .filter(|token| !token.trim().is_empty());
            }
            Err(error) => {
                warn!(
                    ?error,
                    "device register failed; trying configured websocket url"
                );
            }
        }

        if let Err(error) = AgentWsClient::new(config, session, ws_url, connection_token)
            .run_forever()
            .await
        {
            error!(?error, "agent websocket loop stopped");
        }
    });

    *state
        .agent_task
        .lock()
        .map_err(|_| anyhow::anyhow!("agent state lock poisoned"))? = Some(handle);
    Ok(())
}

fn stop_agent(state: &DesktopState) -> Result<()> {
    if let Some(handle) = state
        .agent_task
        .lock()
        .map_err(|_| anyhow::anyhow!("agent state lock poisoned"))?
        .take()
    {
        handle.abort();
    }
    Ok(())
}

fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let open = MenuItem::with_id(app, "open", "打开窗口", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "状态：运行中", false, None::<&str>)?;
    let logout = MenuItem::with_id(app, "logout", "退出登录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出程序", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &status, &separator, &logout, &quit])?;

    TrayIconBuilder::with_id("main")
        .tooltip("iCoding Client")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                let _ = show_main_window(app);
            }
            "logout" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = handle.try_state::<DesktopState>() {
                        let _ = stop_agent(&state);
                        let _ = SessionStore::new(state.paths.clone()).clear();
                    }
                    let _ = show_main_window(&handle);
                });
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn ensure_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    if app.get_webview_window("main").is_none() {
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("iCoding Client")
            .inner_size(1227.0, 680.0)
            .min_inner_size(760.0, 560.0)
            .build()?;
    }
    Ok(())
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    ensure_main_window(app)?;
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

fn login_target(login_type: String, value: String) -> Result<LoginTarget> {
    match login_type.as_str() {
        "email" => Ok(LoginTarget::Email(value)),
        "mobile" => Ok(LoginTarget::Mobile(value)),
        other => anyhow::bail!("unsupported login type: {other}"),
    }
}

fn canonicalize_roots(roots: Vec<String>) -> Result<Vec<PathBuf>> {
    if roots.is_empty() {
        anyhow::bail!("provide at least one allowed root");
    }
    roots
        .into_iter()
        .map(|root| canonicalize_root(Path::new(root.trim())))
        .collect()
}

fn canonicalize_root(root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve root {}", root.display()))?;
    if !root.is_dir() {
        anyhow::bail!("allowed root must be a directory: {}", root.display());
    }
    Ok(root)
}

fn display_error(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}
