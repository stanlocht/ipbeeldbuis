#!/usr/bin/env bash
set -euo pipefail

REPO="stanlocht/ipbeeldbuis"
BINARY="ipb"
INSTALL_DIR="/usr/local/bin"

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$OS" in
  darwin) OS="macos" ;;
  linux)  OS="linux" ;;
  *) echo "Unsupported OS: $OS" && exit 1 ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)          ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH" && exit 1 ;;
esac

# Get latest release tag from GitHub API
LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | head -1 | cut -d'"' -f4)

if [ -z "$LATEST" ]; then
  echo "Could not determine latest release. Check https://github.com/$REPO/releases"
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$LATEST/${BINARY}-${OS}-${ARCH}.tar.gz"

echo "Installing ipb $LATEST ($OS/$ARCH)..."
curl -fsSL "$URL" | tar -xz -C /tmp

if [ ! -f "/tmp/$BINARY" ]; then
  echo "Download failed. Check https://github.com/$REPO/releases"
  exit 1
fi

chmod +x "/tmp/$BINARY"

if [ -w "$INSTALL_DIR" ]; then
  mv "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
else
  sudo mv "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
fi

echo "Installed to $INSTALL_DIR/$BINARY"
echo "Run: ipb --help"
