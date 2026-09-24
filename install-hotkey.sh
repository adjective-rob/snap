#!/bin/bash
# Register the Snap hotkey on Linux Wayland.
#
# GNOME: adds a custom keybinding that runs snap-trigger.sh. Existing custom
# shortcuts are kept. Other compositors: prints the line to add to your config.
#
#   ./install-hotkey.sh            # bind Ctrl+Shift+S
#   SNAP_HOTKEY='<Super><Shift>s' ./install-hotkey.sh
#   ./install-hotkey.sh --remove   # unregister
set -e

SNAP_DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
TRIGGER="$SNAP_DIR/snap-trigger.sh"
BINDING="${SNAP_HOTKEY:-<Control><Shift>s}"
SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
KEY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/snap/"

chmod +x "$TRIGGER"

is_gnome() {
    command -v gsettings >/dev/null 2>&1 && [[ "${XDG_CURRENT_DESKTOP:-}" == *GNOME* ]]
}

if ! is_gnome; then
    echo "Not a GNOME session (XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-unset})."
    echo "Bind a key to the trigger script in your compositor:"
    echo ""
    echo "  Sway     (~/.config/sway/config):      bindsym Ctrl+Shift+s exec $TRIGGER"
    echo "  Hyprland (~/.config/hypr/hyprland.conf): bind = CTRL SHIFT, S, exec, $TRIGGER"
    exit 0
fi

current="$(gsettings get "$SCHEMA" custom-keybindings)"

if [ "${1:-}" = "--remove" ]; then
    if [[ "$current" == *"$KEY_PATH"* ]]; then
        # Drop our entry from the list, whatever else is in it.
        new="$(python3 - "$current" "$KEY_PATH" <<'PY'
import ast, sys
lst = [p for p in ast.literal_eval(sys.argv[1].replace("@as ", "")) if p != sys.argv[2]]
print(repr(lst) if lst else "@as []")
PY
)"
        gsettings set "$SCHEMA" custom-keybindings "$new"
    fi
    gsettings reset-recursively "$SCHEMA.custom-keybinding:$KEY_PATH" 2>/dev/null || true
    echo "Snap hotkey removed."
    exit 0
fi

# Append our path to the list without clobbering the user's other shortcuts.
if [[ "$current" != *"$KEY_PATH"* ]]; then
    if [[ "$current" == "@as []" || "$current" == "[]" ]]; then
        new="['$KEY_PATH']"
    else
        new="${current%]}, '$KEY_PATH']"
    fi
    gsettings set "$SCHEMA" custom-keybindings "$new"
fi

gsettings set "$SCHEMA.custom-keybinding:$KEY_PATH" name "Snap Annotation"
gsettings set "$SCHEMA.custom-keybinding:$KEY_PATH" command "$TRIGGER"
gsettings set "$SCHEMA.custom-keybinding:$KEY_PATH" binding "$BINDING"

echo "Registered $BINDING -> $TRIGGER"

if ! command -v gnome-screenshot >/dev/null 2>&1; then
    echo ""
    echo "WARNING: gnome-screenshot is not installed; the overlay cannot capture the screen."
    echo "         sudo apt install gnome-screenshot"
fi
