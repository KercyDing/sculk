set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

[private]
default:
    @just --list

# 安装到 ~/.cargo/bin
install:
    cargo install --path cli

# 安装 sculk-tui 到 ~/.cargo/bin
install-tui:
    cargo install --path tui

# 安装全部客户端
install-all: install install-tui

# 卸载
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
    cargo clippy --workspace -- -D warnings

# 测试（离线优先，和 CI 口径一致）
test:
    cargo nextest run --workspace --features sculk-core/ci --no-tests=pass

# 网络集成测试（需要可用网络环境）
test-e2e:
    cargo nextest run -p sculk-core --test p2p_test --no-tests=pass

# 全量测试（稳定测试 + 网络集成测试）
test-all: test test-e2e

# 格式化
fmt:
    cargo fmt --all

# 生成文档
doc:
    cargo doc --workspace --no-deps --open

# 内部: 交叉编译 iroh-relay
[unix]
_relay-build target linker env_var artifact:
    #!/usr/bin/env bash
    set -euo pipefail
    which "{{ linker }}" > /dev/null 2>&1 || { echo "缺少 musl-cross 工具链，请先安装: brew install filosottile/musl-cross/musl-cross"; exit 1; }
    rustup target list --installed | grep -q "{{ target }}" || rustup target add "{{ target }}"
    export {{ env_var }}="{{ linker }}"
    cargo install iroh-relay --features server --target "{{ target }}" --root target/relay
    mv target/relay/bin/iroh-relay "target/relay/bin/{{ artifact }}"
    echo "产物: target/relay/bin/{{ artifact }}"

# 编译 iroh-relay linux-amd64
[unix]
[group('relay')]
relay-build-x86_64: (_relay-build "x86_64-unknown-linux-musl" "x86_64-linux-musl-gcc" "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER" "iroh-relay-linux-amd64")

# 编译 iroh-relay linux-arm64
[unix]
[group('relay')]
relay-build-aarch64: (_relay-build "aarch64-unknown-linux-musl" "aarch64-linux-musl-gcc" "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER" "iroh-relay-linux-arm64")

# 编译全部架构的 iroh-relay
[unix]
[group('relay')]
relay-build-all: relay-build-x86_64 relay-build-aarch64
