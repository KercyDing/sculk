# sculk-cli

Minecraft P2P multiplayer tunnel CLI, built on [sculk-core](https://crates.io/crates/sculk-core).

## Installation

```sh
cargo install sculk-cli
```

> The binary is named `sculk`, the crate name is `sculk-cli`.

## Usage

### Host

```sh
sculk host
```

Options:
- `-p <PORT>` — Local MC server port (default: 25565)
- `--password <PWD>` — Connection password
- `--max-players <N>` — Maximum player count
- `--relay <URL>` — Override relay address
- `--new-key` — Force generate a new secret key

### Join

```sh
sculk join "sculk://..."
```

Options:
- `-p <PORT>` — Local inbound listen port (default: 30000)
- `--password <PWD>` — Join password
- `--max-retries <N>` — Max reconnection attempts (omit for unlimited)

### Relay Configuration

```sh
sculk relay --list
sculk relay --url https://your-relay.example.com
sculk relay --reset
```

---

## 中文说明

Minecraft P2P 联机隧道命令行客户端，基于 [sculk-core](https://crates.io/crates/sculk-core)。

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
- `--password <PWD>` — 连接密码
- `--max-players <N>` — 最大玩家数
- `--relay <URL>` — 覆盖 relay 地址
- `--new-key` — 强制生成新密钥

#### 加入

```sh
sculk join "sculk://..."
```

常用参数：
- `-p <PORT>` — 本地入站监听端口（默认 30000）
- `--password <PWD>` — 加入密码
- `--max-retries <N>` — 最大重连次数（不传=无限）

#### 中继配置

```sh
sculk relay --list
sculk relay --url https://your-relay.example.com
sculk relay --reset
```

更多信息见[项目主页](https://github.com/KercyDing/sculk)。
