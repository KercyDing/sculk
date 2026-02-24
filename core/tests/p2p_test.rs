//! P2P 隧道集成测试
//!
//! 在同一进程内启动 echo server + host + join，验证数据能通过隧道双向传输。

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use sculk_core::tunnel::TunnelEvent;

/// 启动一个简单的 TCP echo server，收到什么就回什么
async fn echo_server(listener: TcpListener) {
    loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = stream.write_all(&buf[..n]).await;
                    }
                }
            }
        });
    }
}

#[tokio::test]
#[cfg_attr(feature = "ci", ignore)] // CI 无真实网络，跳过
async fn tunnel_echo_roundtrip() {
    // 1. 启动 echo server（模拟 MC 服务端）
    let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_port = echo_listener.local_addr().unwrap().port();
    tokio::spawn(echo_server(echo_listener));

    // 2. Host: 创建隧道
    let (host_tunnel, ticket, mut host_events) =
        sculk_core::tunnel::IrohTunnel::host(mc_port, None, None)
            .await
            .unwrap();

    // 3. Join: 用票据连接，监听随机端口
    let join_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_port = join_listener.local_addr().unwrap().port();
    drop(join_listener); // 释放端口给 IrohTunnel::join 使用

    let (join_tunnel, mut join_events) =
        sculk_core::tunnel::IrohTunnel::join(&ticket, local_port, None)
            .await
            .unwrap();

    // 验证 join 端收到 Connected 事件
    let event = tokio::time::timeout(Duration::from_secs(5), join_events.recv())
        .await
        .expect("timeout waiting for Connected event")
        .expect("channel closed");
    assert!(matches!(event, TunnelEvent::Connected));

    // 验证 host 端收到 PlayerJoined 事件
    let event = tokio::time::timeout(Duration::from_secs(5), host_events.recv())
        .await
        .expect("timeout waiting for PlayerJoined event")
        .expect("channel closed");
    assert!(matches!(event, TunnelEvent::PlayerJoined { .. }));

    // 等待隧道就绪
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 4. 模拟 MC 客户端连接
    let mut client = TcpStream::connect(("127.0.0.1", local_port)).await.unwrap();

    // 5. 发送数据，验证 echo 回来
    let messages = ["hello", "minecraft", "sculk tunnel works!"];
    for msg in &messages {
        client.write_all(msg.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut buf = vec![0u8; msg.len()];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&buf), *msg);
    }

    // 6. 清理
    drop(client);
    join_tunnel.close().await;
    host_tunnel.close().await;
}
