//! sculk CLI
//!
//! Usage:
//! - `sculk host` — create a room and get a ticket
//! - `sculk join <ticket>` — join a room via ticket
//! - `sculk relay` — manage custom relay server

mod key;
mod relay;

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use sculk_core::tunnel::{IrohTunnel, RelayUrl, TunnelEvent};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "sculk",
    version,
    about = "P2P tunnel for Minecraft multiplayer",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start hosting and expose local MC server
    Host {
        /// Local Minecraft server port
        #[arg(short, long, default_value_t = sculk_core::DEFAULT_MC_PORT)]
        port: u16,
        /// Generate a new secret key (changes ticket)
        #[arg(long)]
        new_key: bool,
        /// Custom secret key file path
        #[arg(long)]
        key_path: Option<PathBuf>,
        /// Override relay server URL (takes precedence over config)
        #[arg(short, long)]
        relay: Option<String>,
    },
    /// Join a hosted room via ticket
    Join {
        /// Ticket from the host
        ticket: String,
        /// Local port for MC client to connect
        #[arg(short, long, default_value_t = sculk_core::DEFAULT_INLET_PORT)]
        port: u16,
    },
    /// Manage custom relay server configuration
    Relay {
        /// Set custom relay server URL
        #[arg(long)]
        url: Option<String>,
        /// Show current relay configuration
        #[arg(long)]
        list: bool,
        /// Reset to default n0 relay servers
        #[arg(long)]
        reset: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Host {
            port,
            new_key,
            key_path,
            relay,
        } => {
            let path = key_path.unwrap_or_else(default_key_path);
            let secret_key = if new_key {
                key::generate_new_key(&path)?
            } else {
                key::load_or_generate_key(&path)?
            };
            tracing::info!(key_path = %path.display(), "using secret key");

            let relay_url = resolve_relay_url(relay.as_deref())?;

            let (tunnel, ticket, mut events) =
                IrohTunnel::host(port, Some(secret_key), relay_url).await?;
            let ticket_str = ticket.to_string();
            println!("Ticket: {ticket_str}");

            if let Ok(()) = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&ticket_str)) {
                println!("(Copied to clipboard)");
            }

            println!("Share this ticket with players.");
            println!("Press Ctrl+C to stop.");

            tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    print_event(&event);
                }
            });

            tokio::signal::ctrl_c().await?;
            tunnel.close().await;
        }
        Commands::Join { ticket, port } => {
            let ticket: sculk_core::tunnel::Ticket =
                ticket.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(ref url) = ticket.relay_url {
                println!("Relay: {url}");
            }

            let (tunnel, mut events) = IrohTunnel::join(&ticket, port).await?;
            println!("Tunnel running. Connect MC client to 127.0.0.1:{port}");
            println!("Press Ctrl+C to stop.");

            tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    print_event(&event);
                }
            });

            tokio::signal::ctrl_c().await?;
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
                    .unwrap()
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

/// Resolve relay URL: --relay flag > config file > None (default n0)
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
    }
}
