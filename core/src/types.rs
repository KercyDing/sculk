//! 对 iroh 公共类型的 newtype 封装。

use std::fmt;
use std::str::FromStr;

use subtle::ConstantTimeEq;

/// Relay 服务器地址，封装 [`iroh::RelayUrl`]。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelayUrl(pub(crate) iroh::RelayUrl);

impl fmt::Display for RelayUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RelayUrl {
    type Err = iroh::RelayUrlParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<iroh::RelayUrl>().map(Self)
    }
}

impl From<iroh::RelayUrl> for RelayUrl {
    fn from(url: iroh::RelayUrl) -> Self {
        Self(url)
    }
}

/// 节点密钥，封装 [`iroh::SecretKey`]。
#[derive(Debug, Clone)]
pub struct SecretKey(pub(crate) iroh::SecretKey);

impl SecretKey {
    /// 从 32 字节数组创建密钥。
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self(iroh::SecretKey::from_bytes(bytes))
    }

    /// 导出 32 字节数组。
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// 获取对应的公钥。
    pub fn public(&self) -> iroh::EndpointId {
        self.0.public()
    }
}

impl From<iroh::SecretKey> for SecretKey {
    fn from(key: iroh::SecretKey) -> Self {
        Self(key)
    }
}

/// 服务的稳定逻辑标识。
///
/// 复制为新服务时应生成新值；移动或恢复原服务时应保留原值。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceId([u8; 16]);

impl ServiceId {
    /// 生成新的随机服务标识。
    pub fn generate() -> Self {
        Self(rand::random())
    }

    /// 从固定字节创建服务标识。
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// 返回用于二进制协议的固定字节。
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Debug for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ServiceId").field(&self.0).finish()
    }
}

/// 仅在当前 Host 会话中有效的访问令牌。
///
/// 此类型故意不实现 `Display`，且 `Debug` 输出会脱敏，避免令牌进入日志。
#[derive(Clone, PartialEq, Eq)]
pub struct AccessToken([u8; 32]);

impl AccessToken {
    /// 生成新的 256 位随机令牌。
    pub fn generate() -> Self {
        Self(rand::random())
    }

    /// 从固定字节创建令牌，主要用于 URI 解析与测试。
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// 返回用于二进制协议的固定字节。
    pub const fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// 以常量时间比较两个令牌。
    pub fn matches(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(REDACTED)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_id_has_fixed_binary_layout() {
        let bytes = [0x5a; 16];
        let service_id = ServiceId::from_bytes(bytes);
        assert_eq!(service_id.to_bytes(), bytes);
    }

    #[test]
    fn access_token_compares_all_fixed_bytes() {
        let token = AccessToken::from_bytes([0x5a; 32]);
        let same = AccessToken::from_bytes([0x5a; 32]);
        let mut different_bytes = [0x5a; 32];
        different_bytes[31] ^= 1;
        let different = AccessToken::from_bytes(different_bytes);

        assert!(token.matches(&same));
        assert!(!token.matches(&different));
    }

    #[test]
    fn access_token_debug_is_redacted() {
        let token = AccessToken::from_bytes([0x5a; 32]);
        assert_eq!(format!("{token:?}"), "AccessToken(REDACTED)");
    }

    #[test]
    fn generated_identifiers_are_not_reused() {
        let service_a = ServiceId::generate();
        let service_b = ServiceId::generate();
        let token_a = AccessToken::generate();
        let token_b = AccessToken::generate();

        assert_ne!(service_a, service_b);
        assert!(!token_a.matches(&token_b));
    }
}
