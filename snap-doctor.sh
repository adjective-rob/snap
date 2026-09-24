#!/bin/bash
# Check a Snap install end to end and say what is missing. Read-only, except
# for one silent test screenshot that is deleted afterwards.
SNAP_DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
BINARY="$SNAP_DIR/app/src-tauri/target/release/snap"
PYTHON="$SNAP_DIR/mcp-server/.venv/bin/python"
KEY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/snap/"

ok()   { printf '  \033[0;32m✓\033[0m %s\n' "$1"; }
bad()  { printf '  \033[0;31m✗\033[0m %s\n' "$1"; FAIL=1; }
note() { printf '    %s\n' "$1"; }
FAIL=0

echo ""
echo "Snap doctor"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

OS="$(uname -s)"
SESSION="${XDG_SESSION_TYPE:-unknown}"
DESKTOP="${XDG_CURRENT_DESKTOP:-unknown}"
echo "Platform: $OS, session $SESSION, desktop $DESKTOP"
echo ""

# Binary
if [ -x "$BINARY" ]; then ok "app built: $BINARY"; else bad "app not built"; note "run: make build"; fi

# Capture tool
if [ "$OS" = "Linux" ]; then
    tmp="$(mktemp --suffix=.png)"; rm -f "$tmp"
    tool=""
    if [ "$SESSION" = "wayland" ]; then
        for t in gnome-screenshot grim; do
            if command -v "$t" >/dev/null 2>&1; then
                case $t in
                    gnome-screenshot) "$t" --file="$tmp" 2>/dev/null ;;
                    grim) "$t" "$tmp" 2>/dev/null ;;
                esac
                if [ -s "$tmp" ]; then tool="$t"; break; fi
            fi
        done
    else
        command -v scrot >/dev/null 2>&1 && scrot --overwrite "$tmp" 2>/dev/null && [ -s "$tmp" ] && tool=scrot
    fi
    if [ -n "$tool" ]; then
        size="$(python3 -c "import struct,sys;d=open(sys.argv[1],'rb').read(24);print('%dx%d'%struct.unpack('>II',d[16:24]))" "$tmp" 2>/dev/null)"
        ok "screen capture works via $tool (${size:-?})"
    else
        bad "no working screen capture tool"
        if [ "$SESSION" = "wayland" ]; then note "GNOME: sudo apt install gnome-screenshot    wlroots: sudo apt install grim";
        else note "sudo apt install scrot"; fi
    fi
    rm -f "$tmp"

    # Display info (what the overlay will be told about the monitor)
    python3 - <<'PY' 2>/dev/null
import gi; gi.require_version('Gdk', '3.0')
from gi.repository import Gdk
d = Gdk.Display.get_default()
for i in range(d.get_n_monitors()):
    m = d.get_monitor(i); g = m.get_geometry()
    print(f"    monitor {i}: {g.width}x{g.height} @ scale {m.get_scale_factor()} ({m.get_model()})")
PY

    # Hotkey
    if [ "$SESSION" = "wayland" ]; then
        if [[ "$DESKTOP" == *GNOME* ]]; then
            cmd="$(gsettings get org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY_PATH command 2>/dev/null | tr -d "'")"
            bind="$(gsettings get org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY_PATH binding 2>/dev/null | tr -d "'")"
            if [ -n "$cmd" ] && [ -x "$cmd" ]; then ok "GNOME hotkey $bind -> $cmd";
            else bad "GNOME hotkey not registered"; note "run: ./install-hotkey.sh"; fi
        else
            note "non-GNOME Wayland: bind a key to $SNAP_DIR/snap-trigger.sh in your compositor"
        fi
    else
        if systemctl --user is-active snap.service >/dev/null 2>&1; then ok "tray service running (hotkey Ctrl+Shift+S)";
        else bad "tray service not running"; note "run: make install   (or: systemctl --user start snap.service)"; fi
    fi
fi

# MCP server
if [ -x "$PYTHON" ] && (cd "$SNAP_DIR/mcp-server" && "$PYTHON" -c "from server import mcp" 2>/dev/null); then
    ok "MCP server imports"
else
    bad "MCP server not installed"; note "run: make build"
fi
if command -v claude >/dev/null 2>&1; then
    if claude mcp list 2>/dev/null | grep -q '^snap:'; then ok "registered with Claude Code";
    else bad "not registered with Claude Code"; note "run: ./setup-mcp.sh"; fi
fi

# Inbox and log
inbox="$HOME/.snap/inbox"
n="$(ls "$inbox"/snap-*.json 2>/dev/null | wc -l)"
ok "inbox $inbox ($n annotations)"
if [ -f "$HOME/.snap/snap.log" ]; then
    echo ""
    echo "Last log lines:"
    tail -n 6 "$HOME/.snap/snap.log" | sed 's/^/    /'
fi

echo ""
if [ "$FAIL" = 0 ]; then echo "All good. Press the hotkey, draw, hit Enter."; else echo "Fix the ✗ items above, then re-run ./snap-doctor.sh"; fi
