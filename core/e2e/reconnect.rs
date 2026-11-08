use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use sculk::tunnel::{
    HostConfig, HostedServiceOptions, IrohTunnel, JoinConfig, JoinUri, NodeOptions, SculkNode,
    ServiceId, TokenRefreshPolicy, TunnelEvent,
};

use crate::support::{TestResult, echo_once, spawn_relay, start_echo_target};

#[tokio::test]
async fn join_reconnects_after_service_restart() -> TestResult {
    let Some((relay, _server)) = spawn_relay().await else {
        return Ok(());
    };
    let (target_port, target_task) = start_echo_target().await?;

    let node = SculkNode::bind(NodeOptions {
        relay_url: Some(relay),
        ..NodeOptions::default()
    })
    .await?;
    let service_id = ServiceId::generate();
    let target_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, target_port));
    let service = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr,
            token_state: None,
            token_refresh: TokenRefreshPolicy::Never,
            config: HostConfig::default(),
        })
        .await?;
    let uri = service.join_uri().await?;
    let token_state = service.token_state().await?;
    let uri: JoinUri = uri.expose_secret_uri()?.parse()?;

    let (join, mut events) = IrohTunnel::join(&uri, 0, JoinConfig::default()).await?;
    let local_addr = join.local_addr().ok_or("join did not bind a local port")?;
    echo_once(local_addr).await?;

    node.stop_service(service_id).await?;
    node.start_service(HostedServiceOptions {
        service_id,
        target_addr,
        token_state: Some(token_state),
        token_refresh: TokenRefreshPolicy::Never,
        config: HostConfig::default(),
    })
    .await?;

    let reconnected = tokio::time::timeout(Duration::from_secs(10), async {
        let mut reconnecting = false;
        while let Some(event) = events.recv().await {
            match event {
                TunnelEvent::Reconnecting { .. } => reconnecting = true,
                TunnelEvent::Reconnected => return reconnecting,
                _ => {}
            }
        }
        false
    })
    .await?;
    assert!(reconnected, "join did not reconnect after service restart");

    echo_once(local_addr).await?;

    join.close().await;
    node.close().await;
    target_task.abort();
    Ok(())
}
