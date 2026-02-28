mod app;
mod clipboard;
mod config;
mod input;
mod state;
mod tunnel;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run_tui().await
}
