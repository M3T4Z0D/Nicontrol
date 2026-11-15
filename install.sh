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

# Clean up legacy nitro-control artifacts from pre-rename installations
pkill -f "$BIN_DIR/nitro-control" >/dev/null 2>&1 || true
rm -f \
    "$BIN_DIR/nitro-control" \
    "$APP_DIR/nitro-control.desktop" \
    "$HOME/.config/autostart/nitro-control.desktop"

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

find_ec_driver_source() {
    for candidate in \
        "$HOME/acer-nitro-v15-linux-control/fan-control/driver" \
        "$HOME/acer_nitro_gaming_driver2" \
        "/opt/acer-nitro-v15-linux-control/fan-control/driver"
    do
        if [ -f "$candidate/Makefile" ] && [ -f "$candidate/acer_nitro_gaming_driver2.c" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

install_udev_rule() {
    local rules_src="$SCRIPT_DIR/assets/99-nicontrol.rules"
    if [ ! -f "$rules_src" ]; then
        echo "Warning: $rules_src not found; skipping udev rule installation."
        return 1
    fi
    sudo cp "$rules_src" /etc/udev/rules.d/99-nicontrol.rules
    sudo udevadm control --reload-rules
    sudo udevadm trigger
    echo "Installed udev rule to /etc/udev/rules.d/99-nicontrol.rules."
}

check_and_fix_ec_driver() {
    local kver
    kver="$(uname -r)"

    if [ ! -e /dev/turbo ]; then
        echo ""
        if compgen -G "/lib/modules/*/extra/acer_nitro_gaming_driver2.ko*" >/dev/null 2>&1; then
            echo "[!] EC driver notice: /dev/turbo is missing on kernel $kver."
            echo "    acer_nitro_gaming_driver2 was found in an older kernel, which usually means"
            echo "    your system updated its kernel and the module needs to be rebuilt for $kver."
        elif [ -e /sys/firmware/acpi/platform_profile ]; then
            echo "[i] /dev/turbo not found; Nicontrol will use ACPI platform_profile fallback (Auto/Turbo)."
            return 0
        else
            echo "[!] EC driver notice: /dev/turbo was not found on kernel $kver."
        fi

        local driver_dir=""
        driver_dir="$(find_ec_driver_source || true)"
        if [ -n "$driver_dir" ] && [ -d "/lib/modules/$kver/build" ]; then
            echo "    Found driver source at: $driver_dir"
            if [ -t 0 ]; then
                read -r -p "Rebuild, install, and load acer_nitro_gaming_driver2 for $kver now? [y/N] " ans
                if [[ "$ans" =~ ^[YySs]$ ]]; then
                    make -C "$driver_dir" clean
                    make -C "$driver_dir"
                    sudo mkdir -p "/lib/modules/$kver/extra"
                    sudo cp "$driver_dir/acer_nitro_gaming_driver2.ko" "/lib/modules/$kver/extra/"
                    sudo depmod -a
                    echo "acer_nitro_gaming_driver2" | sudo tee /etc/modules-load.d/acer_nitro_gaming_driver2.conf >/dev/null
                    sudo modprobe acer_nitro_gaming_driver2
                    install_udev_rule || true
                    echo "Driver rebuilt and loaded successfully."
                else
                    echo "    Skipped. You can run manually:"
                    echo "      make -C \"$driver_dir\" clean && make -C \"$driver_dir\""
                    echo "      sudo mkdir -p \"/lib/modules/$kver/extra\" && sudo cp \"$driver_dir/acer_nitro_gaming_driver2.ko\" \"/lib/modules/$kver/extra/\""
                    echo "      sudo depmod -a && sudo modprobe acer_nitro_gaming_driver2"
                fi
            else
                echo "    Run manually to rebuild for $kver:"
                echo "      make -C \"$driver_dir\" clean && make -C \"$driver_dir\""
                echo "      sudo mkdir -p \"/lib/modules/$kver/extra\" && sudo cp \"$driver_dir/acer_nitro_gaming_driver2.ko\" \"/lib/modules/$kver/extra/\""
                echo "      sudo depmod -a && sudo modprobe acer_nitro_gaming_driver2"
            fi
        elif [ -n "$driver_dir" ]; then
            echo "    Driver source found at $driver_dir, but kernel headers (/lib/modules/$kver/build) are missing."
        fi
    fi

    if [ -e /dev/turbo ] && [ ! -w /dev/turbo ]; then
        echo ""
        echo "[!] Permission notice: /dev/turbo exists, but your user cannot write to it."
        if [ -t 0 ]; then
            read -r -p "Install udev rule (/etc/udev/rules.d/99-nicontrol.rules) now? [y/N] " ans
            if [[ "$ans" =~ ^[YySs]$ ]]; then
                install_udev_rule || true
            fi
        else
            echo "    Run: sudo cp \"$SCRIPT_DIR/assets/99-nicontrol.rules\" /etc/udev/rules.d/99-nicontrol.rules && sudo udevadm control --reload-rules && sudo udevadm trigger"
        fi
    fi
}

check_and_fix_ec_driver

echo ""
echo "Make sure $BIN_DIR is in your PATH."
echo "To run the system tray widget now:"
echo "  $BIN_DIR/nicontrol --tray &"
echo ""
echo "To launch the GUI panel:"
echo "  $BIN_DIR/nicontrol"
