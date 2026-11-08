//! iroh `Endpoint` 构建辅助。

use std::time::Duration;

use super::*;
use crate::error::RelayUnavailable;
use iroh::Watcher;
use iroh::{RelayMap, RelayMode};

/// 启动时等待至少一个 home relay 连接成功的最长时间。
pub(super) const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// 有界等待 Relay 连接；`Endpoint::online` 本身没有超时。
pub(super) async fn wait_online(
    endpoint: &Endpoint,
    configured_relay: Option<&RelayUrl>,
) -> std::result::Result<(), RelayUnavailable> {
    wait_online_with(endpoint, configured_relay, RELAY_CONNECT_TIMEOUT).await
}

async fn wait_online_with(
    endpoint: &Endpoint,
    configured_relay: Option<&RelayUrl>,
    timeout: Duration,
) -> std::result::Result<(), RelayUnavailable> {
    let mut watcher = endpoint.home_relay_status();
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let statuses = watcher.get();
        if statuses.iter().any(|status| status.is_connected()) {
            return Ok(());
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(relay_unavailable(&statuses, configured_relay, timeout));
        }

        match tokio::time::timeout(remaining, watcher.updated()).await {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => return Err(relay_unavailable(&statuses, configured_relay, timeout)),
            Err(_) => {}
        }
    }
}

fn relay_unavailable(
    statuses: &[iroh::endpoint::RelayStatus],
    configured_relay: Option<&RelayUrl>,
    timeout: Duration,
) -> RelayUnavailable {
    let mut relays: Vec<String> = statuses
        .iter()
        .map(|status| status.url().to_string())
        .collect();
    if relays.is_empty()
        && let Some(relay) = configured_relay
    {
        relays.push(relay.to_string());
    }
    let last_error = statuses
        .iter()
        .find_map(|status| status.last_error().map(ToString::to_string));
    let auth_denied = statuses
        .iter()
        .find_map(|status| status.auth_denied_reason().map(str::to_string));
    RelayUnavailable::new(timeout, relays, last_error, auth_denied)
}

/// 构建 Endpoint，根据参数配置 secret key 和 relay 模式。
///
/// `presets::N0` 同时注册 n0 的 Pkarr/DNS 地址查询；自定义 Relay 只替换
/// `RelayMode`，不会移除这些查询服务，因此直连地址发现仍会尝试使用 n0。
pub(super) fn build_endpoint(
    secret_key: Option<SecretKey>,
    relay_url: Option<&RelayUrl>,
    address_lookup: bool,
) -> iroh::endpoint::Builder {
    let mut builder = Endpoint::builder(iroh::endpoint::presets::N0);
    if let Some(key) = secret_key {
        builder = builder.secret_key(key.0);
    }
    if let Some(url) = relay_url {
        let relay_map = RelayMap::from(url.0.clone());
        builder = builder.relay_mode(RelayMode::Custom(relay_map));
    }
    if !address_lookup {
        builder = builder.clear_address_lookup();
    }
    #[cfg(feature = "insecure-relay-tls")]
    {
        builder = builder.ca_tls_config(iroh::tls::CaTlsConfig::insecure_skip_verify());
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::tls::CaTlsConfig;

    const TEST_ALPN: &[u8] = b"/sculk/test/1";

    /// Relay 地址是唯一远端提示时，连接应直接经 Relay 建立。
    #[tokio::test]
    async fn connects_through_relay_hint() {
        let Ok((_, relay, _server)) = iroh::test_utils::run_relay_server().await else {
            return;
        };
        let relay = RelayUrl(relay);
        let Ok(host) = build_endpoint(None, Some(&relay), false)
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .clear_address_lookup()
            .alpns(vec![TEST_ALPN.to_vec()])
            .bind()
            .await
        else {
            return;
        };
        let Ok(join) = build_endpoint(None, Some(&relay), false)
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .clear_address_lookup()
            .alpns(vec![TEST_ALPN.to_vec()])
            .bind()
            .await
        else {
            return;
        };
        host.online().await;
        join.online().await;
        let host_id = host.id();

        let host_task = tokio::spawn(async move {
            let accepting = host.accept().await?;
            accepting.await.ok()
        });
        let addr = iroh::EndpointAddr::from(host_id).with_relay_url(relay.0.clone());
        let Ok(conn) = join.connect(addr, TEST_ALPN).await else {
            return;
        };
        let paths = conn.paths();
        let Some(path) = paths.iter().find(|path| path.is_selected()) else {
            return;
        };
        assert!(path.is_relay());

        let accepted = tokio::time::timeout(Duration::from_secs(5), host_task).await;
        assert!(matches!(accepted, Ok(Ok(Some(_)))));
    }

    #[tokio::test]
    async fn wait_online_times_out_without_relay() {
        let Ok(endpoint) = Endpoint::builder(iroh::endpoint::presets::N0DisableRelay)
            .clear_address_lookup()
            .bind()
            .await
        else {
            return;
        };

        let timeout = Duration::from_millis(100);
        let result = wait_online_with(&endpoint, None, timeout).await;
        endpoint.close().await;

        assert!(result.is_err(), "wait_online returned before timeout");
        let Err(error) = result else {
            return;
        };
        assert_eq!(error.timeout(), timeout);
        assert!(error.relays().is_empty());
        assert!(error.to_string().contains("default servers"));
    }
}
