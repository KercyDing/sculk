//! `sculk-tui` 主界面与事件循环。

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const TICK: Duration = Duration::from_millis(200);

#[derive(Debug, Default)]
struct AppState {
    show_help: bool,
}

enum Step {
    Continue,
    Exit,
}

/// 启动 TUI 并处理按键事件，按 `q` / `Esc` 退出。
pub fn run_tui() -> anyhow::Result<()> {
    let mut terminal = init_terminal()?;
    let mut state = AppState::default();
    let run_result = (|| -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| render(frame, &state))?;

            if !event::poll(TICK)? {
                continue;
            }

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
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

fn render(frame: &mut ratatui::Frame<'_>, state: &AppState) {
    let chunks = Layout::vertical([Constraint::Length(8), Constraint::Min(3)])
        .margin(1)
        .split(frame.area());

    let header = Paragraph::new(
        "sculk-tui\n\n\
         [q / Esc] 退出\n\
         [h / ?]   帮助开关\n\
         该程序为纯 TUI 入口",
    )
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .title("状态")
            .borders(Borders::ALL)
            .border_style(Style::default().add_modifier(Modifier::BOLD)),
    );
    frame.render_widget(header, chunks[0]);

    let footer = Paragraph::new(
        "提示: 当前阶段仅实现 TUI 基础框架。后续可接 host/join/relay 面板与实时事件流。",
    )
    .block(Block::default().title("说明").borders(Borders::ALL));
    frame.render_widget(footer, chunks[1]);

    if state.show_help {
        let popup_area = centered_rect(60, 40, frame.area());
        frame.render_widget(Clear, popup_area);
        let help_block = Block::default().title("帮助").borders(Borders::ALL);
        let help = Paragraph::new(
            "快捷键:\n\
             - q / Esc: 退出\n\
             - h / ?: 打开或关闭帮助\n\n\
             目标:\n\
             - 纯 TUI 入口\n\
             - 后续逐步接入业务流程",
        );
        frame.render_widget(help_block, popup_area);
        frame.render_widget(help, popup_area.inner(Margin::new(1, 1)));
    }
}

fn centered_rect(
    width_percent: u16,
    height_percent: u16,
    area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(vertical[1])[1]
}

impl AppState {
    /// 处理单个键盘事件并返回循环控制信号。
    fn handle_key(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Step::Exit,
            KeyCode::Char('h') | KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                Step::Continue
            }
            _ => Step::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{AppState, Step};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn toggle_help_state() {
        let mut state = AppState::default();
        assert!(!state.show_help);
        assert!(matches!(
            state.handle_key(key(KeyCode::Char('h'))),
            Step::Continue
        ));
        assert!(state.show_help);
        assert!(matches!(
            state.handle_key(key(KeyCode::Char('?'))),
            Step::Continue
        ));
        assert!(!state.show_help);
    }

    #[test]
    fn quit_keys_exit() {
        let mut state = AppState::default();
        assert!(matches!(
            state.handle_key(key(KeyCode::Char('q'))),
            Step::Exit
        ));
        assert!(matches!(state.handle_key(key(KeyCode::Esc)), Step::Exit));
    }
}
