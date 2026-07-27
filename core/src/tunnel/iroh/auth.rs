//! 固定长度控制流认证协议。

use super::*;
use crate::types::{AccessToken, ServiceId};

const CONTROL_VERSION: u8 = 0x01;
const CONTROL_REQUEST_LEN: usize = 1 + 16 + 32;
const CONTROL_OK: u8 = 0x00;
const CONTROL_REJECTED: u8 = 0x01;

/// Join 侧打开连接的第一个双向流并完成服务选择与认证。
pub(super) async fn auth_send(
    conn: &Connection,
    service_id: ServiceId,
    token: &AccessToken,
) -> crate::Result<()> {
    tokio::time::timeout(AUTH_TIMEOUT, auth_send_inner(conn, service_id, token))
        .await
        .map_err(|_| crate::error::TunnelError::AuthTimedOut)?
}

async fn auth_send_inner(
    conn: &Connection,
    service_id: ServiceId,
    token: &AccessToken,
) -> crate::Result<()> {
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| crate::error::TunnelError::OpenAuthStream(e.into()))?;
    let mut request = [0_u8; CONTROL_REQUEST_LEN];
    request[0] = CONTROL_VERSION;
    request[1..17].copy_from_slice(&service_id.to_bytes());
    request[17..].copy_from_slice(&token.to_bytes());
    send.write_all(&request)
        .await
        .map_err(|e| crate::error::TunnelError::WriteAuthPayload(e.into()))?;
    send.finish()
        .map_err(|e| crate::error::TunnelError::FinishAuthStream(e.into()))?;
    let result = recv
        .read_to_end(1)
        .await
        .map_err(|e| crate::error::TunnelError::ReadAuthResult(e.into()))?;
    if result.as_slice() == [CONTROL_OK] {
        Ok(())
    } else {
        Err(crate::error::TunnelError::AuthRejectedByHost.into())
    }
}

/// Host 侧只接受客户端的首个双向控制流，并验证服务与令牌。
pub(super) async fn auth_verify(
    conn: &Connection,
    expected_service_id: ServiceId,
    expected_token: &AccessToken,
) -> crate::Result<bool> {
    tokio::time::timeout(
        AUTH_TIMEOUT,
        auth_verify_inner(conn, expected_service_id, expected_token),
    )
    .await
    .map_err(|_| crate::error::TunnelError::AuthTimedOut)?
}

async fn auth_verify_inner(
    conn: &Connection,
    expected_service_id: ServiceId,
    expected_token: &AccessToken,
) -> crate::Result<bool> {
    let (mut send, mut recv) = conn
        .accept_bi()
        .await
        .map_err(|e| crate::error::TunnelError::AcceptAuthStream(e.into()))?;
    let data = recv
        .read_to_end(CONTROL_REQUEST_LEN)
        .await
        .map_err(|e| crate::error::TunnelError::ReadAuthPayload(e.into()))?;
    let valid = data.len() == CONTROL_REQUEST_LEN
        && data[0] == CONTROL_VERSION
        && data[1..17] == expected_service_id.to_bytes()
        && AccessToken::from_bytes(
            data.get(17..)
                .ok_or(crate::error::TunnelError::AuthRejectedByHost)?
                .try_into()
                .map_err(|_| crate::error::TunnelError::AuthRejectedByHost)?,
        )
        .matches(expected_token);
    send.write_all(&[if valid { CONTROL_OK } else { CONTROL_REJECTED }])
        .await
        .map_err(|e| crate::error::TunnelError::WriteAuthDecision(e.into()))?;
    send.finish()
        .map_err(|e| crate::error::TunnelError::FinishAuthStream(e.into()))?;
    Ok(valid)
}
