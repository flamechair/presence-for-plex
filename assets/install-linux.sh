#!/usr/bin/env bash
# Install Presence for Plex desktop integration (icon + .desktop file)
# Run this after installing the binary (e.g., via cargo-dist shell installer).
# Usage: ./install-linux.sh [--uninstall]

set -euo pipefail

ICON_SIZES=(16 24 32 48 64 128 256)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_NAME="presence-for-plex"
DESKTOP_FILE="$SCRIPT_DIR/presence-for-plex.desktop"
ICON_SOURCE="$SCRIPT_DIR/icon.png"
ICON_256="$SCRIPT_DIR/icon-256.png"

# Directories
APPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"

ACTION="${1:-install}"

if [ "$ACTION" = "--uninstall" ] || [ "$ACTION" = "uninstall" ]; then
    echo "Removing Presence for Plex desktop integration..."
    rm -f "$APPS_DIR/$BIN_NAME.desktop"
    for size in "${ICON_SIZES[@]}"; do
        rm -f "$ICONS_DIR/${size}x${size}/apps/$BIN_NAME.png"
    done
    update-desktop-database "$APPS_DIR" 2>/dev/null || true
    echo "Done. Desktop integration removed."
    exit 0
fi

echo "Installing Presence for Plex desktop integration..."

# Install icons at multiple sizes
for size in "${ICON_SIZES[@]}"; do
    ICON_PATH="$ICONS_DIR/${size}x${size}/apps"
    mkdir -p "$ICON_PATH"
    if [ "$size" = "256" ] && [ -f "$ICON_256" ]; then
        cp "$ICON_256" "$ICON_PATH/$BIN_NAME.png"
    elif [ -f "$ICON_SOURCE" ]; then
        cp "$ICON_SOURCE" "$ICON_PATH/$BIN_NAME.png"
    fi
done

# Install .desktop file
mkdir -p "$APPS_DIR"
cp "$DESKTOP_FILE" "$APPS_DIR/$BIN_NAME.desktop"

# Update desktop database
update-desktop-database "$APPS_DIR" 2>/dev/null || true

echo "Done. You may need to log out and back in for the icon to appear in your launcher."
echo ""
echo "Files installed:"
echo "  Desktop entry: $APPS_DIR/$BIN_NAME.desktop"
echo "  Icons:         $ICONS_DIR/{size}x{size}/apps/$BIN_NAME.png"
