//! System clipboard integration owned by the TUI application.

/// Copies text to the system clipboard.
pub fn copy(text: &str) -> bool {
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
            return child.wait().is_ok_and(|status| status.success());
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
            return child.wait().is_ok_and(|status| status.success());
        }
    }

    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .is_ok()
}
