//! `sckt` 终端界面。

mod app;
mod input;
mod keymap;
mod model;
mod terminal;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run_tui().await
}

#[cfg(test)]
mod tests;
