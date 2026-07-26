# 开发文档

## 环境

- Rust `1.91.0` 或更高版本
- [`only`](https://github.com/KercyDing/only)（任务运行）
- `cargo-nextest`（可选）

请先前往 [`only` 项目](https://github.com/KercyDing/only)，按照项目说明下载并安装。
如需使用 `cargo-nextest`，可运行：

```sh
cargo install cargo-nextest --locked
```

## 常用命令

```sh
only check       # fmt 检查 + cargo check + clippy
only ci          # 检查并运行开发测试
only dev build   # 开发构建
only dev run     # 运行开发构建
only dev test    # 开发测试
only rel build   # 发布构建
only rel run     # 运行发布构建
only rel test    # 发布测试
only install     # 构建并安装 sckc 和 sckt
cargo fmt --all
cargo doc --workspace --no-deps --open
```

`only install` 在 macOS 和 Linux 上安装到 `~/.local/bin`，在 Windows 上安装到
`%LOCALAPPDATA%\Programs\Sculk`。

## Workspace 结构

- `core`：隧道核心库（`sculk`）
- `cli`：命令行客户端（`sculk-cli` / `sckc`）
- `tui`：终端图形客户端（`sculk-tui` / `sckt`）

## 相关文档

- 安装与使用：[`docs/install.md`](./install.md)
- Relay 部署：[iroh-relay](https://github.com/KercyDing/iroh-relay)
