# sculk

[![crates.io](https://img.shields.io/crates/v/sculk.svg)](https://crates.io/crates/sculk)
[![docs.rs](https://docs.rs/sculk/badge.svg)](https://docs.rs/sculk)
[![license](https://img.shields.io/crates/l/sculk.svg)](LICENSE)

一个面向 Minecraft 联机的 P2P 隧道项目，基于 iroh/QUIC。

> Sculk（幽匿）是 Minecraft 深暗之域中悄然蔓延的脉络，无声地在节点间传递信号。
>
> sculk 做的事类似，在玩家之间建立隐匿的隧道，让连接自然发生。

- `sckc`：命令行客户端（CLI）
- `sckt`：终端图形客户端（TUI）
- `sculk`：隧道核心库

> demo 程序详见 [sculk-demo](https://github.com/KercyDing/sculk-demo)

## 快速开始

### 安装（推荐脚本）

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.ps1)))
```

详见文档：
- [下载与安装](docs/install.md)

### 简单用法

```sh
# 建房
sckc host

# 加入
sckc join "sculk://..."

# 启动 TUI
sckt
```

详见文档：
- [使用说明](docs/usage.md)

## 文档

- [开发文档](docs/develop.md)
- [自建 Relay 指导文档](docs/deploy.md)

## 许可证

[GPL-3.0](LICENSE)
