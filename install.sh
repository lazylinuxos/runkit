#!/usr/bin/env bash
set -euo pipefail

SCRIPT_NAME=$(basename "$0")
ACTION=${1:-install}

DEST_DIR="/usr/libexec"
BINARIES=(
    "runkit"
    "runkitd"
)

ICON_BASE_NAME="runkit"
ICON_SOURCE_DIR="assets/icons/hicolor"
ICON_TARGET_BASE="/usr/share/icons/hicolor"
ICON_SIZES=(16x16 24x24 32x32 48x48 64x64 96x96 128x128 256x256 512x512)

DESKTOP_SOURCE="assets/applications/tech.geektoshi.Runkit.desktop"
DESKTOP_TARGET="/usr/share/applications/tech.geektoshi.Runkit.desktop"

require_sudo() {
    sudo -v
}

install_dependencies() {
    echo "Installing system dependencies..."
    local deps=(
        rustup
        gtk4-devel
        libadwaita-devel
        glib-devel
        pango-devel
        pkg-config
    )
    sudo xbps-install -S "${deps[@]}"
}

build_binaries() {
    echo "Building packages..."
    cargo build --release
}

install_binaries() {
    local src_dir="target/release"

    if [[ ! -d "$src_dir" ]]; then
        echo "Error: Source directory '$src_dir' does not exist."
        exit 1
    fi

    for bin in "${BINARIES[@]}"; do
        local src_path="${src_dir}/${bin}"
        local dest_path="${DEST_DIR}/${bin}"

        if [[ ! -f "$src_path" ]]; then
            echo "Warning: '$src_path' not found – skipping."
            continue
        fi

        echo "Installing '$src_path' → '$dest_path'..."
        sudo install -m755 "$src_path" "$DEST_DIR"
    done
}

install_icons() {
    local installed_any=false
    local missing_sizes=()

    for size in "${ICON_SIZES[@]}"; do
        local src="${ICON_SOURCE_DIR}/${size}/apps/${ICON_BASE_NAME}.png"
        if [[ -f "$src" ]]; then
            local dest="${ICON_TARGET_BASE}/${size}/apps/${ICON_BASE_NAME}.png"
            echo "Installing icon '$src' -> '$dest'..."
            sudo install -D -m644 "$src" "$dest"
            installed_any=true
        else
            missing_sizes+=("$size")
        fi
    done

    if (( ${#missing_sizes[@]} > 0 )); then
        echo "Note: PNG icons missing for sizes: ${missing_sizes[*]}"
        echo "      Place files under ${ICON_SOURCE_DIR}/<size>/apps/${ICON_BASE_NAME}.png"
    fi

    local svg_src="${ICON_SOURCE_DIR}/scalable/apps/${ICON_BASE_NAME}.svg"
    if [[ -f "$svg_src" ]]; then
        local svg_dest="${ICON_TARGET_BASE}/scalable/apps/${ICON_BASE_NAME}.svg"
        echo "Installing icon '$svg_src' -> '$svg_dest'..."
        sudo install -D -m644 "$svg_src" "$svg_dest"
        installed_any=true
    else
        echo "Note: scalable icon missing at ${svg_src}"
    fi

    if [[ "$installed_any" == true ]]; then
        refresh_icon_cache
    fi
}

install_desktop_entry() {
    if [[ -f "$DESKTOP_SOURCE" ]]; then
        echo "Installing desktop entry '$DESKTOP_SOURCE' -> '$DESKTOP_TARGET'..."
        sudo install -D -m644 "$DESKTOP_SOURCE" "$DESKTOP_TARGET"
        refresh_desktop_database
    else
        echo "Note: desktop entry not found at ${DESKTOP_SOURCE}; skipping."
    fi
}

uninstall_icons() {
    local removed_any=false

    for size in "${ICON_SIZES[@]}"; do
        local dest="${ICON_TARGET_BASE}/${size}/apps/${ICON_BASE_NAME}.png"
        if [[ -f "$dest" ]]; then
            echo "Removing icon '$dest'..."
            sudo rm -f "$dest"
            removed_any=true
        fi
    done

    local svg_dest="${ICON_TARGET_BASE}/scalable/apps/${ICON_BASE_NAME}.svg"
    if [[ -f "$svg_dest" ]]; then
        echo "Removing icon '$svg_dest'..."
        sudo rm -f "$svg_dest"
        removed_any=true
    fi

    if [[ "$removed_any" == true ]]; then
        refresh_icon_cache
    fi
}

refresh_icon_cache() {
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        echo "Updating icon cache..."
        sudo gtk-update-icon-cache -f "$ICON_TARGET_BASE"
    fi
}

uninstall_desktop_entry() {
    if [[ -f "$DESKTOP_TARGET" ]]; then
        echo "Removing desktop entry '$DESKTOP_TARGET'..."
        sudo rm -f "$DESKTOP_TARGET"
        refresh_desktop_database
    fi
}

refresh_desktop_database() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        local dir
        dir=$(dirname "$DESKTOP_TARGET")
        echo "Refreshing desktop database for $dir..."
        sudo update-desktop-database "$dir"
    fi
}

uninstall_binaries() {
    echo "Removing installed binaries..."

    for bin in "${BINARIES[@]}"; do
        local dest_path="${DEST_DIR}/${bin}"

        if [[ ! -f "$dest_path" ]]; then
            echo "Skipping '${dest_path}'; not present."
            continue
        fi

        echo "Removing '$dest_path'..."
        sudo rm -f "$dest_path"
    done
}

case "$ACTION" in
    install)
        require_sudo
        install_dependencies
        build_binaries
        install_binaries
        install_icons
        install_desktop_entry
        ;;
    uninstall)
        require_sudo
        uninstall_binaries
        uninstall_icons
        uninstall_desktop_entry
        ;;
    *)
        echo "Usage: $SCRIPT_NAME [install|uninstall]" >&2
        exit 1
        ;;
esac

echo "Done."
