//! 异步事件循环：tokio::select! 同时监听键盘事件、隧道事件与 tick。

use std::io;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;
use tokio::time;

use crate::state::{AppState, Step};
use crate::ui;

const TICK: Duration = Duration::from_millis(200);

/// 启动 TUI 异步事件循环。
pub async fn run_tui() -> anyhow::Result<()> {
    let mut terminal = init_terminal()?;

    let (app_tx, mut app_rx) = mpsc::unbounded_channel();
    let mut state = AppState::new(app_tx);

    let run_result = async {
        let mut event_stream = EventStream::new();
        let mut tick_interval = time::interval(TICK);

        loop {
            terminal.draw(|frame| ui::render(frame, &mut state))?;

            tokio::select! {
                maybe_event = event_stream.next() => {
                    let Some(event_result) = maybe_event else { break };
                    let Event::Key(key) = event_result? else { continue };
                    if key.kind == KeyEventKind::Release { continue }
                    if matches!(state.handle_key(key), Step::Exit) { break }
                }
                Some(app_event) = app_rx.recv() => {
                    state.handle_app_event(app_event);
                }
                _ = tick_interval.tick() => {
                    state.on_tick();
                }
            }
        }

        anyhow::Ok(())
    }
    .await;

    restore_terminal(&mut terminal)?;
    run_result
}

fn init_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    terminal.show_cursor()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
