//! 网络路径监控：监听 iroh 路径变化，派发 `PathChanged` 事件。

use super::*;
use futures_util::StreamExt;
use iroh::endpoint::PathList;

/// 监控路径变化并发送 `PathChanged` 事件。
///
/// `event_delay == 0` 时只在状态变化时发送；否则按间隔发送并在 relay/direct 切换时立即发送。
pub(super) fn spawn_path_monitor(
    conn: Connection,
    remote_id: PeerId,
    tx: mpsc::Sender<TunnelEvent>,
    event_delay: Duration,
) {
    tokio::spawn(async move {
        let mut watcher = conn.paths_stream();
        let mut last_is_relay: Option<bool> = None;
        let mut last_rtt_ms: Option<u64> = None;

        if let Some(paths) = watcher.next().await
            && let Some((is_relay, rtt_ms)) = extract_selected_path(&paths)
        {
            send_path_event(&remote_id, is_relay, rtt_ms, &tx);
            last_is_relay = Some(is_relay);
            last_rtt_ms = Some(rtt_ms);
        }

        if event_delay.is_zero() {
            loop {
                let Some(paths) = watcher.next().await else {
                    break;
                };
                let Some((is_relay, rtt_ms)) = extract_selected_path(&paths) else {
                    continue;
                };
                if should_emit_on_update(event_delay, last_is_relay, last_rtt_ms, is_relay, rtt_ms)
                {
                    send_path_event(&remote_id, is_relay, rtt_ms, &tx);
                    last_is_relay = Some(is_relay);
                    last_rtt_ms = Some(rtt_ms);
                }
            }
        } else {
            let mut timer = tokio::time::interval(event_delay);
            timer.tick().await;

            loop {
                tokio::select! {
                    next_paths = watcher.next() => {
                        let Some(paths) = next_paths else {
                            break;
                        };
                        let Some((is_relay, rtt_ms)) = extract_selected_path(&paths) else {
                            continue;
                        };
                        if should_emit_on_update(
                            event_delay,
                            last_is_relay,
                            last_rtt_ms,
                            is_relay,
                            rtt_ms,
                        ) {
                            send_path_event(&remote_id, is_relay, rtt_ms, &tx);
                            last_is_relay = Some(is_relay);
                            last_rtt_ms = Some(rtt_ms);
                            timer.reset();
                        } else {
                            last_rtt_ms = Some(rtt_ms);
                        }
                    }
                    _ = timer.tick() => {
                        if let Some((is_relay, rtt_ms)) = extract_selected_path(&conn.paths()) {
                            send_path_event(&remote_id, is_relay, rtt_ms, &tx);
                            last_is_relay = Some(is_relay);
                            last_rtt_ms = Some(rtt_ms);
                        }
                    }
                }
            }
        }
    });
}

/// 提取当前选中路径的 `(is_relay, rtt_ms)`。
fn extract_selected_path(paths: &PathList<'_>) -> Option<(bool, u64)> {
    paths
        .iter()
        .find(|p| p.is_selected())
        .map(|p| (p.is_relay(), p.rtt().as_millis() as u64))
}

fn should_emit_on_update(
    event_delay: Duration,
    last_is_relay: Option<bool>,
    last_rtt_ms: Option<u64>,
    is_relay: bool,
    rtt_ms: u64,
) -> bool {
    last_is_relay != Some(is_relay) || (event_delay.is_zero() && last_rtt_ms != Some(rtt_ms))
}

fn send_path_event(
    remote_id: &PeerId,
    is_relay: bool,
    rtt_ms: u64,
    tx: &mpsc::Sender<TunnelEvent>,
) {
    super::emit_event(
        tx,
        TunnelEvent::PathChanged {
            remote_id: remote_id.clone(),
            is_relay,
            rtt_ms,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_delay_tracks_rtt() {
        assert!(should_emit_on_update(
            Duration::ZERO,
            Some(false),
            Some(20),
            false,
            21
        ));
    }

    #[test]
    fn delay_throttles_rtt() {
        let delay = Duration::from_secs(1);
        assert!(!should_emit_on_update(
            delay,
            Some(false),
            Some(20),
            false,
            21
        ));
        assert!(should_emit_on_update(
            delay,
            Some(false),
            Some(20),
            true,
            21
        ));
    }
}
