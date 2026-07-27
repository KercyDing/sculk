//! `sculk://join/v1/` 分享 URI 的严格二进制编解码。

use std::fmt;
use std::str::FromStr;

use base64::Engine as _;

use crate::error::JoinUriError;
use crate::types::{AccessToken, RelayUrl, ServiceId};
use iroh::EndpointId;

const SCHEME: &str = "sculk";
const HOST: &str = "join";
const VERSION: &str = "v1";
const FLAG_RELAY: u8 = 0b0000_0001;
const FLAGS_KNOWN: u8 = FLAG_RELAY;
const PAYLOAD_BASE_LEN: usize = 1 + 32 + 16 + 32;
const URI_LEN_MAX: usize = 512;
const RELAY_LEN_MAX: usize = u8::MAX as usize;

/// 面向用户分享的完整加入信息。
///
/// 该值包含访问令牌，普通格式化输出会脱敏；只有
/// [`Self::expose_secret_uri`] 会返回可分享的 URI 字符串。
#[derive(Clone, PartialEq, Eq)]
pub struct JoinUri {
    endpoint_id: EndpointId,
    service_id: ServiceId,
    token: AccessToken,
    relay_url: Option<RelayUrl>,
}

impl JoinUri {
    /// 创建新的 Join URI 值。
    pub fn new(
        endpoint_id: EndpointId,
        service_id: ServiceId,
        token: AccessToken,
        relay_url: Option<RelayUrl>,
    ) -> Self {
        Self {
            endpoint_id,
            service_id,
            token,
            relay_url,
        }
    }

    /// 返回目标 EndpointId。
    pub(crate) fn endpoint_id(&self) -> EndpointId {
        self.endpoint_id
    }

    /// 返回目标服务标识。
    pub(crate) fn service_id(&self) -> ServiceId {
        self.service_id
    }

    /// 返回 Join 时需要发送的访问令牌。
    pub(crate) fn token(&self) -> &AccessToken {
        &self.token
    }

    /// 返回可选的自定义 Relay 地址。
    pub fn relay_url(&self) -> Option<&RelayUrl> {
        self.relay_url.as_ref()
    }

    /// 显式导出包含秘密的可分享 URI。
    pub fn expose_secret_uri(&self) -> Result<String, JoinUriError> {
        let mut payload = Vec::with_capacity(PAYLOAD_BASE_LEN + 1 + RELAY_LEN_MAX);
        let mut flags = 0;
        if self.relay_url.is_some() {
            flags |= FLAG_RELAY;
        }
        payload.push(flags);
        payload.extend_from_slice(self.endpoint_id.as_bytes());
        payload.extend_from_slice(&self.service_id.to_bytes());
        payload.extend_from_slice(&self.token.to_bytes());

        if let Some(relay_url) = &self.relay_url {
            let relay = relay_url.to_string();
            let relay_bytes = relay.as_bytes();
            if relay_bytes.len() > RELAY_LEN_MAX {
                return Err(JoinUriError::PayloadTooLong);
            }
            payload.push(relay_bytes.len() as u8);
            payload.extend_from_slice(relay_bytes);
        }

        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        let uri = format!("{SCHEME}://{HOST}/{VERSION}/{encoded}");
        if uri.len() > URI_LEN_MAX {
            return Err(JoinUriError::PayloadTooLong);
        }
        Ok(uri)
    }
}

impl fmt::Debug for JoinUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinUri")
            .field("endpoint_id", &self.endpoint_id)
            .field("service_id", &self.service_id)
            .field("token", &self.token)
            .field("relay_url", &self.relay_url)
            .finish()
    }
}

impl FromStr for JoinUri {
    type Err = JoinUriError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > URI_LEN_MAX {
            return Err(JoinUriError::PayloadTooLong);
        }
        let url = url::Url::parse(value)?;
        if url.scheme() != SCHEME
            || url.host_str() != Some(HOST)
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(JoinUriError::InvalidStructure);
        }

        let Some(encoded) = url.path().strip_prefix(&format!("/{VERSION}/")) else {
            return Err(JoinUriError::UnsupportedVersion);
        };
        if encoded.is_empty() || encoded.contains('/') {
            return Err(JoinUriError::InvalidStructure);
        }
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| JoinUriError::InvalidPayload)?;
        if payload.len() < PAYLOAD_BASE_LEN {
            return Err(JoinUriError::InvalidPayload);
        }

        let flags = payload[0];
        if flags & !FLAGS_KNOWN != 0 {
            return Err(JoinUriError::UnsupportedFlags);
        }
        let endpoint_bytes: [u8; 32] = payload[1..33]
            .try_into()
            .map_err(|_| JoinUriError::InvalidPayload)?;
        let endpoint_id =
            EndpointId::from_bytes(&endpoint_bytes).map_err(|_| JoinUriError::InvalidPayload)?;
        let service_id = ServiceId::from_bytes(
            payload[33..49]
                .try_into()
                .map_err(|_| JoinUriError::InvalidPayload)?,
        );
        let token = AccessToken::from_bytes(
            payload[49..81]
                .try_into()
                .map_err(|_| JoinUriError::InvalidPayload)?,
        );

        let relay_url = if flags & FLAG_RELAY == 0 {
            if payload.len() != PAYLOAD_BASE_LEN {
                return Err(JoinUriError::InvalidPayload);
            }
            None
        } else {
            if payload.len() <= PAYLOAD_BASE_LEN {
                return Err(JoinUriError::InvalidPayload);
            }
            let relay_len = payload[PAYLOAD_BASE_LEN] as usize;
            if relay_len == 0
                || relay_len > RELAY_LEN_MAX
                || payload.len() != PAYLOAD_BASE_LEN + 1 + relay_len
            {
                return Err(JoinUriError::InvalidPayload);
            }
            let relay = std::str::from_utf8(&payload[PAYLOAD_BASE_LEN + 1..])
                .map_err(|_| JoinUriError::InvalidPayload)?;
            Some(
                relay
                    .parse::<RelayUrl>()
                    .map_err(|e| JoinUriError::RelayUrlParse(e.to_string()))?,
            )
        };

        Ok(Self::new(endpoint_id, service_id, token, relay_url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join_uri(relay_url: Option<RelayUrl>) -> JoinUri {
        let endpoint_id = iroh::SecretKey::from_bytes(&[7; 32]).public();
        JoinUri::new(
            endpoint_id,
            ServiceId::from_bytes([8; 16]),
            AccessToken::from_bytes([9; 32]),
            relay_url,
        )
    }

    fn encoded_payload(payload: Vec<u8>) -> String {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        format!("{SCHEME}://{HOST}/{VERSION}/{encoded}")
    }

    fn base_payload() -> Vec<u8> {
        let uri = join_uri(None).expose_secret_uri();
        assert!(uri.is_ok(), "fixed Join URI must encode");
        let Ok(uri) = uri else {
            unreachable!();
        };
        let Some(encoded) = uri.rsplit('/').next() else {
            unreachable!();
        };
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded);
        assert!(payload.is_ok(), "fixed payload must decode");
        payload.unwrap_or_default()
    }

    #[test]
    fn encodes_stable_v1_test_vector() {
        let uri = join_uri(None);
        let value = uri.expose_secret_uri();
        assert!(value.is_ok(), "fixed Join URI must encode");
        let value = value.unwrap_or_default();
        assert_eq!(
            value,
            concat!(
                "sculk://join/v1/",
                "AOpKbGPinFIKvvVQexMuxfmVR3auvr57kkIe6mkURtIs",
                "CAgICAgICAgICAgICAgICAkJCQkJCQkJCQkJCQkJCQkJ",
                "CQkJCQkJCQkJCQkJCQkJ"
            )
        );
    }

    #[test]
    fn roundtrips_fixed_binary_payload() {
        let uri = join_uri(None);
        let value = uri.expose_secret_uri().unwrap_or_default();

        let parsed: Result<JoinUri, _> = value.parse();
        assert!(parsed.is_ok());
        assert_eq!(parsed.ok(), Some(uri));
    }

    #[test]
    fn roundtrips_custom_relay() {
        let relay = "https://relay.example.com".parse().ok();
        let uri = join_uri(relay);
        let value = uri.expose_secret_uri().unwrap_or_default();
        let parsed = value.parse::<JoinUri>();
        assert!(parsed.is_ok());
        assert_eq!(parsed.ok(), Some(uri));
    }

    #[test]
    fn rejects_extra_uri_components() {
        let value = join_uri(None).expose_secret_uri().unwrap_or_default();
        assert!(format!("{value}?x=1").parse::<JoinUri>().is_err());
        assert!(format!("{value}#x").parse::<JoinUri>().is_err());
    }

    #[test]
    fn rejects_legacy_and_unknown_versions() {
        let endpoint_id = join_uri(None).endpoint_id();
        let legacy = format!("sculk://{endpoint_id}");
        assert!(matches!(
            legacy.parse::<JoinUri>(),
            Err(JoinUriError::InvalidStructure)
        ));

        let payload = base_payload();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        let unknown = format!("sculk://join/v2/{encoded}");
        assert!(matches!(
            unknown.parse::<JoinUri>(),
            Err(JoinUriError::UnsupportedVersion)
        ));
    }

    #[test]
    fn rejects_unknown_flags_and_trailing_data() {
        let mut unknown_flags = base_payload();
        unknown_flags[0] = 0b1000_0000;
        assert!(matches!(
            encoded_payload(unknown_flags).parse::<JoinUri>(),
            Err(JoinUriError::UnsupportedFlags)
        ));

        let mut trailing = base_payload();
        trailing.push(0);
        assert!(matches!(
            encoded_payload(trailing).parse::<JoinUri>(),
            Err(JoinUriError::InvalidPayload)
        ));
    }

    #[test]
    fn rejects_invalid_payload_lengths() {
        let payload = base_payload();
        assert!(matches!(
            encoded_payload(Vec::new()).parse::<JoinUri>(),
            Err(JoinUriError::InvalidStructure)
        ));
        for len in [1, PAYLOAD_BASE_LEN - 1] {
            assert!(matches!(
                encoded_payload(payload[..len].to_vec()).parse::<JoinUri>(),
                Err(JoinUriError::InvalidPayload)
            ));
        }

        let mut relay_missing_length = payload.clone();
        relay_missing_length[0] = FLAG_RELAY;
        assert!(matches!(
            encoded_payload(relay_missing_length).parse::<JoinUri>(),
            Err(JoinUriError::InvalidPayload)
        ));

        let mut relay_length_mismatch = payload;
        relay_length_mismatch[0] = FLAG_RELAY;
        relay_length_mismatch.extend_from_slice(&[2, b'a']);
        assert!(matches!(
            encoded_payload(relay_length_mismatch).parse::<JoinUri>(),
            Err(JoinUriError::InvalidPayload)
        ));
    }

    #[test]
    fn rejects_invalid_relay_text() {
        let mut empty_relay = base_payload();
        empty_relay[0] = FLAG_RELAY;
        empty_relay.push(0);
        assert!(matches!(
            encoded_payload(empty_relay).parse::<JoinUri>(),
            Err(JoinUriError::InvalidPayload)
        ));

        let mut invalid_utf8 = base_payload();
        invalid_utf8[0] = FLAG_RELAY;
        invalid_utf8.extend_from_slice(&[1, 0xff]);
        assert!(matches!(
            encoded_payload(invalid_utf8).parse::<JoinUri>(),
            Err(JoinUriError::InvalidPayload)
        ));

        let mut invalid_url = base_payload();
        invalid_url[0] = FLAG_RELAY;
        invalid_url.extend_from_slice(&[3, b'b', b'a', b'd']);
        assert!(matches!(
            encoded_payload(invalid_url).parse::<JoinUri>(),
            Err(JoinUriError::RelayUrlParse(_))
        ));
    }

    #[test]
    fn enforces_uri_and_relay_limits() {
        let prefix = "https://relay.example/";
        let relay_max = format!("{prefix}{}", "a".repeat(RELAY_LEN_MAX - prefix.len()));
        assert_eq!(relay_max.len(), RELAY_LEN_MAX);
        let relay = relay_max.parse::<RelayUrl>();
        assert!(relay.is_ok(), "maximum relay URL must parse");
        let Some(relay) = relay.ok() else {
            unreachable!();
        };
        let encoded = join_uri(Some(relay)).expose_secret_uri();
        assert!(encoded.is_ok(), "maximum relay URL must encode");
        assert!(encoded.is_ok_and(|value| value.len() <= URI_LEN_MAX));

        let relay_too_long = format!("{relay_max}a").parse::<RelayUrl>();
        assert!(relay_too_long.is_ok(), "oversized relay URL must parse");
        let Some(relay_too_long) = relay_too_long.ok() else {
            unreachable!();
        };
        assert!(matches!(
            join_uri(Some(relay_too_long)).expose_secret_uri(),
            Err(JoinUriError::PayloadTooLong)
        ));

        let oversized = format!("sculk://join/v1/{}", "a".repeat(URI_LEN_MAX));
        assert!(matches!(
            oversized.parse::<JoinUri>(),
            Err(JoinUriError::PayloadTooLong)
        ));
    }

    #[test]
    fn debug_output_redacts_access_token() {
        let uri = join_uri(None);
        let debug = format!("{uri:?}");
        assert!(debug.contains("AccessToken(REDACTED)"));
        assert!(!debug.contains("09090909"));
    }
}
