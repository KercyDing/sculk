use super::*;

#[tokio::test]
async fn joins_and_forwards_to_the_selected_service() {
    // IPv6 loopback proves routing preserves the full SocketAddr, not only the target port.
    let listener = tokio::net::TcpListener::bind("[::1]:0").await;
    assert!(listener.is_ok());
    let Ok(listener) = listener else {
        return;
    };
    let target_addr = listener.local_addr();
    assert!(target_addr.is_ok());
    let Ok(target_addr) = target_addr else {
        return;
    };
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut data = [0_u8; 4];
        if stream.read_exact(&mut data).await.is_ok() {
            let _ = stream.write_all(&data).await;
            let _ = release_rx.await;
        }
    });

    let node = bind_test_node().await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service = node
        .start_service(HostedServiceOptions {
            service_id: ServiceId::generate(),
            target_addr,
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
    let status_updates = service.subscribe_status().await;
    assert!(status_updates.is_ok());
    let Ok(mut status_updates) = status_updates else {
        node.close().await;
        return;
    };
    assert!(status_updates.recv().await.is_some());
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
    let connected =
        recv_status_where(&mut status_updates, |status| status.connection_count == 1).await;
    assert!(
        connected.is_some(),
        "authenticated connection status missing"
    );
    let rotated = service.rotate_token().await;
    assert!(rotated.is_ok(), "rotation after authentication failed");
    let local_addr = join.local_addr();
    assert!(local_addr.is_some());
    let Some(local_addr) = local_addr else {
        join.close().await;
        node.close().await;
        return;
    };
    let client = tokio::net::TcpStream::connect(local_addr).await;
    assert!(client.is_ok());
    let Ok(mut client) = client else {
        join.close().await;
        node.close().await;
        return;
    };
    let write = client.write_all(b"ping").await;
    assert!(write.is_ok());
    let mut echoed = [0_u8; 4];
    let read = tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echoed)).await;
    assert!(matches!(read, Ok(Ok(_))));
    assert_eq!(&echoed, b"ping");
    let bridging = recv_status_where(&mut status_updates, |status| status.bridge_count == 1).await;
    assert!(bridging.is_some(), "active bridge status missing");
    let _ = release_tx.send(());
    drop(client);
    let bridge_closed =
        recv_status_where(&mut status_updates, |status| status.bridge_count == 0).await;
    assert!(bridge_closed.is_some(), "closed bridge status missing");
    join.close().await;
    node.close().await;
}

#[tokio::test]
async fn rejects_unknown_service_and_wrong_token_identically() {
    let node = bind_test_node().await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service_id = ServiceId::generate();
    let token = AccessToken::generate();
    let started = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: Some(TokenState::new(token.clone(), SystemTime::now())),
            token_refresh: TokenRefreshPolicy::Never,
            config: HostConfig::default(),
        })
        .await;
    assert!(started.is_ok());

    for (requested_service, requested_token) in [
        (ServiceId::generate(), token.clone()),
        (service_id, AccessToken::generate()),
    ] {
        let uri = JoinUri::new(node.endpoint_id(), requested_service, requested_token, None);
        let rejected = join_node(&node, &uri).await;
        assert!(
            matches!(
                rejected,
                Err(crate::SculkError::Tunnel(
                    crate::error::TunnelError::AuthRejectedByHost
                ))
            ),
            "rejection must use the public authorization error"
        );
    }
    node.close().await;
}

#[tokio::test]
async fn rejects_additional_stream_before_authentication() -> TestResult {
    let node = bind_test_node().await?;
    let client = build_endpoint(None, local_relay())
        .clear_address_lookup()
        .bind()
        .await?;
    let conn = client.connect(node.inner.endpoint.addr(), ALPN).await?;
    let (_control_send, _control_recv) = conn.open_bi().await?;
    let (mut extra_send, _extra_recv) = conn.open_bi().await?;
    extra_send.write_all(b"x").await?;
    extra_send.finish()?;

    let closed = tokio::time::timeout(Duration::from_secs(3), conn.closed()).await?;
    assert!(matches!(
        closed,
        ConnectionError::ApplicationClosed(ApplicationClose {
            error_code,
            ..
        }) if error_code == CLOSE_AUTH_FAILED
    ));

    client.close().await;
    node.close().await;
    Ok(())
}

#[tokio::test]
async fn reports_fixed_local_port_conflict() -> TestResult {
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let occupied_addr = occupied.local_addr()?;
    let node = bind_test_node().await?;
    let service = node
        .start_service(HostedServiceOptions {
            service_id: ServiceId::generate(),
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await?;
    let uri = service.join_uri().await?;

    let join = IrohTunnel::join_direct(
        &uri,
        node.inner.endpoint.addr(),
        occupied_addr.port(),
        JoinConfig::default(),
    )
    .await;
    assert_eq!(
        join.as_ref().err().map(crate::SculkError::category),
        Some(ErrorCategory::LocalPortUnavailable)
    );
    if let Ok((tunnel, _)) = join {
        tunnel.close().await;
    }
    node.close().await;
    Ok(())
}

#[tokio::test]
async fn bounds_local_forwarding_sessions() -> TestResult {
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let target_addr = target.local_addr()?;
    let node = bind_test_node().await?;
    let service = node
        .start_service(HostedServiceOptions {
            service_id: ServiceId::generate(),
            target_addr,
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await?;
    let uri = service.join_uri().await?;
    let (join, _) = IrohTunnel::join_direct(
        &uri,
        node.inner.endpoint.addr(),
        0,
        JoinConfig::new().local_sessions_max(NonZeroUsize::MIN),
    )
    .await?;
    let Some(local_addr) = join.local_addr() else {
        join.close().await;
        node.close().await;
        return Err(std::io::Error::other("Join listener address is missing").into());
    };

    let mut first_local = tokio::net::TcpStream::connect(local_addr).await?;
    first_local.write_all(b"a").await?;
    let (first_target, _) = tokio::time::timeout(Duration::from_secs(3), target.accept()).await??;

    let mut second_local = tokio::net::TcpStream::connect(local_addr).await?;
    second_local.write_all(b"b").await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), target.accept())
            .await
            .is_err(),
        "second session bypassed the configured limit"
    );

    drop(first_local);
    drop(first_target);
    tokio::time::timeout(Duration::from_secs(3), target.accept()).await??;

    drop(second_local);
    join.close().await;
    node.close().await;
    Ok(())
}

#[tokio::test]
async fn join_reconnects_after_service_republished_with_same_uri() -> TestResult {
    let node = bind_test_node().await?;
    let service_id = ServiceId::generate();
    let target_addr = SocketAddr::from(([127, 0, 0, 1], 25565));
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
    let (join, mut events) = join_node(&node, &uri).await?;

    node.stop_service(service_id).await?;
    node.start_service(HostedServiceOptions {
        service_id,
        target_addr,
        token_state: Some(token_state),
        token_refresh: TokenRefreshPolicy::Never,
        config: HostConfig::default(),
    })
    .await?;

    let reconnected = tokio::time::timeout(Duration::from_secs(5), async {
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

    join.close().await;
    node.close().await;
    Ok(())
}

#[tokio::test]
async fn enforces_service_connection_limit() {
    let node = bind_test_node().await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service = node
        .start_service(HostedServiceOptions {
            service_id: ServiceId::generate(),
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::new().max_players(Some(1)),
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
    let status = service.subscribe_status().await;
    assert!(status.is_ok());
    let Ok(mut status) = status else {
        node.close().await;
        return;
    };
    assert!(status.recv().await.is_some());
    let events = service.subscribe().await;
    assert!(events.is_ok());
    let Ok(mut events) = events else {
        node.close().await;
        return;
    };

    let first = join_node(&node, &uri).await;
    assert!(first.is_ok());
    let Ok((first, _)) = first else {
        node.close().await;
        return;
    };
    let first_connected =
        recv_status_where(&mut status, |status| status.connection_count == 1).await;
    assert!(first_connected.is_some());

    let second = join_node(&node, &uri).await;
    let second = second.ok().map(|(tunnel, _)| tunnel);
    let rejected = async {
        for _ in 0..16 {
            let event = events.recv().await.ok()?;
            if matches!(event, TunnelEvent::PlayerRejected { .. }) {
                return Some(());
            }
        }
        None
    };
    let rejected = tokio::time::timeout(Duration::from_secs(5), rejected).await;
    assert!(matches!(rejected, Ok(Some(()))));
    assert_eq!(
        service
            .status()
            .await
            .ok()
            .map(|status| status.connection_count),
        Some(1)
    );

    if let Some(second) = second {
        second.close().await;
    }
    first.close().await;
    node.close().await;
}

#[tokio::test]
async fn bounds_unauthenticated_connections_globally() {
    let mut options = test_node_options();
    options.unauthenticated_connections_max = NonZeroUsize::MIN;
    let node = SculkNode::bind(options).await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let client_a = build_endpoint(None, local_relay())
        .clear_address_lookup()
        .bind()
        .await;
    assert!(client_a.is_ok());
    let Ok(client_a) = client_a else {
        node.close().await;
        return;
    };
    let node_addr = node.inner.endpoint.addr();
    let conn_a = client_a.connect(node_addr, ALPN).await;
    assert!(conn_a.is_ok());
    let Ok(conn_a) = conn_a else {
        client_a.close().await;
        node.close().await;
        return;
    };

    let bounded = async {
        for _ in 0..100 {
            let status = node.status().await;
            if status.unauthenticated_connection_count == 1 {
                return Some(status);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        None
    };
    let bounded = tokio::time::timeout(Duration::from_secs(3), bounded).await;
    assert!(matches!(
        bounded,
        Ok(Some(SculkNodeStatus {
            unauthenticated_connection_count: 1,
            unauthenticated_connections_max: 1,
            ..
        }))
    ));

    conn_a.close(CLOSE_AUTH_FAILED, b"test complete");
    client_a.close().await;
    node.close().await;
}

#[tokio::test]
async fn records_target_connection_failures_in_status() {
    let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await;
    assert!(unused.is_ok());
    let Ok(unused) = unused else {
        return;
    };
    let target_addr = unused.local_addr();
    assert!(target_addr.is_ok());
    let Ok(target_addr) = target_addr else {
        return;
    };

    let node = bind_test_node().await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service = node
        .start_service(HostedServiceOptions {
            service_id: ServiceId::generate(),
            target_addr,
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
    let local_addr = join.local_addr();
    assert!(local_addr.is_some());
    let Some(local_addr) = local_addr else {
        join.close().await;
        node.close().await;
        return;
    };
    drop(unused);
    let client = tokio::net::TcpStream::connect(local_addr).await;
    assert!(client.is_ok());
    let Ok(mut client) = client else {
        join.close().await;
        node.close().await;
        return;
    };
    assert!(client.write_all(b"ping").await.is_ok());

    let error_recorded = async {
        for _ in 0..100 {
            let status = service.status().await.ok()?;
            if status.last_error == Some(ErrorCategory::TargetUnavailable) {
                return Some(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        None
    };
    let error_recorded = tokio::time::timeout(Duration::from_secs(7), error_recorded).await;
    assert!(matches!(error_recorded, Ok(Some(()))));
    join.close().await;
    node.close().await;
}
