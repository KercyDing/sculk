//! `sculk-tui` 的交互主界面（最小实现）。

use std::io::{self, Write};

/// 启动 TUI 主界面，直到用户主动退出。
pub fn run_tui() -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        render_home(&mut stdout)?;
        line.clear();
        stdin.read_line(&mut line)?;

        match line.trim() {
            "q" | "quit" | "exit" => break,
            "help" | "h" => {
                println!("提示: 当前版本为纯 TUI 入口，输入 `q` 退出。");
                wait_enter(&mut stdout)?;
            }
            _ => {
                println!("未知输入，输入 `help` 查看提示，输入 `q` 退出。");
                wait_enter(&mut stdout)?;
            }
        }
    }

    Ok(())
}

fn render_home(stdout: &mut io::Stdout) -> anyhow::Result<()> {
    print!("\x1B[2J\x1B[H");
    println!("┌──────────────────────────────────────────────────────────┐");
    println!("│                       sculk-tui                          │");
    println!("├──────────────────────────────────────────────────────────┤");
    println!("│ 默认进入 TUI。                                            │");
    println!("│                                                          │");
    println!("│ 使用方式:                                                 │");
    println!("│ 1) 输入 `help` 查看说明                                   │");
    println!("│ 2) 输入 `q` 退出                                          │");
    println!("└──────────────────────────────────────────────────────────┘");
    print!("> ");
    stdout.flush()?;
    Ok(())
}

fn wait_enter(stdout: &mut io::Stdout) -> anyhow::Result<()> {
    print!("按 Enter 继续...");
    stdout.flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(())
}
