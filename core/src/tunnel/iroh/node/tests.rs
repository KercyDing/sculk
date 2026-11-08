use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

use std::net::Ipv4Addr;
use std::sync::LazyLock;

static LOCAL_RELAY: LazyLock<Option<RelayUrl>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        let Some((url, server)) = runtime.block_on(async {
            let mut config = iroh_relay::server::ServerConfig::default();
            config.relay = Some(iroh_relay::server::RelayConfig::new((
                Ipv4Addr::LOCALHOST,
                0,
            )));
            let server = iroh_relay::server::Server::spawn(config).await.ok()?;
            let url = format!("http://{}", server.http_addr()?).parse().ok()?;
            Some((url, server))
        }) else {
            return;
        };
        let _ = tx.send(Some(url));
        runtime.block_on(async move {
            let _server = server;
            std::future::pending::<()>().await;
        });
    });
    rx.recv().ok().flatten()
});

fn local_relay() -> Option<&'static RelayUrl> {
    LOCAL_RELAY.as_ref()
}

fn test_node_options() -> NodeOptions {
    let mut options = NodeOptions::default();
    if let Some(relay) = local_relay() {
        options.relay_url = Some(relay.clone());
    }
    options
}

async fn bind_test_node() -> std::result::Result<SculkNode, SculkNodeError> {
    let result = SculkNode::bind(test_node_options()).await;
    if let Err(error) = &result {
        eprintln!("bind_test_node failed: {error}");
    }
    result
}

async fn recv_status_where(
    updates: &mut HostedServiceStatusSubscription,
    predicate: impl Fn(&HostedServiceStatus) -> bool,
) -> Option<HostedServiceStatus> {
    for _ in 0..16 {
        let update = tokio::time::timeout(Duration::from_secs(5), updates.recv()).await;
        let Ok(Some(status)) = update else {
            return None;
        };
        if predicate(&status) {
            return Some(status);
        }
    }
    None
}

async fn join_node(
    node: &SculkNode,
    uri: &JoinUri,
) -> crate::Result<(IrohTunnel, mpsc::Receiver<TunnelEvent>)> {
    IrohTunnel::join_direct(uri, node.inner.endpoint.addr(), 0, JoinConfig::default()).await
}

mod basics;
mod connections;
mod lifecycle;
mod token;
