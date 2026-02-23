#!/bin/sh
set -e

# sculk installer script

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux*)
        if [ "$ARCH" = "x86_64" ]; then
            BINARY="sculk-linux-amd64"
        else
            echo "Error: Unsupported architecture $ARCH for Linux"
            exit 1
        fi
        ;;
    Darwin*)
        if [ "$ARCH" = "arm64" ]; then
            BINARY="sculk-darwin-arm64"
        elif [ "$ARCH" = "x86_64" ]; then
            BINARY="sculk-darwin-amd64"
        else
            echo "Error: Unsupported architecture $ARCH for macOS"
            exit 1
        fi
        ;;
    *)
        echo "Error: Unsupported operating system $OS"
        exit 1
        ;;
esac

DOWNLOAD_URL="https://github.com/SeaLantern-Studio/sculk/releases/latest/download/$BINARY"

echo "Downloading sculk for $OS $ARCH..."
TEMP_FILE=$(mktemp)
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_FILE"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$DOWNLOAD_URL" -O "$TEMP_FILE"
else
    echo "Error: curl or wget is required"
    exit 1
fi

chmod +x "$TEMP_FILE"

# Determine install directory
if [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
elif [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

echo "Installing sculk to $INSTALL_DIR..."
mv "$TEMP_FILE" "$INSTALL_DIR/sculk"

echo "sculk installed successfully!"

# Check if install directory is in PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        ;;
    *)
        echo ""
        echo "Adding $INSTALL_DIR to PATH..."
        SHELL_NAME=$(basename "$SHELL")
        case "$SHELL_NAME" in
            bash)  SHELL_RC="$HOME/.bashrc" ;;
            zsh)   SHELL_RC="$HOME/.zshrc" ;;
            fish)  SHELL_RC="$HOME/.config/fish/config.fish" ;;
            *)     SHELL_RC="$HOME/.profile" ;;
        esac

        if [ -f "$SHELL_RC" ] && ! grep -q "$INSTALL_DIR" "$SHELL_RC"; then
            echo "" >> "$SHELL_RC"
            echo "# sculk path" >> "$SHELL_RC"
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_RC"
            echo "Added to PATH in $SHELL_RC"
        fi

        export PATH="$INSTALL_DIR:$PATH"
        ;;
esac

# Verify installation
echo ""
if command -v sculk >/dev/null 2>&1; then
    echo "Verification: $(sculk --version)"
else
    echo "Please restart your terminal or run: source $SHELL_RC"
fi
echo ""
