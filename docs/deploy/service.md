# 自建 Relay 服务器部署

sculk 默认使用 n0 提供的公共 relay，如需自建，参考以下步骤。

## 获取 iroh-relay 二进制

### 方式一：下载预编译产物（推荐）

从 [Releases](https://github.com/KercyDing/sculk/releases/tag/relay-v0.96.1) 下载对应架构的二进制：

- `iroh-relay-linux-amd64`
- `iroh-relay-linux-arm64`

### 方式二：本地交叉编译

```bash
just relay-build        # 编译全部架构（amd64 + arm64）
just relay-build amd64  # 仅 linux/amd64
just relay-build arm64  # 仅 linux/arm64
```

产物输出至 `target/relay/bin/`。编译依赖 musl-cross 工具链：

- macOS：`brew install filosottile/musl-cross/musl-cross`
- Linux：`apt install musl-tools` 或对应发行版包管理器
- Windows：需安装 WSL，脚本会自动通过 WSL 调用 `build.sh`

## systemd 服务

将二进制复制到服务器后，创建 `/etc/systemd/system/iroh-relay.service`：

```ini
[Unit]
Description=Iroh Relay Server (dev mode, plain HTTP)
After=network.target

[Service]
ExecStart=/usr/local/bin/iroh-relay --dev
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

启用并启动：

```bash
systemctl enable --now iroh-relay
```

## 在 sculk 中配置

iroh-relay 启动后会在日志中打印监听地址，将该地址填入 sculk：

```bash
# CLI
sculk relay --url <iroh-relay 输出的 URL>

# TUI
# 进入「中继」标签页，切换到「自建中继」并填入 URL
```

若绑定了域名并配置了 TLS 反向代理，填入 `https://` 地址即可；`--dev` 裸跑则填 `http://` 地址。
