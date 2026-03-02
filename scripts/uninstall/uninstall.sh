#!/bin/sh
set -e

# sculk 卸载脚本

if [ -t 0 ]; then
    echo "请选择要卸载的组件："
    echo "  1) sckc(sculk-cli)"
    echo "  2) sckt(sculk-tui)"
    echo "  3) 全部 (默认)"
    printf "输入选项 [1/2/3]: "
    read choice
else
    choice="3"
    echo "未检测到交互式终端，默认卸载全部组件。"
fi

uninstall_sculk=0
uninstall_tui=0
case "$choice" in
    1)
        uninstall_sculk=1
        ;;
    2)
        uninstall_tui=1
        ;;
    ""|3)
        uninstall_sculk=1
        uninstall_tui=1
        ;;
    *)
        echo "错误：无效选项 '$choice'"
        exit 1
        ;;
esac

remove_binary() {
    name="$1"
    found=0
    for dir in "/usr/local/bin" "$HOME/.local/bin"; do
        path="$dir/$name"
        if [ -f "$path" ]; then
            if [ -w "$dir" ]; then
                rm -f "$path"
            else
                sudo rm -f "$path"
            fi
            echo "已删除：$path"
            found=1
        fi
    done
    cargo_path="$HOME/.cargo/bin/$name"
    if [ -f "$cargo_path" ]; then
        echo "检测到 $cargo_path，由 cargo install 安装，请手动执行：cargo uninstall $([ "$name" = "sckc" ] && echo sculk-cli || echo sculk-tui)"
        found=1
    fi
    if [ "$found" -eq 0 ]; then
        echo "警告：常见路径中未找到 $name"
    fi
}

echo "正在卸载所选组件..."
if [ "$uninstall_sculk" -eq 1 ]; then
    remove_binary "sckc"
fi
if [ "$uninstall_tui" -eq 1 ]; then
    remove_binary "sckt"
fi

echo ""
echo "卸载完成。"
echo ""
