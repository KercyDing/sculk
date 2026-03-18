//! sckc 命令行工具（CLI）。
//!
//! 用法：
//! - `sckc host`：创建房间并生成连接票据（ticket）
//! - `sckc join "<ticket>"`：通过票据加入房间（注意给 ticket 加引号）
//! - `sckc relay`：管理自定义 relay 配置

use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand};
use sculk::persist::{self, Profile};
use sculk::tunnel::{IrohTunnel, TunnelConfig, TunnelEvent};
use tracing_subscriber::EnvFilter;

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
    name = "sckc",
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
        #[arg(short, long, default_value_t = sculk::DEFAULT_MC_PORT)]
        port: u16,
        /// 强制重新生成新密钥
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
        #[arg(short, long, default_value_t = sculk::DEFAULT_INLET_PORT)]
        port: u16,
        /// 路径状态打印间隔（秒，默认 0: 仅变化时输出）
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// 连接密码
        #[arg(long)]
        password: Option<String>,
        /// 最大重连次数（默认无限）
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_ansi(true)
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let cli = Cli::parse();
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
            let path = match key_path {
                Some(path) => path,
                None => persist::default_key_path()?,
            };
            let secret_key = if new_key {
                persist::generate_new_key(&path)?
            } else {
                persist::load_or_generate_key(&path)?
            };
            tracing::info!(key_path = %path.display(), "using secret key");

            let profile = Profile::load()?;
            let relay_url = profile.resolve_relay_url(relay.as_deref())?;
            let config = TunnelConfig::new()
                .event_delay(Duration::from_secs(delay))
                .password(password)
                .max_players(max_players);

            let (tunnel, ticket, mut events) =
                IrohTunnel::host(port, Some(secret_key), relay_url, config).await?;
            let ticket_str = ticket.to_string();
            let quoted = format!("\"{ticket_str}\"");
            println!("Ticket: {quoted}");

            if sculk::clipboard::clipboard_copy(&quoted) {
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
            let ticket: sculk::tunnel::Ticket =
                ticket.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(ref url) = ticket.relay_url {
                println!("Relay: {url}");
            }

            let config = TunnelConfig::new()
                .event_delay(Duration::from_secs(delay))
                .password(password)
                .max_retries(max_retries);

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
            let mut profile = Profile::load()?;
            if reset {
                profile.relay.custom = false;
                profile.save()?;
                println!("Reset to default n0 relay servers.");
            } else if let Some(url) = url {
                // 验证 URL 格式
                profile.resolve_relay_url(Some(&url))?;
                profile.relay.custom = true;
                profile.relay.url = Some(url.clone());
                profile.save()?;
                println!("Custom relay saved: {url}");
            } else if list {
                if profile.relay.custom {
                    match &profile.relay.url {
                        Some(url) => println!("Current relay: {url} (custom)"),
                        None => println!("Custom relay enabled but URL not set."),
                    }
                } else {
                    println!("Using default n0 relay servers.");
                }
            } else {
                let mut relay_subcommand = Cli::command()
                    .find_subcommand("relay")
                    .ok_or_else(|| anyhow::anyhow!("relay subcommand should exist"))?
                    .clone();
                relay_subcommand.print_help()?;
            }
        }
    }

    Ok(())
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
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands};

    #[test]
    fn parse_host_command_from_args() {
        let cli_res = Cli::try_parse_from(["sckc", "host", "-p", "25565"]);
        assert!(cli_res.is_ok(), "parse host");
        let cli = if let Ok(v) = cli_res { v } else { return };
        assert!(matches!(cli.command, Commands::Host { port: 25565, .. }));
    }

    #[test]
    fn parse_join_defaults() {
        let cli_res = Cli::try_parse_from(["sckc", "join", "ticket"]);
        assert!(cli_res.is_ok(), "parse join");
        let cli = if let Ok(v) = cli_res { v } else { return };
        assert!(
            matches!(cli.command, Commands::Join { port, .. } if port == sculk::DEFAULT_INLET_PORT)
        );
    }
}
