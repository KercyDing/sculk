# sculk

[![crates.io](https://img.shields.io/crates/v/sculk.svg)](https://crates.io/crates/sculk)
[![docs.rs](https://docs.rs/sculk/badge.svg)](https://docs.rs/sculk)
[![license](https://img.shields.io/crates/l/sculk.svg)](LICENSE)

一个面向 Minecraft 联机的 P2P 隧道项目，基于 iroh/QUIC，提供：
- `sckc`：命令行客户端（CLI）
- `sckt`：终端图形客户端（TUI）
- `sculk`：可复用隧道核心库

> Sculk（幽匿）是 Minecraft 深暗之域中悄然蔓延的脉络，无声地在节点间传递信号。
> 
> sculk 做的事类似——在玩家之间建立隐匿的隧道，让连接自然发生。

## 项目结构

这是一个 Rust workspace：

- `core` (`sculk`)：隧道能力与票据协议
  - `IrohTunnel::host/join`
  - `Ticket`（`sculk://...`）
  - `TunnelConfig` / `TunnelEvent`
- `cli` (`sculk-cli`)：`sckc` 命令行入口
  - 建房、加入、中继配置管理
- `tui` (`sculk-tui`)：`sckt`（`ratatui + crossterm`）终端界面
  - 建房/加入/中继三面板 + 日志 + 状态栏

## 工作原理

`host` 端把本地 MC 服务端（默认 `25565`）暴露为可分享票据；
`join` 端把远端隧道映射到本地端口（默认 `30000`），MC 客户端连本地即可。
链路优先直连（NAT 打洞），失败回退 relay。

连接流程：

1. 房主启动 `sckc host`，读取/生成密钥，生成 `sculk://...` 票据并分享
2. 玩家通过 `sckc join "sculk://..."` 连接，经密码校验和人数校验后建立 QUIC 隧道
3. 隧道在两端之间双向转发 TCP 流量：玩家 MC 客户端 → 本地端口 → QUIC → 房主 MC 服务端
4. 运行时通过 `TunnelEvent` 推送状态变化（玩家加入/离开、路径切换、重连等）

## 安装

### 方式一：一键脚本（推荐）

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.ps1)))
```

脚本会交互式询问安装：
1. `sckc`
2. `sckt`
3. 全部

### 方式二：从 crates.io 安装

```sh
cargo install sculk-cli
cargo install sculk-tui
```

### 方式三：从源码安装

```sh
cargo install --path cli
cargo install --path tui

# 或使用 just
just install-all
```

## 卸载

### 一键脚本

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/uninstall/uninstall.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/uninstall/uninstall.ps1)))
```

### Cargo 卸载（注意包名）

```sh
cargo uninstall sculk-cli
cargo uninstall sculk-tui
```

> 二进制名是 `sckc`（CLI）/ `sckt`（TUI），Cargo 包名仍是 `sculk-cli` / `sculk-tui`。

## CLI 使用

### 建房

```sh
sckc host
```

常用参数：
- `-p <PORT>`：本地 MC 服务端端口（默认 25565）
- `--new-key`：强制生成新密钥（ticket 会变）
- `--key-path <PATH>`：自定义密钥路径
- `--relay <URL>`：覆盖 relay（优先级高于配置文件）
- `--password <PWD>`：连接密码
- `--max-players <N>`：最大玩家数

### 加入

```sh
sckc join "sculk://..."
```

常用参数：
- `-p <PORT>`：本地入站监听端口（默认 30000）
- `--password <PWD>`：加入密码
- `--max-retries <N>`：最大重连次数（不传=无限）

### 中继配置

```sh
sckc relay --list
sckc relay --url https://your-relay.example.com
sckc relay --reset
```

- relay 优先级：命令行 `--relay` > 配置文件 > 默认 n0 relay。
- 确定后写入票据，join 端直接使用票据中的 relay。

## TUI 使用

```sh
sckt
```

默认三模式：`建房 / 加入 / 中继`

主要按键：
- `←/→`：切换模式（边界钳制，不循环）
- `Tab`：切换焦点（左侧配置/右侧日志）
- `↑/↓`：
  - 左侧：切字段或中继项
  - 右侧：日志上下滚动（边界钳制）
- `Enter`：执行主动作
  - 建房/加入：启动或停止隧道
  - 中继：应用选中中继
- `e`：进入编辑模式
- `q`：退出编辑模式（中继 URL 会应用）
- `h`：开关帮助
- `Esc`：1 秒内连按两次退出
- `c`：清空日志

## 配置与数据目录

默认位于系统 `data_dir()/sculk`：
- macOS：`~/Library/Application Support/sculk/`
- Linux：`~/.local/share/sculk/`
- Windows：`%APPDATA%\sculk\`

文件列表：
- `secret.key`：32 字节 iroh 私钥，持久化后 ticket 可跨重启保持稳定
- `profile.toml`：用户偏好配置（端口、中继、上次票据等）

`profile.toml` 结构示例：

```toml
[host]
port = 25565

[join]
port = 30000
last_ticket = "sculk://..."

[relay]
custom = false
# url = "https://your-relay.example.com"
```

说明：
- `--new-key` 会重置身份并改变 ticket
- 未出现的字段自动取默认值，增删字段不会导致旧配置解析失败

## 开发

### 环境

- Rust `1.91.1`
- `just`（命令管理）
- `cargo-nextest`（测试）

```sh
cargo install just just-lsp
cargo install cargo-nextest --locked
```

### 常用命令

```sh
just check          # fmt + check + clippy
just test           # 离线测试
just test-e2e       # 网络集成测试
just test-all       # 全量测试
just fmt            # 格式化
just doc            # 生成文档
just relay-build    # 交叉编译 iroh-relay

just install        # 安装 sckc
just install-tui    # 安装 sckt
just install-all    # 安装全部
just uninstall      # 卸载 sculk-cli
just uninstall-tui  # 卸载 sculk-tui
just uninstall-all  # 卸载全部
```

## 发布产物

Release 会同时构建两个客户端（`sckc` + `sckt`）：

- Linux amd64：`sckc-linux-amd64` / `sckt-linux-amd64`
- macOS amd64：`sckc-darwin-amd64` / `sckt-darwin-amd64`
- macOS arm64：`sckc-darwin-arm64` / `sckt-darwin-arm64`
- Windows amd64：`sckc-windows-amd64.exe` / `sckt-windows-amd64.exe`

## 自建 Relay

默认使用 n0 公共 relay，如需自建请参考 [部署文档](docs/deploy/service.md)。

## 网络与 NAT 说明

- 理想路径：直连（延迟更低）
- 兜底路径：relay（可用性更高，延迟通常更高）

经验上：
- 家宽/IPv6 环境更容易直连
- 双方都在严格对称 NAT 时通常只能走 relay

可用 `iroh doctor report` 观察 NAT 情况（例如 `mapping_varies_by_dest_ip`）。

## 致谢

特别为 [SeaLantern](https://github.com/SeaLantern-Studio/SeaLantern) 提供联机服务。

## 许可证

[GPL-3.0](LICENSE)
