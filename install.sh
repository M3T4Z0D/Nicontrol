#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"
SVG_ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR" "$SVG_ICON_DIR"

if [ -f "$SCRIPT_DIR/nicontrol" ] && [ ! -f "$SCRIPT_DIR/Cargo.toml" ]; then
    echo "Installing pre-built nicontrol binary..."
    rm -f "$BIN_DIR/nicontrol"
    cp "$SCRIPT_DIR/nicontrol" "$BIN_DIR/nicontrol"
elif [ -f "$SCRIPT_DIR/nicontrol" ] && ! command -v cargo >/dev/null 2>&1; then
    echo "Installing pre-built nicontrol binary..."
    rm -f "$BIN_DIR/nicontrol"
    cp "$SCRIPT_DIR/nicontrol" "$BIN_DIR/nicontrol"
else
    echo "Building nicontrol (release)..."
    cargo build --release
    rm -f "$BIN_DIR/nicontrol"
    cp target/release/nicontrol "$BIN_DIR/nicontrol"
fi

chmod +x "$BIN_DIR/nicontrol"

if [ -f "$SCRIPT_DIR/assets/nicontrol.png" ]; then
    cp "$SCRIPT_DIR/assets/nicontrol.png" "$ICON_DIR/nicontrol.png"
    cp "$SCRIPT_DIR/assets/nicontrol.png" "$HOME/.local/share/icons/nicontrol.png"
fi
if [ -f "$SCRIPT_DIR/assets/nicontrol.svg" ]; then
    cp "$SCRIPT_DIR/assets/nicontrol.svg" "$SVG_ICON_DIR/nicontrol.svg"
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
fi

cat > "$APP_DIR/nicontrol.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Nicontrol
Comment=Open-source Fan & Power Profile Control Center
Exec=$BIN_DIR/nicontrol
Icon=nicontrol
Terminal=false
Categories=System;Utility;
StartupWMClass=Nicontrol
EOF

echo "Installed binary to: $BIN_DIR/nicontrol"
echo "Installed desktop entry to: $APP_DIR/nicontrol.desktop"
echo ""
echo "Make sure $BIN_DIR is in your PATH."
echo "To run the system tray widget now:"
echo "  $BIN_DIR/nicontrol --tray &"
echo ""
echo "To launch the GUI panel:"
echo "  $BIN_DIR/nicontrol"
