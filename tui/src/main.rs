//! sckt 终端图形工具（TUI）。
//!
//! 用法：
//! - `sckt`：启动终端图形界面
//! - 在界面中切换「建房 / 加入 / 中继」模式并执行对应操作
//! - 通过右侧日志查看实时事件与错误信息

mod app;
mod services;
mod state;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::run_tui().await
}
