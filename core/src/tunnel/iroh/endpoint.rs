use super::*;

/// 构建 Endpoint builder，根据参数配置 secret key 和 relay 模式
pub(super) fn build_endpoint(
    secret_key: Option<SecretKey>,
    relay_url: Option<&RelayUrl>,
) -> iroh::endpoint::Builder {
    let mut builder = Endpoint::builder();
    if let Some(key) = secret_key {
        builder = builder.secret_key(key);
    }
    if let Some(url) = relay_url {
        let relay_map = RelayMap::from(url.clone());
        builder = builder.relay_mode(RelayMode::Custom(relay_map));
    }
    builder
}
