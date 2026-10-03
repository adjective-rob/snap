#!/bin/bash
# Install Snap on Linux or macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/adjective-rob/snap/main/install.sh | bash
#
# Fetches the repo (scripts + MCP server), downloads the prebuilt app from the
# latest GitHub release (builds from source if there is none for this
# platform), wires up the hotkey, and registers the MCP server with your AI
# tools. Safe to re-run; that is also how you update.
#
#   SNAP_DIR=/some/path   where to put the checkout (default ~/.local/share/snap-annotate)
#   SNAP_BUILD=1          build from source even if a prebuilt app exists
#   SNAP_HOTKEY='<Super><Shift>s'   GNOME Wayland hotkey (default Ctrl+Shift+S)
set -euo pipefail

REPO="adjective-rob/snap"
say()  { printf '\033[0;34m==>\033[0m %s\n' "$1"; }
die()  { printf '\033[0;31merror:\033[0m %s\n' "$1" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
    Linux-x86_64)               ASSET="snap-linux-x86_64" ;;
    Darwin-arm64)               ASSET="snap-macos-arm64.zip" ;;
    Darwin-x86_64)              ASSET="snap-macos-x86_64.zip" ;;
    Linux-*|Darwin-*)           ASSET="" ;;  # no prebuilt app; build from source
    *) die "unsupported platform $OS. On Windows, download the installer from https://github.com/$REPO/releases" ;;
esac

have git  || die "git is required"
have curl || die "curl is required"
have python3 || die "python3 (3.11+) is required for the MCP server"

# ----- 1. The checkout -----
# Run from inside a clone: use it as is. Otherwise clone (or update) SNAP_DIR,
# pinned to the latest release tag when there is one so scripts and app match.
SELF_DIR=""
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
    SELF_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}" 2>/dev/null || echo "${BASH_SOURCE[0]}")")" && pwd)"
fi

TAG="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null \
    | sed -n 's|.*/releases/tag/||p')" || TAG=""

if [ -n "$SELF_DIR" ] && [ -f "$SELF_DIR/snap-trigger.sh" ]; then
    SNAP_DIR="$SELF_DIR"
    say "Using this checkout: $SNAP_DIR"
else
    SNAP_DIR="${SNAP_DIR:-$HOME/.local/share/snap-annotate}"
    REF="${TAG:-main}"
    if [ -d "$SNAP_DIR/.git" ]; then
        say "Updating $SNAP_DIR to $REF"
        git -C "$SNAP_DIR" fetch --quiet --depth 1 origin "$REF"
        git -C "$SNAP_DIR" checkout --quiet --detach FETCH_HEAD
    else
        say "Cloning $REPO ($REF) into $SNAP_DIR"
        mkdir -p "$(dirname "$SNAP_DIR")"
        git clone --quiet --depth 1 --branch "$REF" "https://github.com/$REPO.git" "$SNAP_DIR"
    fi
fi
cd "$SNAP_DIR"

# ----- 2. The app -----
# Linux scripts expect the binary at its build path, so a download goes there.
BINARY="$SNAP_DIR/app/src-tauri/target/release/snap"
installed=""
if [ -z "${SNAP_BUILD:-}" ] && [ -n "$ASSET" ] && [ -n "$TAG" ]; then
    tmp="$(mktemp -d)"
    say "Downloading $ASSET ($TAG)"
    if curl -fsSL -o "$tmp/$ASSET" "https://github.com/$REPO/releases/download/$TAG/$ASSET"; then
        if [ "$OS" = "Darwin" ]; then
            rm -rf /Applications/snap.app
            ditto -x -k "$tmp/$ASSET" /Applications/
        else
            mkdir -p "$(dirname "$BINARY")"
            install -m 755 "$tmp/$ASSET" "$BINARY"
        fi
        installed=1
    else
        say "No prebuilt app for this platform in $TAG"
    fi
    rm -rf "$tmp"
fi

if [ -z "$installed" ]; then
    say "Building the app from source (a few minutes the first time)"
    have cargo || die "Rust is required to build: https://rustup.rs"
    have npm   || die "Node.js 18+ is required to build: https://nodejs.org"
    if [ "$OS" = "Darwin" ]; then
        (cd app && npm install && npx tauri build --bundles app)
        rm -rf /Applications/snap.app
        cp -R app/src-tauri/target/release/bundle/macos/snap.app /Applications/
    else
        (cd app && npm install && npx tauri build --no-bundle) \
            || die "build failed. Missing system libraries? See SETUP.md, or run: make deps"
    fi
fi

if [ "$OS" = "Linux" ]; then
    missing="$(ldd "$BINARY" 2>/dev/null | awk '/not found/ {print $1}' | tr '\n' ' ')"
    if [ -n "$missing" ]; then
        die "the app needs system libraries that are not installed: $missing
  Ubuntu/Debian: sudo apt install libwebkit2gtk-4.1-0 libayatana-appindicator3-1 libxdo3
  then re-run this script."
    fi
fi

# ----- 3. The MCP server -----
say "Installing the MCP server"
if have uv; then
    (cd mcp-server && uv venv --quiet --allow-existing .venv && uv pip install --quiet -e .)
else
    python3 -c 'import sys; sys.exit(sys.version_info < (3, 11))' \
        || die "python3 is older than 3.11. Install uv (https://astral.sh/uv) or a newer Python."
    (cd mcp-server && python3 -m venv .venv && .venv/bin/pip install --quiet -e .)
fi

# ----- 4. Hotkey / start on login -----
if [ "$OS" = "Darwin" ]; then
    say "Installing the LaunchAgent and starting Snap"
    mkdir -p "$HOME/Library/LaunchAgents" "$HOME/.snap"
    plist="$HOME/Library/LaunchAgents/com.adjective.snap.plist"
    sed "s|%SNAP_PATH%|$SNAP_DIR|g; s|%HOME_PATH%|$HOME|g" snap.plist > "$plist"
    launchctl unload "$plist" 2>/dev/null || true
    launchctl load -w "$plist"
elif [ "${XDG_SESSION_TYPE:-}" = "wayland" ]; then
    say "Registering the hotkey"
    ./install-hotkey.sh
else
    say "Installing the tray app as a user service"
    mkdir -p "$HOME/.config/systemd/user"
    sed "s|%h/Desktop/snap|$SNAP_DIR|g" snap.service > "$HOME/.config/systemd/user/snap.service"
    systemctl --user daemon-reload
    systemctl --user enable snap.service
    systemctl --user restart snap.service
fi

# ----- 5. AI tools -----
./setup-mcp.sh

if [ "$OS" = "Linux" ]; then
    ./snap-doctor.sh || true
else
    echo "Press Ctrl+Shift+S once and allow Screen Recording / Accessibility when macOS asks."
fi
echo ""
echo "Snap is installed in $SNAP_DIR. Re-run this script to update."
