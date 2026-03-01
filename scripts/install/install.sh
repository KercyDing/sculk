#!/bin/sh
set -e

# sculk 安装脚本

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux*)
        if [ "$ARCH" = "x86_64" ]; then
            SUFFIX="linux-amd64"
        else
            echo "错误：Linux 暂不支持架构 $ARCH"
            exit 1
        fi
        ;;
    Darwin*)
        if [ "$ARCH" = "arm64" ]; then
            SUFFIX="darwin-arm64"
        elif [ "$ARCH" = "x86_64" ]; then
            SUFFIX="darwin-amd64"
        else
            echo "错误：macOS 暂不支持架构 $ARCH"
            exit 1
        fi
        ;;
    *)
        echo "错误：暂不支持操作系统 $OS"
        exit 1
        ;;
esac

if [ -w "/usr/local/bin" ] || [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

if [ -t 0 ]; then
    echo "请选择要安装的组件："
    echo "  1) sculk"
    echo "  2) sculk-tui"
    echo "  3) 全部 (默认)"
    printf "输入选项 [1/2/3]: "
    read choice
else
    choice="3"
    echo "未检测到交互式终端，默认安装全部组件。"
fi

install_sculk=0
install_tui=0
case "$choice" in
    1)
        install_sculk=1
        ;;
    2)
        install_tui=1
        ;;
    ""|3)
        install_sculk=1
        install_tui=1
        ;;
    *)
        echo "错误：无效选项 '$choice'"
        exit 1
        ;;
esac

if [ "$install_sculk" -eq 1 ]; then
    install_list="sculk"
fi
if [ "$install_tui" -eq 1 ]; then
    if [ -n "$install_list" ]; then
        install_list="$install_list sculk-tui"
    else
        install_list="sculk-tui"
    fi
fi

download_binary() {
    artifact="$1"
    target_name="$2"
    url="https://github.com/KercyDing/sculk/releases/latest/download/$artifact"

    echo "正在下载 $artifact..."
    temp_file="$(mktemp)"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$temp_file"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$temp_file"
    else
        echo "错误：需要 curl 或 wget"
        exit 1
    fi

    chmod +x "$temp_file"
    echo "正在安装 $target_name 到 $INSTALL_DIR..."
    mv "$temp_file" "$INSTALL_DIR/$target_name"
}

echo "正在安装组件：$install_list"

# 检测 cargo install 冲突
check_cargo_conflict() {
    cargo_path="$HOME/.cargo/bin/$1"
    if [ -f "$cargo_path" ]; then
        pkg_name=$([ "$1" = "sculk" ] && echo "sculk-cli" || echo "sculk-tui")
        echo "警告：检测到 $cargo_path，建议先执行 cargo uninstall $pkg_name 避免冲突。"
        printf "是否继续安装？[y/N] "
        read answer
        case "$answer" in
            [yY]*)
                printf "是否删除 $cargo_path？[y/N] "
                read del_answer
                case "$del_answer" in
                    [yY]*) rm -f "$cargo_path"; echo "已删除 $cargo_path" ;;
                esac
                ;;
            *) echo "已跳过 $1"; return 1 ;;
        esac
    fi
    return 0
}

if [ "$install_sculk" -eq 1 ]; then
    if check_cargo_conflict "sculk"; then
        download_binary "sculk-$SUFFIX" "sculk"
    fi
fi
if [ "$install_tui" -eq 1 ]; then
    if check_cargo_conflict "sculk-tui"; then
        download_binary "sculk-tui-$SUFFIX" "sculk-tui"
    fi
fi

# 检查安装目录是否在 PATH 中
SHELL_RC="$HOME/.profile"
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        ;;
    *)
        echo ""
        echo "正在将 $INSTALL_DIR 添加到 PATH..."
        SHELL_NAME=$(basename "$SHELL")
        case "$SHELL_NAME" in
            bash)  SHELL_RC="$HOME/.bashrc" ;;
            zsh)   SHELL_RC="$HOME/.zshrc" ;;
            fish)  SHELL_RC="$HOME/.config/fish/config.fish" ;;
            *)     SHELL_RC="$HOME/.profile" ;;
        esac

        if [ -f "$SHELL_RC" ] && ! grep -q "$INSTALL_DIR" "$SHELL_RC"; then
            echo "" >> "$SHELL_RC"
            echo "# sculk 路径" >> "$SHELL_RC"
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_RC"
            echo "已在 $SHELL_RC 写入 PATH"
        fi

        export PATH="$INSTALL_DIR:$PATH"
        ;;
esac

echo ""
if [ "$install_sculk" -eq 1 ]; then
    if command -v sculk >/dev/null 2>&1; then
        echo "验证 (sculk): $(sculk --version)"
    else
        echo "sculk 已安装，但当前 PATH 中未找到命令。"
    fi
fi
if [ "$install_tui" -eq 1 ]; then
    if command -v sculk-tui >/dev/null 2>&1; then
        echo "验证 (sculk-tui): $(sculk-tui --version)"
    else
        echo "sculk-tui 已安装，但当前 PATH 中未找到命令。"
    fi
fi

echo ""
echo "安装完成。"
if [ -n "$SHELL_RC" ]; then
    echo "如果命令暂不可用，请重启终端或执行：source $SHELL_RC"
fi
echo ""
