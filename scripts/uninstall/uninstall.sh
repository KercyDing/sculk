#!/bin/sh
set -e

# sculk uninstaller script

echo "Uninstalling sculk..."

# Find and remove binary
FOUND=0
for dir in "/usr/local/bin" "$HOME/.local/bin"; do
    if [ -f "$dir/sculk" ]; then
        if [ -w "$dir" ]; then
            rm -f "$dir/sculk"
        else
            sudo rm -f "$dir/sculk"
        fi
        echo "Removed: $dir/sculk"
        FOUND=1
    fi
done

if [ $FOUND -eq 0 ]; then
    echo "Warning: sculk binary not found in common locations"
fi

echo ""
echo "sculk uninstalled successfully!"
echo ""
