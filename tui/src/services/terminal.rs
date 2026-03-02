//! 终端生命周期服务：进入/退出 raw mode 与 alternate screen。

use std::io;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// 应用终端类型别名。
pub type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

/// 终端会话守卫：构造时进入 TUI 模式，释放时恢复终端。
pub struct TerminalSession {
    terminal: AppTerminal,
}

impl TerminalSession {
    /// 进入终端会话。
    ///
    /// Purpose: 启动 TUI 前初始化 raw mode 与 alternate screen。
    /// Args: 无。
    /// Returns: 成功时返回会话守卫。
    /// Edge Cases: 任一步骤失败会返回错误，调用方可中止启动。
    pub fn enter() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;
        Ok(Self { terminal })
    }

    /// 绘制一帧。
    ///
    /// Purpose: 暴露最小绘制接口，避免在调用方长期借用终端引用。
    /// Args: `f` 为单帧渲染闭包。
    /// Returns: 绘制成功返回 `Ok(())`。
    /// Edge Cases: 底层 IO 失败时返回错误。
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
