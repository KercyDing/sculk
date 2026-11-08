use super::*;

#[tokio::test]
async fn stopping_one_service_keeps_node_and_other_service() {
    let node_result = SculkNode::bind(NodeOptions::default()).await;
    assert!(node_result.is_ok());
    let Ok(node) = node_result else {
        return;
    };
    let first_id = ServiceId::generate();
    let second_id = ServiceId::generate();
    let first = node
        .start_service(HostedServiceOptions {
            service_id: first_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await;
    assert!(first.is_ok());
    let Ok(first) = first else {
        node.close().await;
        return;
    };
    let first_status = first.subscribe_status().await;
    assert!(first_status.is_ok());
    let Ok(mut first_status) = first_status else {
        node.close().await;
        return;
    };
    let initial = first_status.recv().await;
    assert!(matches!(
        initial,
        Some(HostedServiceStatus {
            phase: HostedServicePhase::Active,
            uri_generation: 1,
            connection_count: 0,
            bridge_count: 0,
            ..
        })
    ));
    let second = node
        .start_service(HostedServiceOptions {
            service_id: second_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::After(Duration::from_millis(20)),
            config: HostConfig::default(),
        })
        .await;
    assert!(second.is_ok());
    let Ok(second) = second else {
        return;
    };
    let node_status = node.status().await;
    assert_eq!(node_status.endpoint_id, node.endpoint_id());
    assert!(node_status.online);
    assert!(node_status.relay_url.is_none());
    assert_eq!(node_status.service_count, 2);
    assert_eq!(node_status.unauthenticated_connection_count, 0);
    assert_eq!(
        node_status.unauthenticated_connections_max,
        NODE_UNAUTHENTICATED_CONNECTIONS_MAX.get()
    );
    let second_status = second.status().await;
    assert!(second_status.is_ok());
    assert_eq!(
        second_status.ok().map(|status| status.connection_count),
        Some(0)
    );
    let updates = second.subscribe().await;
    assert!(updates.is_ok());
    let Ok(mut updates) = updates else {
        return;
    };
    let first_uri = second.join_uri().await;
    assert!(first_uri.is_ok());
    let rotated_uri = second.rotate_token().await;
    assert!(rotated_uri.is_ok());
    let (Ok(first_uri), Ok(rotated_uri)) = (first_uri, rotated_uri) else {
        return;
    };
    assert_eq!(first_uri.endpoint_id(), rotated_uri.endpoint_id());
    assert_eq!(first_uri.service_id(), rotated_uri.service_id());
    assert!(!first_uri.token().matches(rotated_uri.token()));
    let second_status = second.status().await;
    assert_eq!(
        second_status.ok().map(|status| status.uri_generation),
        Some(2)
    );
    let rotated = tokio::time::timeout(Duration::from_secs(1), updates.recv()).await;
    assert!(matches!(rotated, Ok(Ok(TunnelEvent::TokenRotated))));
    assert_eq!(node.service_count().await, 2);

    let stopped = node.stop_service(first_id).await;
    assert!(stopped.is_ok());
    let stopped_status = first_status.recv().await;
    assert!(matches!(
        stopped_status,
        Some(HostedServiceStatus {
            phase: HostedServicePhase::Stopped,
            ..
        })
    ));
    assert!(matches!(
        first.status().await,
        Err(SculkNodeError::ServiceNotFound)
    ));
    assert_eq!(node.service_count().await, 1);
    assert!(second.join_uri().await.is_ok());
    node.close().await;
    let closed = node.status().await;
    assert!(!closed.online);
    assert_eq!(closed.service_count, 0);
}

#[tokio::test]
async fn stopping_service_closes_authenticated_connections() {
    let node = SculkNode::bind(NodeOptions::default()).await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service_id = ServiceId::generate();
    let service = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await;
    assert!(service.is_ok());
    let Ok(service) = service else {
        node.close().await;
        return;
    };
    let uri = service.join_uri().await;
    assert!(uri.is_ok());
    let Ok(uri) = uri else {
        node.close().await;
        return;
    };
    let join = join_node(&node, &uri).await;
    assert!(join.is_ok());
    let Ok((join, _)) = join else {
        node.close().await;
        return;
    };

    let connected = async {
        for _ in 0..100 {
            if service
                .status()
                .await
                .is_ok_and(|status| status.connection_count == 1)
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    };
    assert!(connected.await);
    assert!(node.stop_service(service_id).await.is_ok());

    let closed = async {
        for _ in 0..100 {
            if join
                .connections()
                .is_ok_and(|connections| connections.is_empty())
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    };
    assert!(closed.await, "authenticated connection remained after stop");
    assert!(matches!(
        service.status().await,
        Err(SculkNodeError::ServiceNotFound)
    ));

    join.close().await;
    node.close().await;
}

#[tokio::test]
async fn rejects_duplicate_service_ids() {
    let node = SculkNode::bind(NodeOptions::default()).await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service_id = ServiceId::generate();
    let first = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await;
    assert!(first.is_ok());
    let duplicate = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await;
    assert!(matches!(duplicate, Err(SculkNodeError::DuplicateService)));
    assert_eq!(node.service_count().await, 1);
    node.close().await;
}

#[tokio::test]
async fn dropping_last_node_handle_closes_endpoint() {
    let node = SculkNode::bind(NodeOptions::default()).await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let observer = node.inner.endpoint.clone();
    drop(node);

    let closed = tokio::time::timeout(Duration::from_secs(3), observer.closed()).await;
    assert!(closed.is_ok(), "Node Endpoint remained open after drop");
}
