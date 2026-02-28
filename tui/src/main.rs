mod app;
mod state;
mod ui;

fn main() -> anyhow::Result<()> {
    app::run_tui()
}
