#!/bin/sh
set -e

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS-$ARCH" in
    Linux-x86_64) SUFFIX="linux-amd64" ;;
    Darwin-arm64) SUFFIX="darwin-arm64" ;;
    Darwin-x86_64) SUFFIX="darwin-amd64" ;;
    *)
        echo "错误：暂不支持 $OS $ARCH"
        exit 1
        ;;
esac

if [ -w "/usr/local/bin" ] || [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

cargo_path="$HOME/.cargo/bin/sculk"
if [ -f "$cargo_path" ]; then
    echo "警告：检测到 $cargo_path，建议先执行 cargo uninstall sculk-cli 避免冲突。"
    printf "是否继续安装？[y/N] "
    read answer
    case "$answer" in
        [yY]*) ;;
        *) exit 0 ;;
    esac
fi

artifact="sculk-$SUFFIX"
url="https://github.com/KercyDing/sculk/releases/latest/download/$artifact"
temp_file="$(mktemp)"
trap 'rm -f "$temp_file"' EXIT

echo "正在下载 $artifact..."
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$temp_file"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$temp_file"
else
    echo "错误：需要 curl 或 wget"
    exit 1
fi

chmod +x "$temp_file"
mv "$temp_file" "$INSTALL_DIR/sculk"
echo "sculk 已安装到 $INSTALL_DIR/sculk"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "请将 $INSTALL_DIR 添加到 PATH。"
        ;;
esac
