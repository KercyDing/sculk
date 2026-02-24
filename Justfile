set windows-powershell := true

default:
    @just --list

# 安装到 ~/.cargo/bin
install:
    cargo install --path cli

# 卸载
uninstall:
    cargo uninstall sculk

# 检查
check:
    cargo fmt --all -- --check
    cargo check --workspace
    cargo clippy --workspace -- -D warnings

# 测试
test:
    cargo nextest run --workspace --no-tests=pass

# 格式化
fmt:
    cargo fmt --all

# 生成文档
doc:
    cargo doc --workspace --no-deps
