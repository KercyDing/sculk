# sculk

> 跨平台零权限 P2P 安全隧道引擎，为 [SeaLantern](https://github.com/SeaLantern-Studio/SeaLantern) 提供联机网络支持。

命名源自 Minecraft 中的"幽匿（Sculk）"—— 在底层悄无声息地感知并传递网络数据。

## 工作原理

基于 [iroh](https://github.com/n0-computer/iroh) 实现 P2P 隧道，通过 QUIC + NAT 打洞建立直连，无需公网 IP、端口映射或管理员权限。

```mermaid
graph LR
    MC[MC 客户端] -->|连接| Inlet[本地监听<br/>localhost:30000]
    Inlet ===|iroh QUIC<br/>NAT 打洞直连 / Relay 回退| Outlet[本地转发<br/>localhost:25565]
    Outlet -->|转发| Server[MC 服务端]
```

## 安装

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/SeaLantern-Studio/sculk/main/scripts/install/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/SeaLantern-Studio/sculk/main/scripts/install/install.ps1 | iex
```

## 使用

```sh
# 房主：创建房间，获得连接票据
sculk host -p 25565
# 输出: 票据 xxxxx，分享给玩家

# 玩家：用票据加入
sculk join xxxxx -p 30000
```

## 构建

```sh
cargo build
cargo test
```

## 卸载

**macOS / Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/SeaLantern-Studio/sculk/main/scripts/uninstall/uninstall.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/SeaLantern-Studio/sculk/main/scripts/uninstall/uninstall.ps1 | iex
```

## License

[GPL-3.0](LICENSE)
