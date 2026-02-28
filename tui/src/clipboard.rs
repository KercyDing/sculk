//! 剪贴板操作（移植自 cli/app.rs）。

/// 复制文本到系统剪贴板。
pub fn clipboard_copy(text: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && let Ok(mut child) = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().is_ok_and(|s| s.success());
        }

        if std::env::var_os("DISPLAY").is_some()
            && let Ok(mut child) = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().is_ok_and(|s| s.success());
        }
    }

    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .is_ok()
}
