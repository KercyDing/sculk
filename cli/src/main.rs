//! sckc 命令行工具（CLI）。
//!
//! 用法：
//! - `sckc host`：创建房间并生成连接票据（ticket）
//! - `sckc join "<ticket>"`：通过票据加入房间（注意给 ticket 加引号）
//! - `sckc relay`：管理自定义 relay 配置

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use sculk::persist::{self, Profile};
use sculk::tunnel::{
    HostConfig, HostOptions, JoinConfig, JoinOptions, TunnelEvent, TunnelMode, TunnelPhase,
    TunnelService, TunnelStatus, TunnelSubscription, TunnelUpdate,
};
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
    about = "P2P tunnel for Minecraft multiplayer",
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
    /// Host a room and expose a local Minecraft server
    Host {
        /// Local Minecraft server port
        #[arg(short, long, default_value_t = sculk::DEFAULT_MC_PORT)]
        port: u16,
        /// Generate and replace the secret key
        #[arg(long)]
        new_key: bool,
        /// Path to the secret key file
        #[arg(long)]
        key_path: Option<PathBuf>,
        /// Override the relay URL from the profile
        #[arg(short, long)]
        relay: Option<String>,
        /// Path status interval in seconds; 0 reports changes only
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// Require this connection password
        #[arg(long)]
        password: Option<String>,
        /// Maximum number of connected players
        #[arg(long)]
        max_players: Option<u32>,
    },
    /// Join a room with a connection ticket
    Join {
        /// Connection ticket provided by the host
        ticket: String,
        /// Local port for the Minecraft client
        #[arg(short, long, default_value_t = sculk::DEFAULT_INLET_PORT)]
        port: u16,
        /// Path status interval in seconds; 0 reports changes only
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// Connection password required by the host
        #[arg(long)]
        password: Option<String>,
        /// Maximum reconnection attempts; unlimited by default
        #[arg(long)]
        max_retries: Option<u32>,
    },
    /// Manage the relay configuration
    #[command(arg_required_else_help = true)]
    Relay {
        /// Set a custom relay URL
        #[arg(long)]
        url: Option<String>,
        /// Show the current relay configuration
        #[arg(long)]
        list: bool,
        /// Reset to the default n0 relay servers
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
    let service = TunnelService::new();
    let mut updates = service.subscribe();

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
            let config = HostConfig::new()
                .event_delay(Duration::from_secs(delay))
                .password(password)
                .max_players(max_players);

            service
                .start_host(
                    HostOptions::new(port)
                        .secret_key(Some(secret_key))
                        .relay_url(relay_url)
                        .config(config),
                )
                .await?;
            let status = wait_until_active(&mut updates, TunnelMode::Host).await?;
            let ticket = status
                .state
                .ticket
                .ok_or_else(|| anyhow::anyhow!("host started without a ticket"))?;
            let ticket_str = ticket.to_string();
            let quoted = format!("\"{ticket_str}\"");
            let ticket_style = *CLAP_STYLES.get_literal();
            println!(
                "Ticket: {}{quoted}{}",
                ticket_style.render(),
                ticket_style.render_reset()
            );

            if sculk::clipboard::clipboard_copy(&quoted) {
                println!("(Copied to clipboard)");
            }

            let hint_style = *CLAP_STYLES.get_valid();
            println!(
                "{}Share this ticket with players.{}",
                hint_style.render(),
                hint_style.render_reset()
            );
            println!("Press Ctrl+C to stop.");

            wait_for_shutdown(&service, updates).await?;
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

            let config = JoinConfig::new()
                .event_delay(Duration::from_secs(delay))
                .password(password)
                .max_retries(max_retries);

            service
                .start_join(JoinOptions::new(ticket, port).config(config))
                .await?;
            wait_until_active(&mut updates, TunnelMode::Join).await?;
            println!("Tunnel running. Connect MC client to 127.0.0.1:{port}");
            println!("Press Ctrl+C to stop.");

            wait_for_shutdown(&service, updates).await?;
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
            }
        }
    }

    Ok(())
}

async fn wait_until_active(
    updates: &mut TunnelSubscription,
    expected_mode: TunnelMode,
) -> anyhow::Result<TunnelStatus> {
    while let Some(update) = updates.recv().await {
        match update {
            TunnelUpdate::Status(status)
                if status.state.phase == TunnelPhase::Active
                    && status.state.mode == Some(expected_mode) =>
            {
                return Ok(status);
            }
            TunnelUpdate::Status(status) if status.state.phase == TunnelPhase::Idle => {
                anyhow::bail!("tunnel stopped before becoming active");
            }
            TunnelUpdate::Event(TunnelEvent::Error { message }) => {
                anyhow::bail!("{message}");
            }
            TunnelUpdate::Event(event) => print_event(&event),
            _ => {}
        }
    }
    anyhow::bail!("tunnel service closed before becoming active")
}

async fn wait_for_shutdown(
    service: &TunnelService,
    mut updates: TunnelSubscription,
) -> anyhow::Result<()> {
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut interrupted = false;
    loop {
        tokio::select! {
            result = &mut ctrl_c => {
                result?;
                interrupted = true;
                break;
            }
            update = updates.recv() => {
                match update {
                    Some(TunnelUpdate::Status(status))
                        if status.state.phase == TunnelPhase::Idle => break,
                    Some(TunnelUpdate::Status(_)) => {}
                    Some(TunnelUpdate::Event(event)) => print_event(&event),
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }
    service.shutdown().await?;
    if interrupted {
        let closed_style = *CLAP_STYLES.get_error();
        println!(
            "\n{}Closed.{}",
            closed_style.render(),
            closed_style.render_reset()
        );
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
