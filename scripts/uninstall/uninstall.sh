#!/bin/sh
set -e

removed=0
for name in sculk sckc sckt; do
    for dir in "/usr/local/bin" "$HOME/.local/bin"; do
        path="$dir/$name"
        if [ -f "$path" ]; then
            if [ -w "$dir" ]; then
                rm -f "$path"
            else
                sudo rm -f "$path"
            fi
            echo "已删除：$path"
            removed=1
        fi
    done
done

if [ -f "$HOME/.cargo/bin/sculk" ] || [ -f "$HOME/.cargo/bin/sckc" ]; then
    echo "检测到 Cargo 安装版本，请执行：cargo uninstall sculk-cli"
    removed=1
fi
if [ -f "$HOME/.cargo/bin/sckt" ]; then
    echo "检测到旧版 TUI，请执行：cargo uninstall sculk-tui"
    removed=1
fi

if [ "$removed" -eq 0 ]; then
    echo "常见路径中未找到 sculk。"
fi
