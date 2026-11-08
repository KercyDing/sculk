use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

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
