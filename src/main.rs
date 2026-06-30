use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use icoding_client::{
    auth::{AuthClient, LoginTarget, Session, SessionStore},
    capabilities::CapabilityDispatcher,
    config::{AppConfig, AppPaths},
    device::{DeviceClient, DeviceRegisterRequest},
    ws::AgentWsClient,
};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "icoding-client")]
#[command(about = "Desktop agent client core")]
struct Cli {
    #[arg(long, env = "ICODING_API_BASE_URL")]
    api_base_url: Option<String>,

    #[arg(long, env = "ICODING_WS_URL")]
    ws_url: Option<String>,

    #[arg(long)]
    save_server: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Desktop,
    ConfigPath,
    Whoami,
    SendCode {
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        mobile: Option<String>,
    },
    VerifyCode {
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        mobile: Option<String>,
        #[arg(long)]
        code: String,
    },
    Logout,
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },
    RegisterDevice,
    Serve {
        #[arg(long)]
        skip_device_register: bool,
    },
    FsList {
        path: PathBuf,
        #[arg(long)]
        recursive: bool,
    },
    FsRead {
        path: PathBuf,
    },
    Exec {
        #[arg(long)]
        cwd: PathBuf,
        command: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum PolicyCommand {
    Show,
    SetRoots {
        roots: Vec<PathBuf>,
    },
    AddRoot {
        root: PathBuf,
    },
    RemoveRoot {
        root: PathBuf,
    },
    Shell {
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "disable")]
        enable: bool,
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "enable")]
        disable: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "icoding_client=info,warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let paths = AppPaths::resolve()?;
    let mut config = AppConfig::load_or_create(&paths)?;
    config.apply_server_override(cli.api_base_url.clone(), cli.ws_url.clone());
    if cli.save_server {
        config.save(&paths)?;
    }

    let session_store = SessionStore::new(paths.clone());

    match cli.command.unwrap_or(default_command()) {
        Commands::Desktop => {
            run_desktop_or_service(paths, config).await?;
        }
        Commands::ConfigPath => {
            print_json(&json!({
                "config_file": paths.config_file,
                "session_file": paths.session_file,
                "token_file": paths.token_file,
                "log_dir": paths.log_dir
            }))?;
        }
        Commands::Whoami => {
            let auth = AuthClient::new(config.server.api_base_url.clone())?;
            let session = session_store.load()?;
            let user = auth
                .current_user(session.as_ref().map(|session| session.token.as_str()))
                .await?;
            print_json(&json!({ "user": user }))?;
        }
        Commands::SendCode { email, mobile } => {
            let target = LoginTarget::from_parts(email, mobile)?;
            let auth = AuthClient::new(config.server.api_base_url.clone())?;
            auth.initialize_session().await?;
            auth.send_verification_code(&target).await?;
            println!("verification code sent");
        }
        Commands::VerifyCode {
            email,
            mobile,
            code,
        } => {
            let target = LoginTarget::from_parts(email, mobile)?;
            let auth = AuthClient::new(config.server.api_base_url.clone())?;
            let login = auth.verify_code(&target, &code).await?;
            let session = Session {
                token: login.token,
                user: login.user,
            };
            session_store.save(&session)?;
            print_json(&json!({
                "saved": true,
                "user": session.user
            }))?;
        }
        Commands::Logout => {
            session_store.clear()?;
            println!("logged out");
        }
        Commands::Policy { command } => {
            handle_policy_command(command, &mut config, &paths)?;
        }
        Commands::RegisterDevice => {
            let session = require_session(&session_store)?;
            let request = DeviceRegisterRequest::from_config(&config, session.user);
            let response = DeviceClient::new(config.server.api_base_url.clone())
                .register(&session.token, &request)
                .await?;
            print_json(&serde_json::to_value(response)?)?;
        }
        Commands::Serve {
            skip_device_register,
        } => {
            let session = require_session(&session_store)?;
            let mut ws_url = None;
            let mut connection_token = None;

            if !skip_device_register {
                let request = DeviceRegisterRequest::from_config(&config, session.user.clone());
                let response = DeviceClient::new(config.server.api_base_url.clone())
                    .register(&session.token, &request)
                    .await?;
                ws_url = response.ws_url.filter(|url| !url.trim().is_empty());
                connection_token = response
                    .connection_token
                    .filter(|token| !token.trim().is_empty());
            }

            AgentWsClient::new(config, session, ws_url, connection_token)
                .run_forever()
                .await?;
        }
        Commands::FsList { path, recursive } => {
            let dispatcher = CapabilityDispatcher::new(config);
            let output = dispatcher
                .dispatch(
                    "fs.list",
                    json!({
                        "path": path,
                        "recursive": recursive
                    }),
                )
                .await?;
            print_json(&output.result)?;
        }
        Commands::FsRead { path } => {
            let dispatcher = CapabilityDispatcher::new(config);
            let output = dispatcher
                .dispatch(
                    "fs.read",
                    json!({
                        "path": path
                    }),
                )
                .await?;
            print_json(&output.result)?;
        }
        Commands::Exec { cwd, command } => {
            let command = command.join(" ");
            if command.trim().is_empty() {
                anyhow::bail!("command is required");
            }
            let dispatcher = CapabilityDispatcher::new(config);
            let output = dispatcher
                .dispatch(
                    "process.exec",
                    json!({
                        "cwd": cwd,
                        "command": command
                    }),
                )
                .await?;
            print_json(&output.result)?;
        }
    }

    Ok(())
}

fn handle_policy_command(
    command: PolicyCommand,
    config: &mut AppConfig,
    paths: &AppPaths,
) -> Result<()> {
    match command {
        PolicyCommand::Show => {
            print_json(&json!({
                "allowed_roots": config.policy.allowed_roots,
                "blocked_paths": config.policy.blocked_paths,
                "shell_exec_enabled": config.policy.shell_exec_enabled,
                "max_file_read_bytes": config.policy.max_file_read_bytes,
                "max_file_write_bytes": config.policy.max_file_write_bytes,
                "max_command_output_bytes": config.policy.max_command_output_bytes,
                "default_command_timeout_seconds": config.policy.default_command_timeout_seconds
            }))?;
        }
        PolicyCommand::SetRoots { roots } => {
            if roots.is_empty() {
                anyhow::bail!("provide at least one root directory");
            }
            config.policy.allowed_roots = canonicalize_roots(roots)?;
            config.save(paths)?;
            print_policy_saved(config)?;
        }
        PolicyCommand::AddRoot { root } => {
            let root = canonicalize_root(&root)?;
            if !config.policy.allowed_roots.iter().any(|item| item == &root) {
                config.policy.allowed_roots.push(root);
            }
            config.save(paths)?;
            print_policy_saved(config)?;
        }
        PolicyCommand::RemoveRoot { root } => {
            let root = canonicalize_root(&root)?;
            config.policy.allowed_roots.retain(|item| item != &root);
            if config.policy.allowed_roots.is_empty() {
                anyhow::bail!("allowed roots cannot be empty");
            }
            config.save(paths)?;
            print_policy_saved(config)?;
        }
        PolicyCommand::Shell { enable, disable } => {
            if enable {
                config.policy.shell_exec_enabled = true;
            }
            if disable {
                config.policy.shell_exec_enabled = false;
            }
            config.save(paths)?;
            print_policy_saved(config)?;
        }
    }

    Ok(())
}

fn canonicalize_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    roots
        .into_iter()
        .map(|root| canonicalize_root(&root))
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

fn print_policy_saved(config: &AppConfig) -> Result<()> {
    print_json(&json!({
        "saved": true,
        "policy": {
            "allowed_roots": config.policy.allowed_roots,
            "shell_exec_enabled": config.policy.shell_exec_enabled
        }
    }))
}

fn default_command() -> Commands {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Commands::Desktop
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Commands::Serve {
            skip_device_register: false,
        }
    }
}

async fn run_desktop_or_service(paths: AppPaths, config: AppConfig) -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        icoding_client::desktop::run_desktop(paths, config)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let session_store = SessionStore::new(paths);
        let session = require_session(&session_store)?;
        AgentWsClient::new(config, session, None, None)
            .run_forever()
            .await
    }
}

fn require_session(store: &SessionStore) -> Result<Session> {
    store
        .load()?
        .context("not logged in; run verify-code after sending a verification code")
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
