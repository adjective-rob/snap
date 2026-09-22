#!/bin/bash
# Trigger a new Snap overlay. Called by the desktop environment's global hotkey.
# If the tray app is running, signal it. Otherwise launch the overlay directly.

# Resolve the repo from this script's own location so the checkout can live anywhere.
SNAP_DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
BINARY="$SNAP_DIR/app/src-tauri/target/release/snap"
LOCK="/tmp/snap-overlay.lock"
LOG="$HOME/.snap/snap.log"
# The app writes its raw capture to a fixed path in the shared /tmp. On a machine
# with several user accounts, a leftover file owned by another user makes the
# capture fail with "permission denied", so clean it up before and after each run.
CAPTURE="${TMPDIR:-/tmp}/snap-capture.png"

log() {
    mkdir -p "$HOME/.snap"
    echo "[$(date -u '+%Y-%m-%d %H:%M:%S')] [trigger] $1" >> "$LOG"
}

# Guard: don't open if overlay is already active
if [ -f "$LOCK" ]; then
    pid=$(cat "$LOCK" 2>/dev/null)
    if kill -0 "$pid" 2>/dev/null; then
        log "overlay already active (pid=$pid), ignoring"
        exit 0
    else
        rm -f "$LOCK"
    fi
fi

log "hotkey triggered — launching overlay"

if [ -e "$CAPTURE" ] && ! rm -f "$CAPTURE" 2>/dev/null; then
    log "WARNING: cannot remove $CAPTURE (owned by $(stat -c %U "$CAPTURE" 2>/dev/null)); capture will fail until that user removes it"
fi

# Launch the app in single-shot overlay mode
# The app will capture screen, show overlay, save, and exit
"$BINARY" --overlay-mode &
OVERLAY_PID=$!
echo "$OVERLAY_PID" > "$LOCK"

# Clean up lock when overlay exits
wait "$OVERLAY_PID" 2>/dev/null
rm -f "$LOCK" "$CAPTURE"
log "overlay closed"
