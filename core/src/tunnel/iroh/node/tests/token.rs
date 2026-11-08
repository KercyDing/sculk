use super::*;

#[test]
fn rotation_schedule_handles_clock_jumps_without_catch_up() {
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let deadline = start + Duration::from_secs(120);
    let monotonic_start = Instant::now();
    let monotonic_deadline = monotonic_start + Duration::from_secs(120);
    let mut schedule =
        RotationSchedule::new(Duration::from_secs(120), deadline, monotonic_deadline);

    assert_eq!(
        schedule.delay(start, monotonic_start),
        ROTATION_CLOCK_CHECK_MAX,
        "long waits must be split into bounded clock checks"
    );
    let jumped_forward = start + Duration::from_secs(500);
    assert!(schedule.should_rotate(jumped_forward, monotonic_start));
    assert!(schedule.reset(jumped_forward, monotonic_start));
    assert!(!schedule.should_rotate(jumped_forward, monotonic_start));
    assert_eq!(
        schedule.deadline,
        jumped_forward + Duration::from_secs(120),
        "missed periods must collapse into one rotation"
    );

    let jumped_backward = start - Duration::from_secs(500);
    assert_eq!(
        schedule.delay(jumped_backward, monotonic_start),
        ROTATION_CLOCK_CHECK_MAX,
        "backward wall-clock jumps must not delay the monotonic deadline"
    );
    assert!(schedule.should_rotate(jumped_backward, monotonic_deadline));
}

#[test]
fn rotation_retry_backoff_is_bounded() {
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let monotonic_start = Instant::now();
    let mut schedule = RotationSchedule::new(Duration::from_secs(60), start, monotonic_start);

    schedule.record_failure();
    assert_eq!(schedule.retry_delay(), ROTATION_RETRY_MIN);
    schedule.record_failure();
    assert_eq!(schedule.retry_delay(), ROTATION_RETRY_MIN.saturating_mul(2));
    for _ in 0..64 {
        schedule.record_failure();
    }
    assert_eq!(schedule.retry_delay(), ROTATION_RETRY_MAX);
    assert_eq!(schedule.delay(start, monotonic_start), ROTATION_RETRY_MAX);
    assert!(schedule.should_rotate(start, monotonic_start));
}

#[test]
fn rejects_unrepresentable_rotation_deadline() {
    let deadline = rotation_deadline(SystemTime::UNIX_EPOCH, Some(Duration::MAX));
    assert!(matches!(
        deadline,
        Err(SculkNodeError::InvalidRefreshPeriod)
    ));
}

#[test]
fn token_policy_applies_across_publications() {
    let created_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let now = created_at + Duration::from_secs(60);
    let token = AccessToken::from_bytes([0x5a; 32]);
    let saved = TokenState::new(token.clone(), created_at);

    let always = resolve_token_state(
        TokenRefreshPolicy::Always,
        Some(saved.clone()),
        now,
        Instant::now(),
    );
    assert!(always.is_ok());
    let Some(always) = always.ok() else {
        return;
    };
    assert!(!always.state.token.matches(&token));
    assert_eq!(always.state.created_at, now);

    let never = resolve_token_state(
        TokenRefreshPolicy::Never,
        Some(saved.clone()),
        now,
        Instant::now(),
    );
    assert!(never.is_ok());
    let Some(never) = never.ok() else {
        return;
    };
    assert!(never.state.token.matches(&token));
    assert_eq!(never.state.created_at, created_at);
    assert!(never.deadline.is_none());

    let period = Duration::from_secs(3_600);
    let timed = resolve_token_state(
        TokenRefreshPolicy::After(period),
        Some(saved),
        now,
        Instant::now(),
    );
    assert!(timed.is_ok());
    let Some(timed) = timed.ok() else {
        return;
    };
    assert!(timed.state.token.matches(&token));
    assert_eq!(timed.state.created_at, created_at);
    assert_eq!(timed.deadline, Some(created_at + period));
}

#[test]
fn timed_policy_rotates_expired_or_future_state() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    let period = Duration::from_secs(3_600);
    for created_at in [now - period, now + Duration::from_secs(1)] {
        let token = AccessToken::from_bytes([0x5a; 32]);
        let resolved = resolve_token_state(
            TokenRefreshPolicy::After(period),
            Some(TokenState::new(token.clone(), created_at)),
            now,
            Instant::now(),
        );
        assert!(resolved.is_ok());
        let Some(resolved) = resolved.ok() else {
            return;
        };
        assert!(!resolved.state.token.matches(&token));
        assert_eq!(resolved.state.created_at, now);
        assert_eq!(resolved.deadline, Some(now + period));
    }

    assert!(matches!(
        resolve_token_state(
            TokenRefreshPolicy::After(Duration::ZERO),
            None,
            now,
            Instant::now()
        ),
        Err(SculkNodeError::InvalidRefreshPeriod)
    ));
}

#[tokio::test]
async fn token_policy_controls_uri_across_node_restarts() {
    let secret_key = SecretKey::from_bytes(&[0x3c; 32]);
    let service_id = ServiceId::from_bytes([0x4b; 16]);
    let target_addr = SocketAddr::from(([127, 0, 0, 1], 25565));

    let mut options = test_node_options();
    options.secret_key = Some(secret_key.clone());
    let first = SculkNode::bind(options.clone()).await;
    assert!(first.is_ok());
    let Ok(first) = first else {
        return;
    };
    let first_service = first
        .start_service(HostedServiceOptions {
            service_id,
            target_addr,
            token_state: None,
            token_refresh: TokenRefreshPolicy::Never,
            config: HostConfig::default(),
        })
        .await;
    assert!(first_service.is_ok());
    let Ok(first_service) = first_service else {
        first.close().await;
        return;
    };
    let first_uri = first_service.join_uri().await;
    let saved = first_service.token_state().await;
    first.close().await;
    assert!(first_uri.is_ok());
    assert!(saved.is_ok());
    let (Ok(first_uri), Ok(saved)) = (first_uri, saved) else {
        return;
    };

    options.secret_key = Some(secret_key.clone());
    let second = SculkNode::bind(options.clone()).await;
    assert!(second.is_ok());
    let Ok(second) = second else {
        return;
    };
    let never = second
        .start_service(HostedServiceOptions {
            service_id,
            target_addr,
            token_state: Some(saved.clone()),
            token_refresh: TokenRefreshPolicy::Never,
            config: HostConfig::default(),
        })
        .await;
    assert!(never.is_ok());
    let Ok(never) = never else {
        second.close().await;
        return;
    };
    assert_eq!(never.join_uri().await.ok(), Some(first_uri.clone()));
    second.close().await;

    options.secret_key = Some(secret_key);
    let third = SculkNode::bind(options).await;
    assert!(third.is_ok());
    let Ok(third) = third else {
        return;
    };
    let always = third
        .start_service(HostedServiceOptions {
            service_id,
            target_addr,
            token_state: Some(saved),
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await;
    assert!(always.is_ok());
    let Ok(always) = always else {
        third.close().await;
        return;
    };
    assert_ne!(always.join_uri().await.ok(), Some(first_uri));
    third.close().await;
}

#[tokio::test]
async fn manual_rotation_resets_automatic_deadline() {
    let node = bind_test_node().await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let period = Duration::from_millis(300);
    let service = node
        .start_service(HostedServiceOptions {
            service_id: ServiceId::generate(),
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::After(period),
            config: HostConfig::default(),
        })
        .await;
    assert!(service.is_ok());
    let Ok(service) = service else {
        node.close().await;
        return;
    };
    let initial = service.status().await;
    assert!(initial.is_ok());
    let initial_created_at = initial.as_ref().ok().map(|status| status.token_created_at);
    assert!(
        initial
            .as_ref()
            .ok()
            .and_then(|status| status.next_rotation_at)
            .is_some()
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    let manual = service.rotate_token().await;
    assert!(manual.is_ok());
    let after_manual = service.status().await;
    assert!(after_manual.is_ok());
    let Some(after_manual) = after_manual.ok() else {
        node.close().await;
        return;
    };
    assert_eq!(after_manual.uri_generation, 2);
    assert!(Some(after_manual.token_created_at) > initial_created_at);

    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        service
            .status()
            .await
            .ok()
            .map(|status| status.uri_generation),
        Some(2),
        "the old automatic deadline was not cancelled"
    );

    let rotated = async {
        for _ in 0..40 {
            let status = service.status().await.ok()?;
            if status.uri_generation == 3 {
                return Some(status);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        None
    };
    let rotated = tokio::time::timeout(Duration::from_secs(1), rotated).await;
    assert!(matches!(rotated, Ok(Some(_))));
    node.close().await;
}

#[tokio::test]
async fn rotation_conflict_preserves_current_uri() {
    let node = bind_test_node().await;
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
    let current_uri = service.join_uri().await;
    assert!(current_uri.is_ok());
    let Some(hosted) = node.inner.services.read().await.get(&service_id).cloned() else {
        node.close().await;
        return;
    };
    let rotation = hosted.token_rotation.lock().await;

    assert!(matches!(
        service.rotate_token().await,
        Err(SculkNodeError::RotationInProgress)
    ));
    drop(rotation);

    let after_conflict = service.join_uri().await;
    assert!(after_conflict.is_ok());
    let (Ok(current_uri), Ok(after_conflict)) = (current_uri, after_conflict) else {
        node.close().await;
        return;
    };
    assert!(current_uri.token().matches(after_conflict.token()));
    assert_eq!(
        service
            .status()
            .await
            .ok()
            .map(|status| status.uri_generation),
        Some(1)
    );
    node.close().await;
}

#[tokio::test]
async fn stopped_rotation_task_cannot_modify_republished_service() {
    let node = bind_test_node().await;
    assert!(node.is_ok());
    let Ok(node) = node else {
        return;
    };
    let service_id = ServiceId::generate();
    let old = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25565)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::After(Duration::from_millis(100)),
            config: HostConfig::default(),
        })
        .await;
    assert!(old.is_ok());
    let stopped = node.stop_service(service_id).await;
    assert!(stopped.is_ok());

    let current = node
        .start_service(HostedServiceOptions {
            service_id,
            target_addr: SocketAddr::from(([127, 0, 0, 1], 25566)),
            token_state: None,
            token_refresh: TokenRefreshPolicy::Always,
            config: HostConfig::default(),
        })
        .await;
    assert!(current.is_ok());
    let Ok(current) = current else {
        node.close().await;
        return;
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        current
            .status()
            .await
            .ok()
            .map(|status| status.uri_generation),
        Some(1)
    );
    node.close().await;
}

#[tokio::test]
async fn rotation_failure_updates_status_and_warning_event() {
    let node = bind_test_node().await;
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
    let events = service.subscribe().await;
    assert!(events.is_ok());
    let Ok(mut events) = events else {
        node.close().await;
        return;
    };
    let session_generation = node
        .inner
        .services
        .read()
        .await
        .get(&service_id)
        .map(|service| service.session_generation);
    assert!(session_generation.is_some());
    let Some(session_generation) = session_generation else {
        node.close().await;
        return;
    };

    node.record_rotation_failure(
        service_id,
        session_generation,
        ErrorCategory::Internal,
        ROTATION_RETRY_MIN,
    )
    .await;
    assert_eq!(
        service
            .status()
            .await
            .ok()
            .and_then(|status| status.last_error),
        Some(ErrorCategory::Internal)
    );
    let warning = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
    assert!(matches!(
        warning,
        Ok(Ok(TunnelEvent::TokenRotationFailed {
            retry_in: ROTATION_RETRY_MIN
        }))
    ));
    node.close().await;
}

#[tokio::test]
async fn rejects_old_uri_after_token_rotation() {
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
            config: HostConfig::default(),
        })
        .await;
    assert!(service.is_ok());
    let Ok(service) = service else {
        node.close().await;
        return;
    };
    let old_uri = service.join_uri().await;
    assert!(old_uri.is_ok());
    let Ok(old_uri) = old_uri else {
        node.close().await;
        return;
    };
    let rotated_uri = service.rotate_token().await;
    assert!(rotated_uri.is_ok());
    let Ok(rotated_uri) = rotated_uri else {
        node.close().await;
        return;
    };

    let old_join = join_node(&node, &old_uri).await;
    assert!(old_join.is_err());
    let new_join = join_node(&node, &rotated_uri).await;
    assert!(new_join.is_ok());
    if let Ok((join, _)) = new_join {
        join.close().await;
    }
    node.close().await;
}
