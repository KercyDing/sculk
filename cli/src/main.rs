//! sculk CLI
//!
//! 用法:
//! - `sculk host` — 创建房间，获得票据
//! - `sculk join <ticket>` — 用票据加入房间

use clap::{Parser, Subcommand};
use sculk_core::tunnel::IrohTunnel;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "sculk",
    version,
    about = "P2P tunnel for Minecraft multiplayer"
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
    },
    /// Join a hosted room via ticket
    Join {
        /// Ticket from the host
        ticket: String,
        /// Local port for MC client to connect
        #[arg(short, long, default_value_t = sculk_core::DEFAULT_INLET_PORT)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Host { port } => {
            let (tunnel, ticket) = IrohTunnel::host(port).await?;
            println!("Ticket: {ticket}");
            println!("Share this ticket with players.");
            println!("Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await?;
            tunnel.close().await;
        }
        Commands::Join { ticket, port } => {
            let tunnel = IrohTunnel::join(&ticket, port).await?;
            println!("Tunnel running. Connect MC client to 127.0.0.1:{port}");
            println!("Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await?;
            tunnel.close().await;
        }
    }

    Ok(())
}
