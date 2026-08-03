# sculk

<kbd>[English](README-en.md)</kbd>

[![crates.io](https://img.shields.io/crates/v/sculk.svg)](https://crates.io/crates/sculk)
[![docs.rs](https://docs.rs/sculk/badge.svg)](https://docs.rs/sculk)

一个面向 Minecraft 联机的 P2P 隧道项目，基于 iroh/QUIC。

客户端程序在这里 → [SeaLantern-Connect](https://github.com/SeaLantern-Studio/SeaLantern-Connect)

> Sculk（幽匿）是 Minecraft 深暗之域中悄然蔓延的脉络，无声地在节点间传递信号。
>
> sculk 做的事类似，在玩家之间建立隐匿的隧道，让连接自然发生。

- `sculk`：命令行客户端（CLI）
- `sculk`：隧道核心库

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
- [下载、安装与卸载](docs/install.md)

### 简单用法

```sh
# 建房
sculk host

# 加入
sculk join "sculk://join/v1/<payload>"
```

详见文档：
- [使用说明](docs/usage.md)

## Relay 与自建部署

sculk 会优先尝试建立 P2P 直连；当双方受 NAT、网络策略或运营商环境限制而无法直连时，连接需要经过 Relay 转发。

项目默认使用 iroh 提供的公共 Relay，开箱即用，但其可用性、网络延迟和带宽不由 sculk 控制，部分地区或复杂网络环境下可能出现连接缓慢、不稳定或无法连接。sculk 本身也不提供公共 Relay 的可用性保证。

如果需要更稳定的入口、更合适的服务器地域，或者希望自行控制带宽和服务可用性，可以部署专用 Relay，并在 `sculk` CLI 或上层应用中配置其 URL。

部署方法和可直接使用的构建产物见：[iroh-relay](https://github.com/KercyDing/iroh-relay)

## 给开发者

开发需要 Rust `1.91.0` 或更高版本，并使用 [`only`](https://github.com/KercyDing/only) 运行项目任务；`cargo-nextest` 为可选依赖。

Workspace 包含两个 crate：

- `core`：隧道核心库（`sculk`）
- `cli`：命令行客户端（`sculk-cli` / `sculk`）

常用命令：

```sh
only check       # 格式、编译与 Clippy 检查
only ci          # 检查并运行开发测试
only dev build   # 开发构建
only rel build   # 发布构建
only install     # 构建并安装 sculk CLI
```

## 许可证

Copyright (C) 2026 KercyDing

项目采用 [MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE) 双重许可，使用者可任选其一。
