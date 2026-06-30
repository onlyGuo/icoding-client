use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command, CommandFactory, FromArgMatches, Parser, Subcommand};
use icoding_client::{
    auth::{AuthClient, LoginTarget, Session, SessionStore},
    capabilities::CapabilityDispatcher,
    config::{AppConfig, AppPaths},
    device::{DeviceClient, DeviceRegisterRequest},
    i18n::Language,
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

    let language = Language::detect();
    let cli = parse_cli(language);
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
            println!(
                "{}",
                language.select("Verification code sent.", "验证码已发送。")
            );
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
            println!("{}", language.select("Logged out.", "已退出登录。"));
        }
        Commands::Policy { command } => {
            handle_policy_command(command, &mut config, &paths, language)?;
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
                anyhow::bail!(language.select("command is required", "必须提供命令"));
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
    language: Language,
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
                anyhow::bail!(language.select(
                    "provide at least one root directory",
                    "请至少提供一个根目录"
                ));
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
                anyhow::bail!(language.select("allowed roots cannot be empty", "允许目录不能为空"));
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

fn parse_cli(language: Language) -> Cli {
    let matches = localized_cli_command(language).get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
}

fn localized_cli_command(language: Language) -> Command {
    let text = |english, chinese| language.select(english, chinese);
    let command = Cli::command()
        .about(text(
            "Desktop agent client for iCoding",
            "iCoding 桌面智能体客户端",
        ))
        .mut_arg("api_base_url", |arg| {
            arg.help(text("Override the API base URL", "覆盖 API 基础地址"))
        })
        .mut_arg("ws_url", |arg| {
            arg.help(text("Override the WebSocket URL", "覆盖 WebSocket 地址"))
        })
        .mut_arg("save_server", |arg| {
            arg.help(text("Save server URL overrides", "保存服务器地址覆盖配置"))
        })
        .mut_subcommand("desktop", |command| {
            command.about(text("Start the desktop application", "启动桌面应用"))
        })
        .mut_subcommand("config-path", |command| {
            command.about(text("Show local configuration paths", "显示本地配置路径"))
        })
        .mut_subcommand("whoami", |command| {
            command.about(text("Show the current signed-in user", "显示当前登录用户"))
        })
        .mut_subcommand("send-code", |command| {
            command.about(text("Send a login verification code", "发送登录验证码"))
        })
        .mut_subcommand("verify-code", |command| {
            command.about(text(
                "Verify a login code and save the session",
                "验证登录码并保存会话",
            ))
        })
        .mut_subcommand("logout", |command| {
            command.about(text("Clear the local session", "清除本地登录会话"))
        })
        .mut_subcommand("policy", |command| {
            command
                .about(text(
                    "View or update the local policy",
                    "查看或修改本地策略",
                ))
                .mut_subcommand("show", |command| {
                    command.about(text("Show the current policy", "显示当前策略"))
                })
                .mut_subcommand("set-roots", |command| {
                    command.about(text("Replace allowed roots", "替换允许目录"))
                })
                .mut_subcommand("add-root", |command| {
                    command.about(text("Add an allowed root", "添加允许目录"))
                })
                .mut_subcommand("remove-root", |command| {
                    command.about(text("Remove an allowed root", "移除允许目录"))
                })
                .mut_subcommand("shell", |command| {
                    command.about(text(
                        "Enable or disable command execution",
                        "启用或禁用命令执行",
                    ))
                })
        })
        .mut_subcommand("register-device", |command| {
            command.about(text("Register this device", "注册当前设备"))
        })
        .mut_subcommand("serve", |command| {
            command.about(text("Run the agent service", "运行智能体服务"))
        })
        .mut_subcommand("fs-list", |command| {
            command.about(text("List a directory", "列出目录内容"))
        })
        .mut_subcommand("fs-read", |command| {
            command.about(text("Read a file", "读取文件"))
        })
        .mut_subcommand("exec", |command| {
            command.about(text("Execute a command", "执行命令"))
        });

    if language == Language::Chinese {
        localize_chinese_help(command)
    } else {
        command
    }
}

fn localize_chinese_help(command: Command) -> Command {
    let has_subcommands = command.get_subcommands().next().is_some();
    let template = if has_subcommands {
        "{about-with-newline}\n用法: {usage}\n\n命令:\n{subcommands}\n\n选项:\n{options}"
    } else {
        "{about-with-newline}\n用法: {usage}\n\n参数:\n{positionals}\n\n选项:\n{options}"
    };

    command
        .disable_help_subcommand(true)
        .disable_help_flag(true)
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .action(ArgAction::Help)
                .help("显示帮助"),
        )
        .help_template(template)
        .mut_subcommands(localize_chinese_help)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_help_defaults_to_english() {
        let help = localized_cli_command(Language::English)
            .render_long_help()
            .to_string();
        assert!(help.contains("Desktop agent client for iCoding"));
        assert!(help.contains("Start the desktop application"));
    }

    #[test]
    fn cli_help_supports_chinese() {
        let help = localized_cli_command(Language::Chinese)
            .render_long_help()
            .to_string();
        assert!(help.contains("iCoding 桌面智能体客户端"));
        assert!(help.contains("启动桌面应用"));
    }
}
