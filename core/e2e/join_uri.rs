use sculk::tunnel::{AccessToken, HostConfig, IrohTunnel, JoinConfig, JoinUri, ServiceId};

use crate::support::{TestResult, echo_once, spawn_relay, start_echo_target};

#[tokio::test]
async fn full_join_uri_forwards_tcp() -> TestResult {
    let Some((relay, _server)) = spawn_relay().await else {
        return Ok(());
    };
    let (target_port, target_task) = start_echo_target().await?;

    let (host, uri, _host_events) = IrohTunnel::host(
        target_port,
        None,
        Some(relay),
        ServiceId::generate(),
        AccessToken::generate(),
        HostConfig::default(),
    )
    .await?;
    let uri: JoinUri = uri.expose_secret_uri()?.parse()?;

    let (join, _join_events) = IrohTunnel::join(&uri, 0, JoinConfig::default()).await?;
    let local_addr = join.local_addr().ok_or("join did not bind a local port")?;

    echo_once(local_addr).await?;

    join.close().await;
    host.close().await;
    target_task.abort();
    Ok(())
}
