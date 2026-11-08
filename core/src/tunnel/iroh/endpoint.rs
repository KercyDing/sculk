//! iroh `Endpoint` 构建辅助。

use super::*;
use iroh::{RelayMap, RelayMode};

/// 构建 Endpoint，根据参数配置 secret key 和 relay 模式。
///
/// `presets::N0` 同时注册 n0 的 Pkarr/DNS 地址查询；自定义 Relay 只替换
/// `RelayMode`，不会移除这些查询服务，因此直连地址发现仍会尝试使用 n0。
pub(super) fn build_endpoint(
    secret_key: Option<SecretKey>,
    relay_url: Option<&RelayUrl>,
) -> iroh::endpoint::Builder {
    let mut builder = Endpoint::builder(iroh::endpoint::presets::N0);
    if let Some(key) = secret_key {
        builder = builder.secret_key(key.0);
    }
    if let Some(url) = relay_url {
        let relay_map = RelayMap::from(url.0.clone());
        builder = builder.relay_mode(RelayMode::Custom(relay_map));
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
        let Ok(host) = build_endpoint(None, Some(&relay))
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
            .clear_address_lookup()
            .alpns(vec![TEST_ALPN.to_vec()])
            .bind()
            .await
        else {
            return;
        };
        let Ok(join) = build_endpoint(None, Some(&relay))
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
}
