# Docker NAT / QAD 测试

在单机上用 Docker 模拟两个局域网、两个 NAT Router 和一个公网 Relay，验证 sculk 在 Relay-only 和 UDP 打洞两种网络下的 TCP 转发。

## 网络拓扑

```text
LAN A 172.30.1.0/24 ── nat-a ──┐
                                 ├── WAN 172.30.0.0/24 ── relay (TLS + QAD)
LAN B 172.30.2.0/24 ── nat-b ──┘
```

组件：

- `host`：LAN A，本地跑 TCP echo 服务，并通过 `sculk host` 发布；
- `join`：LAN B，读取共享目录里的 `sculk://` URI，执行 `sculk join`，再发 TCP 数据；
- `nat-a`、`nat-b`：Linux Router，做 IPv4 转发、MASQUERADE 和防火墙；
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
5. 依次运行 `relay-only` 和 `holepunch`。

`run.sh` 默认从 `Cargo.lock` 读取 `iroh-relay` 的版本；如果需要指定其他版本，可以用 `RELAY_VERSION` 覆盖。

Docker builder 使用了 BuildKit cache mount，第一次构建较慢，之后只重编改动的 crate。

## 用例

| 用例 | NAT 规则 | 预期路径 |
|---|---|---|
| `relay-only` | 丢弃两端 NAT 公网 IP 之间的 UDP | `relay` |
| `holepunch` | 允许直连 UDP | `direct` |

两个用例都会验证：

- `hello` 能经 tunnel 转发到 Host 的 TCP echo 服务并原路返回；
- CLI 的 `PathChanged` 最终路径符合预期。

`relay-only` 靠 Router 里的这条规则阻断直连：

```sh
iptables -A FORWARD -p udp -d "$peer_wan" -j DROP
```

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

- 查看 `docker/nat/run/join.log` 里的 `PathChanged` 事件；
- 确认两端能访问 Relay 的 UDP `7842`；
- 确认两个 NAT Router 允许打洞的返回流量；
- 检查 `nat-a` / `nat-b` 的 `iptables` 和 `ip_forward` 是否生效。

如果 `run.sh` 失败，会输出 `host.log`、`join.log` 以及 Relay / NAT 的最近日志。

## 生成文件

以下目录都是测试产物，已加入 `.gitignore`：

```text
docker/nat/bin/     # iroh-relay 二进制
docker/nat/certs/   # 自签证书
docker/nat/run/     # URI、日志、测试结果
```

## 限制

- 只验证 Linux NAT 行为，不能模拟校园网或运营商 CGNAT；
- 路径断言来自 CLI 的 `PathChanged` 日志，不是直接读取 `ConnectionSnapshot::is_relay`；
- 目前没有覆盖丢包、延迟、Relay 故障和断线重连；
- Docker Router 规则是按当前拓扑写的，新增网络时需要同步调整。
