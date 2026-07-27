# sculk

P2P tunnel core library for Minecraft multiplayer, built on
[iroh](https://crates.io/crates/iroh)/QUIC. `TunnelService` owns the tunnel
lifecycle, state machine, connection snapshots, and event distribution.
Applications only send commands and consume a unified update subscription.

```rust,no_run
use sculk::tunnel::{
    HostOptions, TunnelEvent, TunnelPhase, TunnelService, TunnelUpdate,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = TunnelService::new();
    let mut updates = service.subscribe();
    let mut uri_printed = false;

    // Expose the local Minecraft server on port 25565.
    service.start_host(HostOptions::new(25565)).await?;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            result = &mut ctrl_c => {
                result?;
                service.shutdown().await?;
                break;
            }
            update = updates.recv() => {
                match update {
                    Some(TunnelUpdate::Status(status)) => {
                        if status.state.phase == TunnelPhase::Active
                            && !uri_printed
                            && let Some(uri) = status.state.join_uri
                        {
                            println!("Join URI: {}", uri.expose_secret_uri()?);
                            uri_printed = true;
                        }
                        if status.state.phase == TunnelPhase::Idle {
                            break;
                        }
                    }
                    Some(TunnelUpdate::Event(TunnelEvent::Error { message })) => {
                        eprintln!("Tunnel error: {message}");
                    }
                    Some(TunnelUpdate::Event(event)) => {
                        println!("Tunnel event: {event:?}");
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    Ok(())
}
```

To join an existing tunnel, parse the shared `JoinUri` and call
`service.start_join(JoinOptions::new(uri))`. See
[sculk](https://github.com/KercyDing/sculk) for the complete project
and its CLI integration.

## Features

| Feature | Enabled by default | Description |
|---|---|---|
| `persist` | No | Persist keys and user configuration |

---

## 中文说明

面向 Minecraft 联机的 P2P 隧道核心库，基于
[iroh](https://crates.io/crates/iroh)/QUIC。`TunnelService` 负责隧道的启动、
停止、状态机、连接快照和事件分发；上层只需发送命令并订阅统一更新频道。

```rust,no_run
use sculk::tunnel::{
    HostOptions, TunnelEvent, TunnelPhase, TunnelService, TunnelUpdate,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = TunnelService::new();
    let mut updates = service.subscribe();
    let mut uri_printed = false;

    // 将本地 25565 端口作为 Host 暴露给其他玩家。
    service.start_host(HostOptions::new(25565)).await?;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            result = &mut ctrl_c => {
                result?;
                service.shutdown().await?;
                break;
            }
            update = updates.recv() => {
                match update {
                    Some(TunnelUpdate::Status(status)) => {
                        if status.state.phase == TunnelPhase::Active
                            && !uri_printed
                            && let Some(uri) = status.state.join_uri
                        {
                            println!("分享 URI: {}", uri.expose_secret_uri()?);
                            uri_printed = true;
                        }
                        if status.state.phase == TunnelPhase::Idle {
                            break;
                        }
                    }
                    Some(TunnelUpdate::Event(TunnelEvent::Error { message })) => {
                        eprintln!("隧道错误: {message}");
                    }
                    Some(TunnelUpdate::Event(event)) => {
                        println!("隧道事件: {event:?}");
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    Ok(())
}
```

加入已有隧道时，解析对方分享的 `JoinUri`，然后调用
`service.start_join(JoinOptions::new(uri))`。完整项目与 CLI
示例见 [sculk](https://github.com/KercyDing/sculk)。

### Features

| Feature | 默认启用 | 说明 |
|---|---|---|
| `persist` | 否 | 密钥与用户配置持久化 |

### License / 许可证

Licensed under your choice of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE).

本核心库采用 [MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE) 双重许可，
使用者可任选其一。
