use super::*;

#[test]
fn validates_target_address_boundaries() {
    assert!(validate_target(SocketAddr::from(([127, 0, 0, 1], 1))).is_ok());
    assert!(validate_target(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 65535))).is_ok());

    let target = "192.168.1.2:25565".parse();
    assert!(target.is_ok());
    let result = validate_target(target.unwrap_or_else(|_| unreachable!()));
    assert!(matches!(result, Err(SculkNodeError::TargetNotLoopback)));

    let zero_port = SocketAddr::from(([127, 0, 0, 1], 0));
    assert!(matches!(
        validate_target(zero_port),
        Err(SculkNodeError::InvalidTargetPort)
    ));
}
