//! 网络路径监控：监听 iroh 路径变化，派发 `PathChanged` 事件。

use super::*;

/// 监控路径变化并发送 `PathChanged` 事件。
///
/// `event_delay == 0` 时只在状态变化时发送；否则按间隔发送并在 relay/direct 切换时立即发送。
pub(super) fn spawn_path_monitor(
    conn: Connection,
    remote_id: String,
    tx: mpsc::Sender<TunnelEvent>,
    event_delay: Duration,
) {
    tokio::spawn(async move {
        let mut watcher = conn.paths();
        let mut last_is_relay: Option<bool> = None;
        let mut last_rtt_ms: Option<u64> = None;

        if let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) {
            send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
            last_is_relay = Some(is_relay);
            last_rtt_ms = Some(rtt_ms);
        }

        if event_delay.is_zero() {
            loop {
                if watcher.updated().await.is_err() {
                    break;
                }
                let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) else {
                    continue;
                };
                if last_is_relay != Some(is_relay) || last_rtt_ms != Some(rtt_ms) {
                    send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
                    last_is_relay = Some(is_relay);
                    last_rtt_ms = Some(rtt_ms);
                }
            }
        } else {
            let mut timer = tokio::time::interval(event_delay);
            timer.tick().await;

            loop {
                tokio::select! {
                    result = watcher.updated() => {
                        if result.is_err() { break; }
                        let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) else {
                            continue;
                        };
                        if last_is_relay != Some(is_relay) {
                            send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
                            last_is_relay = Some(is_relay);
                            timer.reset();
                        }
                    }
                    _ = timer.tick() => {
                        if let Some((is_relay, rtt_ms)) = extract_selected_path(&watcher.get()) {
                            send_path_event(&remote_id, is_relay, rtt_ms, &tx).await;
                            last_is_relay = Some(is_relay);
                        }
                    }
                }
            }
        }
    });
}

/// 提取当前选中路径的 `(is_relay, rtt_ms)`。
fn extract_selected_path(paths: &PathInfoList) -> Option<(bool, u64)> {
    paths
        .iter()
        .find(|p| p.is_selected())
        .map(|p| (p.is_relay(), p.rtt().as_millis() as u64))
}

async fn send_path_event(
    remote_id: &str,
    is_relay: bool,
    rtt_ms: u64,
    tx: &mpsc::Sender<TunnelEvent>,
) {
    let _ = tx
        .send(TunnelEvent::PathChanged {
            remote_id: remote_id.to_string(),
            is_relay,
            rtt_ms,
        })
        .await;
}
