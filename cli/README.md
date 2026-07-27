# sculk-cli

Minecraft P2P multiplayer tunnel CLI, built on [sculk](https://crates.io/crates/sculk).

## Installation

```sh
cargo install sculk-cli
```

> The binary is named `sculk`, while the crate is named `sculk-cli`.

## Usage

### Host

```sh
sculk host
```

Options:
- `-p <PORT>` — Local MC server port (default: 25565)
- `--max-players <N>` — Maximum player count
- `--relay <URL>` — Override relay address
- `--new-key` — Force generate a new secret key
- `-t, --time <POLICY>` — Save the Share URI refresh policy (`always`,
  `never`, `1h`, `3h`, `6h`, `12h`, or `24h`)
- `--new-uri` — Generate a new Share URI for this start without changing
  the saved refresh policy

### Join

```sh
sculk join "sculk://join/v1/<payload>"
```

Options:
- `-p <PORT>` — Optional local inbound listen port (default: automatic)
- `--max-retries <N>` — Max reconnection attempts (omit for unlimited)

### Relay Configuration

```sh
sculk relay --list
sculk relay --url https://your-relay.example.com
sculk relay --reset
```

---

## 中文说明

Minecraft P2P 联机隧道命令行客户端，基于 [sculk](https://crates.io/crates/sculk)。

### 安装

```sh
cargo install sculk-cli
```

> 二进制名是 `sculk`，Cargo 包名是 `sculk-cli`。

### 使用

#### 建房

```sh
sculk host
```

常用参数：
- `-p <PORT>` — 本地 MC 服务端端口（默认 25565）
- `--max-players <N>` — 最大玩家数
- `--relay <URL>` — 覆盖 relay 地址
- `--new-key` — 强制生成新密钥
- `-t, --time <POLICY>` — 保存分享链接刷新策略（`always`、`never`、
  `1h`、`3h`、`6h`、`12h` 或 `24h`）
- `--new-uri` — 仅本次启动生成新分享链接，不修改已保存的刷新策略

#### 加入

```sh
sculk join "sculk://join/v1/<payload>"
```

常用参数：
- `-p <PORT>` — 可选本地入站监听端口（默认自动分配）
- `--max-retries <N>` — 最大重连次数（不传=无限）

#### 中继配置

```sh
sculk relay --list
sculk relay --url https://your-relay.example.com
sculk relay --reset
```

更多信息见[项目主页](https://github.com/KercyDing/sculk)。

## 许可证

Copyright (C) 2026 KercyDing

本客户端采用 [MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE) 双重许可，
使用者可任选其一。
