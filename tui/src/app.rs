//! 异步事件循环：tokio::select! 同时监听键盘事件、隧道事件与 tick。

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::time;

use crate::services::terminal::TerminalSession;
use crate::state::{AppState, Step};
use crate::ui;

const TICK: Duration = Duration::from_millis(200);

/// 启动 TUI 异步事件循环。
///
/// Purpose: 驱动渲染循环并统一处理键盘、隧道与 tick 事件。
/// Args: 无。
/// Returns: 正常退出时返回 `Ok(())`，异常时返回错误。
/// Edge Cases: 会话异常中断时由 `TerminalSession` 在 Drop 中恢复终端。
pub async fn run_tui() -> anyhow::Result<()> {
    let mut terminal = TerminalSession::enter()?;

    let (app_tx, mut app_rx) = mpsc::unbounded_channel();
    let mut state = AppState::new(app_tx);
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

    Ok(())
}
