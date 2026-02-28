//! 事件循环：初始化终端、轮询键盘事件、驱动渲染。

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::state::{AppState, Step};
use crate::ui;

const TICK: Duration = Duration::from_millis(200);

/// 启动 TUI 并处理按键事件，按 `Esc` 连按两次退出。
pub fn run_tui() -> anyhow::Result<()> {
    let mut terminal = init_terminal()?;
    let mut state = AppState::default();
    let run_result = (|| -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| ui::render(frame, &mut state))?;

            if !event::poll(TICK)? {
                state.on_tick();
                continue;
            }

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }

            if matches!(state.handle_key(key), Step::Exit) {
                break;
            }
        }
        Ok(())
    })();

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
