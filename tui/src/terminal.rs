//! 终端会话生命周期。

use std::io;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// 应用使用的终端类型。
pub type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

/// 释放时恢复终端状态。
pub struct TerminalSession {
    terminal: AppTerminal,
}

impl TerminalSession {
    /// 进入原始模式和备用屏幕。
    pub fn enter() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        match Self::enter_inner() {
            Ok(session) => Ok(session),
            Err(e) => {
                let _ = disable_raw_mode();
                Err(e)
            }
        }
    }

    /// 在启用原始模式后完成初始化。
    fn enter_inner() -> anyhow::Result<Self> {
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;
        Ok(Self { terminal })
    }

    /// 绘制一帧。
    pub fn draw<F>(&mut self, f: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(f)?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
