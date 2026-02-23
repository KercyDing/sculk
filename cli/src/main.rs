//! sculk-cli: P2P 安全隧道的命令行入口
//!
//! 提供 `host` 和 `join` 子命令，分别用于创建和加入房间。

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "sculk", version, about = "P2P 安全隧道 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 创建房间，将本地 MC 服务端暴露到隧道
    Host {
        /// 本地 MC 端口
        #[arg(short, long, default_value_t = sculk_core::DEFAULT_MC_PORT)]
        port: u16,
    },
    /// 通过票据加入远程服务器
    Join {
        /// 房主提供的连接票据
        ticket: String,
        /// 本地监听端口
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
            println!("TODO: host on MC port {port}");
        }
        Commands::Join { ticket, port } => {
            println!("TODO: join with ticket {ticket}, local port {port}");
        }
    }

    Ok(())
}
