//! 异步事件循环：tokio::select! 同时监听键盘事件、隧道事件与 tick。

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use sculk::tunnel::{TunnelMode, TunnelPhase, TunnelUpdate};
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
    let mut tunnel_updates = state.ctx.tunnel.subscribe();
    let mut event_stream = EventStream::new();
    let mut tick_interval = time::interval(TICK);

    loop {
        terminal.draw(|frame| ui::render(frame, &mut state))?;

        tokio::select! {
            maybe_event = event_stream.next() => {
                let Some(event_result) = maybe_event else { break };
                let event = match event_result {
                    Ok(event) => event,
                    Err(e) => {
                        state.add_log(&format!("事件读取失败: {e}"));
                        continue;
                    }
                };
                let Event::Key(key) = event else { continue };
                if key.kind == KeyEventKind::Release { continue }
                if matches!(state.handle_key(key), Step::Exit) { break }
            }
            Some(message) = app_rx.recv() => {
                state.add_log(&message);
            }
            update = tunnel_updates.recv() => {
                let Some(update) = update else { break };
                copy_host_ticket(&mut state, &update);
                state.handle_tunnel_update(update);
            }
            _ = tick_interval.tick() => {
                state.on_tick();
            }
        }
    }

    // 清理异步资源
    let _ = tokio::time::timeout(Duration::from_secs(3), state.ctx.tunnel.shutdown()).await;

    Ok(())
}

fn copy_host_ticket(state: &mut AppState, update: &TunnelUpdate) {
    let TunnelUpdate::Status(status) = update else {
        return;
    };
    if state.phase == TunnelPhase::Active
        || status.state.phase != TunnelPhase::Active
        || status.state.mode != Some(TunnelMode::Host)
    {
        return;
    }
    let Some(ticket) = status.state.ticket.as_ref() else {
        return;
    };
    if sculk::clipboard::clipboard_copy(&ticket.to_string()) {
        state.add_log("票据已复制到剪贴板");
    }
}
