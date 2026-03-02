set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

[private]
default:
    @just --list

# 安装到 sckc 到 ~/.cargo/bin
install:
    cargo install --path cli

# 安装 sckt 到 ~/.cargo/bin
install-tui:
    cargo install --path tui

# 安装全部客户端
install-all: install install-tui

# 卸载 sculk
uninstall:
    cargo uninstall sculk-cli

# 卸载 sculk-tui
uninstall-tui:
    cargo uninstall sculk-tui

# 卸载全部客户端
uninstall-all: uninstall uninstall-tui

# 检查
check:
    cargo fmt --all -- --check
    cargo check --workspace
    cargo clippy --workspace --all-targets --all-features

# 测试
test:
    cargo nextest run --workspace --no-tests=pass

# 格式化
fmt:
    cargo fmt --all

# 生成文档
doc:
    cargo doc --workspace --no-deps --open

# 编译 iroh-relay relay 服务端
[unix]
[group('relay')]
relay-build target='all':
    bash scripts/deploy/build.sh {{ target }}

[windows]
[group('relay')]
relay-build target='all':
    pwsh scripts/deploy/build.ps1 -Target {{ target }}
