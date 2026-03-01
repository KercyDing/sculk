#!/usr/bin/env bash
# 交叉编译 iroh-relay 为 Linux musl 静态二进制。
# 用法: build.sh [amd64|arm64|all]   默认 all
set -euo pipefail

TARGET="${1:-all}"
OUT="target/relay/bin"
mkdir -p "$OUT"

build() {
    local rust_target="$1" linker="$2" linker_env="$3" artifact="$4"

    if ! command -v "$linker" &>/dev/null; then
        echo "错误: 未找到 $linker"
        echo "  macOS: brew install filosottile/musl-cross/musl-cross"
        echo "  Linux: apt install musl-tools  或对应发行版包管理器"
        exit 1
    fi

    rustup target add "$rust_target" 2>/dev/null || true
    export "$linker_env"="$linker"
    cargo install iroh-relay --features server --target "$rust_target" --root target/relay
    mv "target/relay/bin/iroh-relay" "$OUT/$artifact"
    echo "产物: $OUT/$artifact"
}

case "$TARGET" in
    amd64|x86_64)
        build x86_64-unknown-linux-musl  x86_64-linux-musl-gcc  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER  iroh-relay-linux-amd64
        ;;
    arm64|aarch64)
        build aarch64-unknown-linux-musl aarch64-linux-musl-gcc CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER iroh-relay-linux-arm64
        ;;
    all)
        build x86_64-unknown-linux-musl  x86_64-linux-musl-gcc  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER  iroh-relay-linux-amd64
        build aarch64-unknown-linux-musl aarch64-linux-musl-gcc CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER iroh-relay-linux-arm64
        ;;
    *)
        echo "用法: $0 [amd64|arm64|all]"
        exit 1
        ;;
esac
