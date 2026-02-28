//! sculk 命令行工具（CLI）。
//!
//! 用法：
//! - `sculk host`：创建房间并生成连接票据（ticket）
//! - `sculk join "<ticket>"`：通过票据加入房间（注意给 ticket 加引号）
//! - `sculk relay`：管理自定义 relay 配置

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand};
use sculk_core::tunnel::{IrohTunnel, RelayUrl, TunnelConfig, TunnelEvent};
use tracing_subscriber::EnvFilter;

use crate::{key, relay};

const CLAP_STYLES: clap::builder::styling::Styles = clap::builder::styling::Styles::styled()
    .header(clap::builder::styling::AnsiColor::Yellow.on_default())
    .usage(clap::builder::styling::AnsiColor::Green.on_default())
    .literal(clap::builder::styling::AnsiColor::Cyan.on_default())
    .placeholder(clap::builder::styling::AnsiColor::Green.on_default())
    .valid(clap::builder::styling::AnsiColor::Green.on_default())
    .invalid(clap::builder::styling::AnsiColor::Red.on_default())
    .error(clap::builder::styling::AnsiColor::Red.on_default().bold());

#[derive(Parser)]
#[command(
    name = "sculk",
    version,
    about = "Minecraft 多人联机 P2P 隧道",
    arg_required_else_help = true,
    color = clap::ColorChoice::Always,
    styles = CLAP_STYLES
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 作为房主启动并暴露本地 MC 服务端
    Host {
        /// 本地 Minecraft 服务端端口
        #[arg(short, long, default_value_t = sculk_core::DEFAULT_MC_PORT)]
        port: u16,
        /// 强制生成新密钥（会改变 ticket）
        #[arg(long)]
        new_key: bool,
        /// 自定义密钥文件路径
        #[arg(long)]
        key_path: Option<PathBuf>,
        /// 覆盖 relay 地址（优先级高于配置文件）
        #[arg(short, long)]
        relay: Option<String>,
        /// 路径状态打印间隔（秒，0 = 仅变化时输出）
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// 连接密码
        #[arg(long)]
        password: Option<String>,
        /// 最大玩家数
        #[arg(long)]
        max_players: Option<u32>,
    },
    /// 通过票据加入房主房间
    Join {
        /// 房主提供的 ticket
        ticket: String,
        /// 本地给 MC 客户端连接的端口
        #[arg(short, long, default_value_t = sculk_core::DEFAULT_INLET_PORT)]
        port: u16,
        /// 路径状态打印间隔（秒，0 = 仅变化时输出）
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// 连接密码
        #[arg(long)]
        password: Option<String>,
        /// 最大重连次数（不传则无限重连）
        #[arg(long)]
        max_retries: Option<u32>,
    },
    /// 管理自定义 relay 配置
    Relay {
        /// 设置自定义 relay 地址
        #[arg(long)]
        url: Option<String>,
        /// 显示当前 relay 配置
        #[arg(long)]
        list: bool,
        /// 重置为默认 n0 relay
        #[arg(long)]
        reset: bool,
    },
}

pub async fn run_cli() -> anyhow::Result<()> {
    run_cli_with_args(std::env::args_os()).await
}

pub async fn run_cli_with_args<I, T>(args: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let _ = tracing_subscriber::fmt()
        .with_ansi(true)
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let cli = Cli::parse_from(args);
    run_command(cli).await
}

async fn run_command(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Host {
            port,
            new_key,
            key_path,
            relay,
            delay,
            password,
            max_players,
        } => {
            let path = key_path.unwrap_or_else(default_key_path);
            let secret_key = if new_key {
                key::generate_new_key(&path)?
            } else {
                key::load_or_generate_key(&path)?
            };
            tracing::info!(key_path = %path.display(), "using secret key");

            let relay_url = resolve_relay_url(relay.as_deref())?;
            let config = TunnelConfig {
                event_delay: Duration::from_secs(delay),
                password,
                max_players,
                ..Default::default()
            };

            let (tunnel, ticket, mut events) =
                IrohTunnel::host(port, Some(secret_key), relay_url, config).await?;
            let ticket_str = ticket.to_string();
            let quoted = format!("\"{ticket_str}\"");
            println!("Ticket: {quoted}");

            if clipboard_copy(&quoted) {
                println!("(Copied to clipboard)");
            }

            println!("Share this ticket with players.");
            println!("Press Ctrl+C to stop.");

            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);
            loop {
                tokio::select! {
                    _ = &mut ctrl_c => break,
                    event = events.recv() => match event {
                        Some(e) => print_event(&e),
                        None => break,
                    },
                }
            }

            tunnel.close().await;
        }
        Commands::Join {
            ticket,
            port,
            delay,
            password,
            max_retries,
        } => {
            let ticket: sculk_core::tunnel::Ticket =
                ticket.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(ref url) = ticket.relay_url {
                println!("Relay: {url}");
            }

            let config = TunnelConfig {
                event_delay: Duration::from_secs(delay),
                password,
                max_retries,
                ..Default::default()
            };

            let (tunnel, mut events) = IrohTunnel::join(&ticket, port, config).await?;
            println!("Tunnel running. Connect MC client to 127.0.0.1:{port}");
            println!("Press Ctrl+C to stop.");

            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);
            loop {
                tokio::select! {
                    _ = &mut ctrl_c => break,
                    event = events.recv() => match event {
                        Some(e) => print_event(&e),
                        None => break,
                    },
                }
            }

            tunnel.close().await;
        }
        Commands::Relay { url, list, reset } => {
            let conf = default_relay_conf_path();
            if reset {
                relay::remove_relay_config(&conf)?;
                println!("Reset to default n0 relay servers.");
            } else if let Some(url) = url {
                relay::save_relay_url(&conf, &url)?;
                println!("Custom relay saved: {url}");
            } else if list {
                match relay::load_relay_url(&conf)? {
                    Some(url) => println!("Current relay: {url}"),
                    None => println!("Using default n0 relay servers."),
                }
            } else {
                Cli::command()
                    .find_subcommand("relay")
                    .expect("relay subcommand should exist")
                    .clone()
                    .print_help()?;
            }
        }
    }

    Ok(())
}

fn default_key_path() -> PathBuf {
    dirs::data_dir()
        .expect("cannot determine system data directory")
        .join("sculk")
        .join("secret.key")
}

fn default_relay_conf_path() -> PathBuf {
    dirs::data_dir()
        .expect("cannot determine system data directory")
        .join("sculk")
        .join("relay.conf")
}

/// 解析 relay 地址，优先级：命令行 `--relay` > 配置文件 > `None`（默认 n0）
fn resolve_relay_url(flag: Option<&str>) -> anyhow::Result<Option<RelayUrl>> {
    if let Some(url_str) = flag {
        let url: RelayUrl = url_str
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid relay URL: {e}"))?;
        return Ok(Some(url));
    }
    relay::load_relay_url(&default_relay_conf_path())
}

fn print_event(event: &TunnelEvent) {
    match event {
        TunnelEvent::PlayerJoined { id } => println!("[+] Player joined: {id}"),
        TunnelEvent::PlayerLeft { id, reason } => println!("[-] Player left: {id} ({reason})"),
        TunnelEvent::Connected => println!("[*] Connected to host"),
        TunnelEvent::Disconnected { reason } => println!("[!] Disconnected: {reason}"),
        TunnelEvent::PathChanged {
            remote_id,
            is_relay,
            rtt_ms,
        } => {
            let mode = if *is_relay { "relay" } else { "direct" };
            println!("[~] {remote_id} path: {mode}, RTT: {rtt_ms}ms");
        }
        TunnelEvent::Error { message } => eprintln!("[!] Error: {message}"),
        TunnelEvent::Reconnecting { attempt } => {
            println!("[~] Reconnecting (attempt {attempt})...")
        }
        TunnelEvent::Reconnected => println!("[*] Reconnected to host"),
        TunnelEvent::AuthFailed { id } => println!("[!] Auth failed: {id}"),
        TunnelEvent::PlayerRejected { id, reason } => {
            println!("[-] Player rejected: {id} ({reason})")
        }
    }
}

/// 复制文本到系统剪贴板。
///
/// Linux Wayland 下优先使用 `wl-copy`（fork 后台进程持有内容），
/// 其他平台回退到 arboard。
fn clipboard_copy(text: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && let Ok(mut child) = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().is_ok_and(|s| s.success());
        }

        if std::env::var_os("DISPLAY").is_some()
            && let Ok(mut child) = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().is_ok_and(|s| s.success());
        }
    }

    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands};

    #[test]
    fn parse_host_command_from_args() {
        let cli = Cli::try_parse_from(["sculk", "host", "-p", "25565"]).expect("parse host");
        assert!(matches!(cli.command, Commands::Host { port: 25565, .. }));
    }

    #[test]
    fn parse_join_defaults() {
        let cli = Cli::try_parse_from(["sculk", "join", "ticket"]).expect("parse join");
        assert!(
            matches!(cli.command, Commands::Join { port, .. } if port == sculk_core::DEFAULT_INLET_PORT)
        );
    }
}
