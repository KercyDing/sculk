mod app;
mod state;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run_tui().await
}
