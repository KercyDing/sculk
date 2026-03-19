# 开发文档

## 环境

- Rust `1.91.1`
- `just`（命令管理）
- `cargo-nextest`（测试）

```sh
cargo install just just-lsp
cargo install cargo-nextest --locked
```

## 常用命令

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

## Workspace 结构

- `core`：隧道核心库（`sculk`）
- `cli`：命令行客户端（`sculk-cli` / `sckc`）
- `tui`：终端图形客户端（`sculk-tui` / `sckt`）

## 相关文档

- 安装与使用：[`docs/install.md`](./install.md)
- Relay 部署：[`docs/deploy.md`](./deploy.md)
