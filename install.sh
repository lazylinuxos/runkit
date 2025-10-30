#!/usr/bin/env bash
set -euo pipefail

SCRIPT_NAME=$(basename "$0")
ACTION=${1:-install}

DEST_DIR="/usr/libexec"
BINARIES=(
    "runkit"
    "runkitd"
)

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
        ;;
    uninstall)
        require_sudo
        uninstall_binaries
        ;;
    *)
        echo "Usage: $SCRIPT_NAME [install|uninstall]" >&2
        exit 1
        ;;
esac

echo "Done."
