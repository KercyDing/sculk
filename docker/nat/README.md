# Docker NAT / QAD 测试

在单机上用 Docker 模拟两个局域网、两个 NAT Router 和一个公网 Relay，验证 sculk 在 Relay-only、UDP 打洞、丢包、Relay 故障和断网恢复下的 TCP 转发。

## 网络拓扑

```text
LAN A 172.30.1.0/24 ── nat-a ──┐
                                 ├── WAN 172.30.0.0/24 ── relay (TLS + QAD)
LAN B 172.30.2.0/24 ── nat-b ──┘
```

组件：

- `host`：LAN A，本地跑 TCP echo 服务，并通过 `sculk host` 发布；
- `join`：LAN B，读取共享目录里的 `sculk://` URI，执行 `sculk join`，再发 TCP 数据；
- `nat-a`、`nat-b`：Linux Router，做 IPv4 转发、MASQUERADE、严格 FORWARD 规则和可选 `tc netem`；
- `relay`：`iroh-relay`，启用 TLS 和 QUIC Address Discovery（QAD）。

## 运行

```sh
./docker/nat/run.sh
```

脚本会：

1. 在 Docker builder 里编译 `sculk-cli`，启用测试专用 feature `insecure-relay-tls`；
2. 如果本地没有 `iroh-relay`，从 `KercyDing/iroh-relay` release 下载；
3. 为 `172.30.0.10` 生成自签证书；
4. 构建测试镜像；
5. 依次运行下面的用例。

`run.sh` 默认从 `Cargo.lock` 读取 `iroh-relay` 的版本；如果需要指定其他版本，可以用 `RELAY_VERSION` 覆盖。

Docker builder 使用了 BuildKit cache mount，第一次构建较慢，之后只重编改动的 crate。

## 用例

| 用例 | 网络条件 | 预期 |
|---|---|---|
| `relay-only` | 阻断两端 NAT 公网 IP 之间的 UDP | 路径 `relay`，TCP 正常 |
| `holepunch` | 允许直连 UDP | 路径 `direct`，TCP 正常 |
| `holepunch-no-lookup` | `ADDRESS_LOOKUP=0`，允许直连 UDP | 仍然 `direct`，证明不依赖 n0 lookup |
| `relay-lossy` | Relay-only + `tc netem` 20ms 延迟 / 2% 丢包 | 路径 `relay`，TCP 仍能完成 |
| `relay-restart` | Relay-only，Relay 停止 35s 后恢复 | 出现 `Reconnecting` / `Reconnected`，TCP 恢复 |
| `network-drop` | 直连正常，nat-a 临时 DROP 全部转发流量 35s | 出现重连，TCP 恢复 |

每个用例都会验证：

- `hello` 能经 tunnel 转发到 Host 的 TCP echo 服务并原路返回；
- 当前路径符合预期，或故障恢复后能重新转发。

`relay-only` 和 `relay-lossy` 靠 Router 里的这条规则阻断直连：

```sh
iptables -A FORWARD -p udp -d "$peer_wan" -j DROP
```

`relay-lossy` 通过以下环境变量注入网络质量：

```text
NETEM_DELAY=20ms
NETEM_LOSS=2%
```

脚本会在 Router 的 WAN / LAN 出口执行 `tc qdisc replace ... netem`。

`relay-restart` / `network-drop` 的故障持续时间是 35 秒，用来超过 QUIC idle timeout，确保触发应用层的重连，而不是被底层透明恢复。

## 路径断言与状态文件

`sculk join` 现在支持：

```sh
--status-file /shared/status
```

`join.sh` 读取这个文件里的：

```text
is_relay=true
```

或：

```text
is_relay=false
```

来断言 `relay` / `direct`，不再依赖解析 `PathChanged` 日志。

## 地址查询开关

Host / Join 都支持：

```sh
--no-address-lookup
```

对应配置：

```rust
HostConfig::new().address_lookup(false)
JoinConfig::new().address_lookup(false)
NodeOptions { address_lookup: false, ..Default::default() }
```

`holepunch-no-lookup` 用这个开关验证：关闭 n0 Pkarr/DNS lookup 后，靠 Relay + QAD + QNT 仍然可以直连。

## QAD 与 TLS

`relay.toml` 中开启了：

```toml
enable_quic_addr_discovery = true
quic_bind_addr = "0.0.0.0:7842"
```

Relay 使用 `/certs` 下的自签证书提供 HTTPS，客户端连接：

```text
https://172.30.0.10:3340/
```

因此 `sculk` 构建时启用了：

```toml
insecure-relay-tls = ["iroh/test-utils"]
```

它只用于本地测试，**生产环境不能使用**。

注意：不能用 `iroh-relay --dev`。`--dev` 是明文 HTTP，不启用 QAD，只能验证 Relay 转发，不能验证打洞。

## 排查

如果 `holepunch` 最终仍停留在 `relay`：

- 查看 `docker/nat/run/status` 和 `join.log`；
- 确认两端能访问 Relay 的 UDP `7842`；
- 确认两个 NAT Router 允许打洞的返回流量；
- 检查 `nat-a` / `nat-b` 的 `iptables` 和 `ip_forward` 是否生效。

如果 `run.sh` 失败，会输出 `host.log`、`join.log`、`status` 以及 Relay / NAT 的最近日志。

## 生成文件

以下目录都是测试产物，已加入 `.gitignore`：

```text
docker/nat/bin/     # iroh-relay 二进制
docker/nat/certs/   # 自签证书
docker/nat/run/     # URI、状态、日志、测试结果
```

## 限制

- 只验证 Linux NAT 行为，不能模拟校园网或运营商 CGNAT；
- 主路径通过本地 Relay 和 Docker 网络，不覆盖真实公网 Relay 的 TLS/ACL；
- `network-drop` 只断开一台 Router，不是真实的多路径网络抖动；
- Docker Router 规则是按当前拓扑写的，新增网络时需要同步调整。
