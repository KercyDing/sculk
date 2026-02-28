//! `sculk://` 票据格式的编解码。
//!
//! 票据格式：
//! - `"sculk://<EndpointId>?relay=<RelayUrl>"` — 自定义 relay
//! - `"sculk://<EndpointId>"` — 默认 n0 relay

use std::fmt;
use std::str::FromStr;

use iroh::{EndpointId, RelayUrl};

const SCHEME: &str = "sculk";

/// 连接票据，包含目标节点与可选 relay 地址。
#[derive(Debug)]
pub struct Ticket {
    pub endpoint_id: EndpointId,
    pub relay_url: Option<RelayUrl>,
}

impl Ticket {
    /// 创建票据。
    pub fn new(endpoint_id: EndpointId, relay_url: Option<RelayUrl>) -> Self {
        Self {
            endpoint_id,
            relay_url,
        }
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.relay_url {
            Some(relay) => write!(f, "{SCHEME}://{}?relay={relay}", self.endpoint_id),
            None => write!(f, "{SCHEME}://{}", self.endpoint_id),
        }
    }
}

impl FromStr for Ticket {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = url::Url::parse(s)?;

        if url.scheme() != SCHEME {
            anyhow::bail!(
                "invalid scheme: expected \"{SCHEME}\", got \"{}\"",
                url.scheme()
            );
        }

        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("missing endpoint id in ticket URL"))?;

        if host.is_empty() {
            anyhow::bail!("missing endpoint id in ticket URL");
        }

        let endpoint_id: EndpointId = host
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid endpoint id: {e}"))?;

        let relay_url = url
            .query_pairs()
            .find(|(k, _)| k == "relay")
            .map(|(_, v)| v.parse::<RelayUrl>())
            .transpose()
            .map_err(|e| anyhow::anyhow!("invalid relay URL: {e}"))?;

        Ok(Self {
            endpoint_id,
            relay_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_endpoint_id() -> EndpointId {
        iroh::SecretKey::generate(&mut rand::rng()).public().into()
    }

    #[test]
    fn roundtrip_with_relay() {
        let id = test_endpoint_id();
        let relay: RelayUrl = "https://my-relay.example.com".parse().unwrap();
        let ticket = Ticket::new(id, Some(relay.clone()));

        let s = ticket.to_string();
        assert!(s.starts_with("sculk://"));
        assert!(s.contains("relay="));

        let parsed: Ticket = s.parse().unwrap();
        assert_eq!(parsed.endpoint_id, id);
        assert_eq!(parsed.relay_url.as_ref(), Some(&relay));

        let s2 = parsed.to_string();
        let reparsed: Ticket = s2.parse().unwrap();
        assert_eq!(reparsed.endpoint_id, id);
        assert_eq!(reparsed.relay_url.as_ref(), Some(&relay));
    }

    #[test]
    fn roundtrip_without_relay() {
        let id = test_endpoint_id();
        let ticket = Ticket::new(id, None);

        let s = ticket.to_string();
        assert!(s.starts_with("sculk://"));
        assert!(!s.contains("relay="));

        let parsed: Ticket = s.parse().unwrap();
        assert_eq!(parsed.endpoint_id, id);
        assert!(parsed.relay_url.is_none());

        let s2 = parsed.to_string();
        let reparsed: Ticket = s2.parse().unwrap();
        assert_eq!(reparsed.endpoint_id, id);
        assert!(reparsed.relay_url.is_none());
    }

    #[test]
    fn reject_bad_scheme() {
        let result = "http://abc".parse::<Ticket>();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid scheme"), "unexpected error: {err}");
    }

    #[test]
    fn reject_missing_host() {
        let result = "sculk:///".parse::<Ticket>();
        assert!(result.is_err());
    }
}
