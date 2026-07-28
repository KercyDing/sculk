//! 固定长度控制流认证协议。

use super::*;
use crate::types::{AccessToken, ServiceId};

const CONTROL_VERSION: u8 = 0x01;
const CONTROL_REQUEST_LEN: usize = 1 + 16 + 32;
const CONTROL_OK: u8 = 0x00;
const CONTROL_REJECTED: u8 = 0x01;

pub(super) struct ControlRequest {
    pub(super) service_id: ServiceId,
    pub(super) token: AccessToken,
}

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
    let result = match recv.read_to_end(1).await {
        Ok(result) => result,
        Err(error) => {
            if conn
                .close_reason()
                .as_ref()
                .is_some_and(is_auth_failure_close)
            {
                return Err(crate::error::TunnelError::AuthRejectedByHost.into());
            }
            return Err(crate::error::TunnelError::ReadAuthResult(error.into()).into());
        }
    };
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
    let (mut send, request) = auth_accept(conn).await?;
    let valid = request.service_id == expected_service_id && request.token.matches(expected_token);
    auth_respond(&mut send, valid).await?;
    Ok(valid)
}

/// 接收控制流并解析固定长度请求；调用方必须始终回写一个统一的决定。
pub(super) async fn auth_accept(conn: &Connection) -> crate::Result<(SendStream, ControlRequest)> {
    let (mut send, mut recv) = conn
        .accept_bi()
        .await
        .map_err(|e| crate::error::TunnelError::AcceptAuthStream(e.into()))?;
    if recv.id().index() != 0 {
        auth_respond(&mut send, false).await?;
        return Err(crate::error::TunnelError::AuthRejectedByHost.into());
    }
    let data = tokio::select! {
        biased;
        extra = conn.accept_bi() => {
            extra.map_err(|e| crate::error::TunnelError::AcceptAuthStream(e.into()))?;
            auth_respond(&mut send, false).await?;
            return Err(crate::error::TunnelError::AuthRejectedByHost.into());
        }
        data = recv.read_to_end(CONTROL_REQUEST_LEN) => {
            data.map_err(|e| crate::error::TunnelError::ReadAuthPayload(e.into()))?
        }
    };
    if data.len() != CONTROL_REQUEST_LEN || data[0] != CONTROL_VERSION {
        auth_respond(&mut send, false).await?;
        return Err(crate::error::TunnelError::AuthRejectedByHost.into());
    }
    let service_id = ServiceId::from_bytes(
        data[1..17]
            .try_into()
            .map_err(|_| crate::error::TunnelError::AuthRejectedByHost)?,
    );
    let token = AccessToken::from_bytes(
        data[17..]
            .try_into()
            .map_err(|_| crate::error::TunnelError::AuthRejectedByHost)?,
    );
    Ok((send, ControlRequest { service_id, token }))
}

/// 对控制流请求写入成功或统一拒绝响应。
pub(super) async fn auth_respond(send: &mut SendStream, valid: bool) -> crate::Result<()> {
    send.write_all(&[if valid { CONTROL_OK } else { CONTROL_REJECTED }])
        .await
        .map_err(|e| crate::error::TunnelError::WriteAuthDecision(e.into()))?;
    send.finish()
        .map_err(|e| crate::error::TunnelError::FinishAuthStream(e.into()))?;
    Ok(())
}
