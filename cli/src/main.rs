//! sculk command-line client.
//!
//! Commands:
//! - `sculk host`: host a room and generate a share URI
//! - `sculk join "<uri>"`: join a room through its share URI
//! - `sculk relay`: manage custom relay configuration

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use sculk::persist::{self, HostState, Profile, TokenRefreshSetting};
use sculk::tunnel::{
    HostConfig, HostedServiceHandle, HostedServiceOptions, JoinConfig, JoinOptions, JoinUri,
    LocalPort, NodeOptions, SculkNode, SecretKey, ServiceId, TokenRefreshPolicy, TunnelEvent,
    TunnelMode, TunnelPhase, TunnelService, TunnelStatus, TunnelSubscription, TunnelUpdate,
};
use tracing_subscriber::EnvFilter;

mod clipboard;

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
    about = "P2P tunnel for Minecraft multiplayer",
    arg_required_else_help = true,
    color = clap::ColorChoice::Always,
    styles = CLAP_STYLES
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TokenRefresh {
    #[value(name = "always")]
    Always,
    #[value(name = "never")]
    Never,
    #[value(name = "1h")]
    OneHour,
    #[value(name = "3h")]
    ThreeHours,
    #[value(name = "6h")]
    SixHours,
    #[value(name = "12h")]
    TwelveHours,
    #[value(name = "24h")]
    TwentyFourHours,
}

impl TokenRefresh {
    fn policy(self) -> TokenRefreshPolicy {
        match self {
            Self::Always => TokenRefreshPolicy::Always,
            Self::Never => TokenRefreshPolicy::Never,
            Self::OneHour => TokenRefreshPolicy::After(Duration::from_secs(60 * 60)),
            Self::ThreeHours => TokenRefreshPolicy::After(Duration::from_secs(3 * 60 * 60)),
            Self::SixHours => TokenRefreshPolicy::After(Duration::from_secs(6 * 60 * 60)),
            Self::TwelveHours => TokenRefreshPolicy::After(Duration::from_secs(12 * 60 * 60)),
            Self::TwentyFourHours => TokenRefreshPolicy::After(Duration::from_secs(24 * 60 * 60)),
        }
    }

    fn setting(self) -> TokenRefreshSetting {
        match self {
            Self::Always => TokenRefreshSetting::Always,
            Self::Never => TokenRefreshSetting::Never,
            Self::OneHour => TokenRefreshSetting::OneHour,
            Self::ThreeHours => TokenRefreshSetting::ThreeHours,
            Self::SixHours => TokenRefreshSetting::SixHours,
            Self::TwelveHours => TokenRefreshSetting::TwelveHours,
            Self::TwentyFourHours => TokenRefreshSetting::TwentyFourHours,
        }
    }
}

impl From<TokenRefreshSetting> for TokenRefresh {
    fn from(setting: TokenRefreshSetting) -> Self {
        match setting {
            TokenRefreshSetting::Always => Self::Always,
            TokenRefreshSetting::Never => Self::Never,
            TokenRefreshSetting::OneHour => Self::OneHour,
            TokenRefreshSetting::ThreeHours => Self::ThreeHours,
            TokenRefreshSetting::SixHours => Self::SixHours,
            TokenRefreshSetting::TwelveHours => Self::TwelveHours,
            TokenRefreshSetting::TwentyFourHours => Self::TwentyFourHours,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Host a room and expose a local Minecraft server
    Host {
        /// Local Minecraft server port
        #[arg(short, long, default_value_t = sculk::DEFAULT_MC_PORT)]
        port: u16,
        /// Override the relay URL from the profile
        #[arg(short, long)]
        relay: Option<String>,
        /// Path status interval in seconds
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// Share URI refresh policy
        #[arg(
            short = 't',
            long = "time",
            value_enum,
            value_name = "TIME",
            hide_possible_values = true
        )]
        token_refresh: Option<TokenRefresh>,
        /// Generate and replace the secret key
        #[arg(long)]
        new_key: bool,
        /// Generate a new Share URI for this host start
        #[arg(long)]
        new_uri: bool,
        /// Path to the secret key file
        #[arg(long)]
        key_path: Option<PathBuf>,
        /// Maximum number of connected players
        #[arg(long)]
        max_players: Option<u32>,
    },
    /// Join a room with a share URI
    Join {
        /// Share URI provided by the host
        join_uri: String,
        /// Local port for the Minecraft client
        #[arg(short, long)]
        port: Option<std::num::NonZeroU16>,
        /// Path status interval in seconds
        #[arg(short, long, default_value_t = 0)]
        delay: u64,
        /// Reconnect timeout in seconds; 0 means unlimited
        #[arg(long, default_value_t = 30)]
        reconnect_timeout: u64,
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
    match cli.command {
        Commands::Host {
            port,
            new_key,
            key_path,
            relay,
            delay,
            max_players,
            token_refresh,
            new_uri,
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

            let mut profile = Profile::load()?;
            let relay_url = profile.resolve_relay_url(relay.as_deref())?;
            let token_refresh = match token_refresh {
                Some(token_refresh) => {
                    profile.host.token_refresh = token_refresh.setting();
                    profile.save()?;
                    token_refresh
                }
                None => profile.host.token_refresh.into(),
            };
            let config = HostConfig::new()
                .event_delay(Duration::from_secs(delay))
                .max_players(max_players);

            run_host(port, secret_key, relay_url, config, token_refresh, new_uri).await?;
        }
        Commands::Join {
            join_uri,
            port,
            delay,
            reconnect_timeout,
        } => {
            let service = TunnelService::new();
            let mut updates = service.subscribe();
            let join_uri: JoinUri = join_uri.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(url) = join_uri.relay_url() {
                println!("Relay: {url}");
            }

            let config = JoinConfig::new()
                .event_delay(Duration::from_secs(delay))
                .reconnect_timeout(
                    (reconnect_timeout != 0).then_some(Duration::from_secs(reconnect_timeout)),
                );

            service
                .start_join(
                    JoinOptions::new(join_uri)
                        .local_port(port.map_or(LocalPort::Auto, LocalPort::Fixed))
                        .config(config),
                )
                .await?;
            let status = wait_until_active(&mut updates, TunnelMode::Join).await?;
            let local_addr = status
                .state
                .local_addr
                .ok_or_else(|| anyhow::anyhow!("join started without a local listener"))?;
            println!("Tunnel running. Connect MC client to {local_addr}");
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

async fn run_host(
    port: u16,
    secret_key: SecretKey,
    relay_url: Option<sculk::tunnel::RelayUrl>,
    config: HostConfig,
    token_refresh: TokenRefresh,
    new_uri: bool,
) -> anyhow::Result<()> {
    let state_path = persist::default_host_state_path()?;
    let saved_state = persist::load_host_state(&state_path)?;
    let node = SculkNode::bind(NodeOptions {
        secret_key: Some(secret_key),
        relay_url,
        ..NodeOptions::default()
    })
    .await?;
    let result = run_host_until_shutdown(
        &node,
        port,
        config,
        token_refresh,
        new_uri,
        saved_state,
        &state_path,
    )
    .await;
    node.close().await;
    result?;

    let closed_style = *CLAP_STYLES.get_error();
    println!(
        "\n{}Closed.{}",
        closed_style.render(),
        closed_style.render_reset()
    );
    Ok(())
}

async fn run_host_until_shutdown(
    node: &SculkNode,
    port: u16,
    config: HostConfig,
    token_refresh: TokenRefresh,
    new_uri: bool,
    saved_state: Option<HostState>,
    state_path: &std::path::Path,
) -> anyhow::Result<()> {
    let service_id = saved_state
        .as_ref()
        .map_or_else(ServiceId::generate, |state| state.service_id);
    let token_state = if new_uri {
        None
    } else {
        saved_state.map(|state| state.token_state)
    };
    let host = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            token_state,
            token_refresh: token_refresh.policy(),
            config,
        })
        .await?;
    let mut events = host.subscribe().await?;
    let mut statuses = host.subscribe_status().await?;
    let mut uri_generation = host.status().await?.uri_generation;
    save_host_state(state_path, &host).await?;
    print_join_uri(&host.join_uri().await?, false)?;
    println!("Press Ctrl+C to stop.");

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            result = &mut ctrl_c => {
                result?;
                break;
            }
            event = events.recv() => {
                match event {
                    Ok(TunnelEvent::TokenRotated) => {}
                    Ok(event) => print_event(&event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        eprintln!("[!] Missed {count} tunnel events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            status = statuses.recv() => {
                let Some(status) = status else {
                    break;
                };
                if status.uri_generation > uri_generation {
                    uri_generation = status.uri_generation;
                    save_host_state(state_path, &host).await?;
                    print_join_uri(&host.join_uri().await?, true)?;
                }
            }
        }
    }
    Ok(())
}

async fn save_host_state(path: &std::path::Path, host: &HostedServiceHandle) -> anyhow::Result<()> {
    let state = HostState {
        service_id: host.service_id(),
        token_state: host.token_state().await?,
    };
    persist::save_host_state(path, &state)?;
    Ok(())
}

fn print_join_uri(join_uri: &JoinUri, rotated: bool) -> anyhow::Result<()> {
    let uri = join_uri.expose_secret_uri()?;
    let quoted = format!("\"{uri}\"");
    let uri_style = *CLAP_STYLES.get_literal();
    let label = if rotated {
        "Updated Join URI"
    } else {
        "Join URI"
    };
    println!(
        "{label}: {}{quoted}{}",
        uri_style.render(),
        uri_style.render_reset()
    );

    if clipboard::copy(&quoted) {
        println!("(Copied to clipboard)");
    }
    let hint_style = *CLAP_STYLES.get_valid();
    println!(
        "{}Share this URI with players.{}",
        hint_style.render(),
        hint_style.render_reset()
    );
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
            TunnelUpdate::Event(TunnelEvent::Error { message, .. }) => {
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
        TunnelEvent::Error { message, .. } => eprintln!("[!] Error: {message}"),
        TunnelEvent::Reconnecting { attempt } => {
            println!("[~] Reconnecting (attempt {attempt})...")
        }
        TunnelEvent::Reconnected => println!("[*] Reconnected to host"),
        TunnelEvent::TokenRotationFailed { retry_in } => {
            eprintln!("[!] Token rotation failed; retrying in {retry_in:?}")
        }
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
        let cli_res = Cli::try_parse_from(["sculk", "host", "-p", "25565"]);
        assert!(cli_res.is_ok(), "parse host");
        let cli = if let Ok(v) = cli_res { v } else { return };
        assert!(matches!(cli.command, Commands::Host { port: 25565, .. }));
    }

    #[test]
    fn parse_join_defaults() {
        let cli_res = Cli::try_parse_from(["sculk", "join", "sculk://join/v1/payload"]);
        assert!(cli_res.is_ok(), "parse join");
        let cli = if let Ok(v) = cli_res { v } else { return };
        assert!(matches!(cli.command, Commands::Join { port: None, .. }));
    }

    #[test]
    fn parse_host_token_refresh_presets() {
        for flag in ["-t", "--time"] {
            for value in ["always", "never", "1h", "3h", "6h", "12h", "24h"] {
                let cli = Cli::try_parse_from(["sculk", "host", flag, value]);
                assert!(cli.is_ok(), "parse {flag} {value}");
            }
        }
    }

    #[test]
    fn reject_unsupported_token_refresh() {
        let cli = Cli::try_parse_from(["sculk", "host", "--time", "2h"]);
        assert!(cli.is_err());
    }

    #[test]
    fn host_token_refresh_uses_saved_default() {
        let cli = Cli::try_parse_from(["sculk", "host"]);
        assert!(matches!(
            cli,
            Ok(Cli {
                command: Commands::Host {
                    token_refresh: None,
                    new_uri: false,
                    ..
                }
            })
        ));
    }

    #[test]
    fn parse_host_new_uri() {
        let cli = Cli::try_parse_from(["sculk", "host", "--new-uri"]);
        assert!(matches!(
            cli,
            Ok(Cli {
                command: Commands::Host { new_uri: true, .. }
            })
        ));
    }
}
