set windows-powershell := true

default:
    @just --list

# 开发构建
build:
    cargo build -p sculk-cli

# Release 构建
build-release:
    cargo build -p sculk-cli --release

# 房主：创建房间 (默认 25565)
host port="25565":
    cargo run -p sculk-cli -- host -p {{port}}

# 玩家：加入房间
join ticket port="30000":
    cargo run -p sculk-cli -- join {{ticket}} -p {{port}}

# 检查
check:
    cargo fmt --all -- --check
    cargo check --workspace
    cargo clippy --workspace -- -D warnings

# 测试（跳过需要网络的 P2P 测试）
test:
    cargo nextest run --workspace --no-tests=pass

# 格式化
fmt:
    cargo fmt --all

# 生成文档
doc:
    cargo doc --workspace --no-deps
