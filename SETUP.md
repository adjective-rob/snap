# Snap Setup Guide

Step-by-step instructions to build, install, and configure Snap on Linux.

---

## Prerequisites

### Required

| Dependency | Purpose | Install |
|-----------|---------|---------|
| Rust toolchain | Builds the Tauri app | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js 18+ | Tauri CLI and frontend bundling | `nvm install 20` or [nodejs.org](https://nodejs.org) |
| Python 3.11+ | MCP server runtime | Usually pre-installed on Ubuntu 22.04+ |
| uv | Python package manager | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |

### System Libraries (Ubuntu/Debian)

```bash
sudo apt install -y \
  pkg-config \
  libwebkit2gtk-4.1-dev \
  build-essential \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### Screen Capture Tool

You need **one** of these, depending on your display server:

| Display Server | Tool | Install |
|---------------|------|---------|
| GNOME on Wayland (Ubuntu default) | `gnome-screenshot` | `sudo apt install gnome-screenshot` |
| wlroots Wayland (Sway, Hyprland) | `grim` | `sudo apt install grim` |
| X11 | `scrot` | `sudo apt install scrot` |

**How to check your display server:**
```bash
echo $XDG_SESSION_TYPE
# "wayland" or "x11"
```

Snap tries capture tools in order of preference and uses the first one that works. Install multiple if you switch between display servers.

### Window Context (Optional, X11 only)

```bash
sudo apt install xdotool
```

This captures the active window title, class, and PID when you trigger Snap. On Wayland, window context is not available (GNOME doesn't expose it to external tools). The sidecar JSON will have `null` for these fields on Wayland — annotations still work fine, the agent just won't know which window you were looking at.

---

## Build

### 1. Build the Tauri App

```bash
cd app
npm install
npx tauri build
```

This produces a release binary at:
```
app/src-tauri/target/release/snap
```

Build takes ~60-90 seconds on first run (compiling ~500 Rust crates). Subsequent rebuilds are ~20 seconds.

### 2. Install the MCP Server

```bash
cd mcp-server
uv venv .venv
uv pip install -e .
```

### 3. Verify Both Components

```bash
# Check the binary exists
ls -lh app/src-tauri/target/release/snap

# Check the MCP server loads
mcp-server/.venv/bin/python -c "from server import mcp; print('MCP server OK')"
```

Or use the Makefile:
```bash
make build
```

---

## Configure the Global Hotkey

### GNOME on Wayland (Ubuntu 22.04+, default)

Tauri's global shortcut plugin doesn't work on Wayland. Instead, GNOME handles the hotkey and runs a trigger script.

**1. Register the keybinding:**
```bash
make hotkey          # or: ./install-hotkey.sh
```

This makes `snap-trigger.sh` executable and adds a GNOME custom keybinding for `Ctrl+Shift+S` that runs it. Your existing custom shortcuts are left alone. (`make install` does this for you on a Wayland session.)

**2. Test it:**

Press `Ctrl+Shift+S`. The overlay should appear with a screenshot of your desktop.

**To change the hotkey**, pass a GTK accelerator:
```bash
SNAP_HOTKEY='<Super><Shift>s' make hotkey    # Windows key + Shift + S
SNAP_HOTKEY='<Control><Shift>x' make hotkey  # Ctrl + Shift + X
```

**To remove the keybinding:**
```bash
./install-hotkey.sh --remove
```

### X11

On X11, Snap runs as a background tray app with a built-in global hotkey. No GNOME keybinding needed.

```bash
# Install and enable the systemd service
make install
```

Or manually:
```bash
mkdir -p ~/.config/systemd/user
cp snap.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable snap.service
systemctl --user start snap.service
```

The app starts silently in the system tray. Press `Ctrl+Shift+S` to trigger the overlay.

**Service management:**
```bash
make start     # Start the service
make stop      # Stop the service
make restart   # Restart after rebuild
make status    # Check if running
make logs      # Tail the systemd journal
```

### Other Wayland Compositors (Sway, Hyprland, etc.)

Bind `Ctrl+Shift+S` to the trigger script in your compositor config:

**Sway** (`~/.config/sway/config`):
```
bindsym Ctrl+Shift+s exec ~/Desktop/snap/snap-trigger.sh
```

**Hyprland** (`~/.config/hypr/hyprland.conf`):
```
bind = CTRL SHIFT, S, exec, ~/Desktop/snap/snap-trigger.sh
```

---

## Register the MCP Server

### Automatic Setup (Recommended)

The setup script detects which AI tools you have installed and registers Snap with all of them:

```bash
./setup-mcp.sh
```

Output looks like:
```
Snap MCP Setup
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ MCP server verified

Detecting installed tools...

✓ Claude Code — registered
✓ Claude Desktop — configured at ~/.config/Claude/claude_desktop_config.json
✓ Cursor — configured at ~/.cursor/mcp.json
⊘ Windsurf — not installed, skipping
```

It auto-detects and configures: **Claude Code**, **Claude Desktop**, **Cursor**, and **Windsurf**. Tools that aren't installed are skipped.

### Manual Setup

If you prefer to register manually, or use a tool the script doesn't cover:

**Claude Code:**
```bash
claude mcp add --transport stdio snap -- \
  $HOME/Desktop/snap/mcp-server/.venv/bin/python \
  $HOME/Desktop/snap/mcp-server/server.py
```

**Claude Desktop** — add to `~/.config/Claude/claude_desktop_config.json` (Linux) or `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS):
```json
{
  "mcpServers": {
    "snap": {
      "command": "/home/YOU/Desktop/snap/mcp-server/.venv/bin/python",
      "args": ["/home/YOU/Desktop/snap/mcp-server/server.py"]
    }
  }
}
```

**Cursor** — add to `~/.cursor/mcp.json`:
```json
{
  "mcpServers": {
    "snap": {
      "command": "/home/YOU/Desktop/snap/mcp-server/.venv/bin/python",
      "args": ["/home/YOU/Desktop/snap/mcp-server/server.py"]
    }
  }
}
```

**Windsurf** — add to `~/.codeium/windsurf/mcp_config.json`:
```json
{
  "mcpServers": {
    "snap": {
      "command": "/home/YOU/Desktop/snap/mcp-server/.venv/bin/python",
      "args": ["/home/YOU/Desktop/snap/mcp-server/server.py"]
    }
  }
}
```

**VS Code + Cline** — add to Cline's MCP settings in the extension config:
```json
{
  "snap": {
    "command": "/home/YOU/Desktop/snap/mcp-server/.venv/bin/python",
    "args": ["/home/YOU/Desktop/snap/mcp-server/server.py"]
  }
}
```

**Any other MCP client** — the two values you need:
```
command: /home/YOU/Desktop/snap/mcp-server/.venv/bin/python
args:    /home/YOU/Desktop/snap/mcp-server/server.py
```

Replace `/home/YOU/Desktop/snap` with wherever you cloned the repo.

### Verify

```bash
# Claude Code
claude mcp list
# Should show: snap: ... - ✓ Connected
```

### Test It

1. Take a snap annotation (`Ctrl+Shift+S` → draw → Enter)
2. Open any registered agent
3. Say: "check my latest snap annotation"
4. The agent calls `get_latest_annotation()`, reads the image, and sees what you marked

---

## Add Snap Awareness to CLAUDE.md

Add this to your global `~/.claude/CLAUDE.md` so Claude Code automatically checks for annotations:

```markdown
## Snap — Visual Annotations

You have access to the `snap` MCP tools for reading annotated screenshots.

### Tools:
- `check_new_annotations()` — call at the start of every task to see if there's visual feedback
- `get_latest_annotation()` — returns the most recent annotated screenshot path + metadata
- `list_annotations(last_n)` — returns recent annotations
- `get_annotation(filename)` — returns a specific annotation
- `clear_inbox()` — deletes processed annotations

### When to use:
- At the START of any UI/frontend task, call `check_new_annotations()` first
- If the user says "look at my annotation" or "check my snap", call `get_latest_annotation()`
- After getting metadata, READ THE IMAGE at `image_path` to see the annotated screenshot
- Use `source.window_title` to infer which page/component the user was looking at

### How to interpret:
- Red circles/rectangles = "this area has a problem"
- Arrows = "this should move/connect there"
- Text labels = literal instructions ("fix this", "16px gap", "wrong color")
- Numbered markers = ordered list of issues (fix #1 first, then #2, etc.)
- Freehand marks = emphasis, "look at this general area"
```

For project-specific CLAUDE.md files, add a shorter pointer:

```markdown
## Visual Feedback
This project uses Snap for visual annotations. Call `check_new_annotations()` at the start of UI tasks.
```

---

## File Locations

| Path | Purpose |
|------|---------|
| `~/.snap/inbox/` | Annotated screenshots + sidecar JSON |
| `~/.snap/snap.log` | Event log (auto-rotates at 1MB) |
| `~/.snap/.last_read` | MCP server's read cursor (tracks which annotations are "new") |
| `/tmp/snap-capture.png` | Temporary screen capture (overwritten each time) |
| `/tmp/snap-overlay.lock` | Lock file preventing concurrent overlays |

---

## Troubleshooting

### "Screen capture failed" error on overlay

**Cause:** No screenshot tool installed, or the installed tool doesn't work with your display server.

**Fix:** Install the right tool for your display server (see Prerequisites above). Test it manually:
```bash
# GNOME Wayland
gnome-screenshot --file=/tmp/test.png && echo "OK"

# wlroots Wayland
grim /tmp/test.png && echo "OK"

# X11
scrot /tmp/test.png && echo "OK"
```

### Hotkey doesn't do anything (Wayland)

**Cause:** GNOME custom keybinding not registered, or `snap-trigger.sh` isn't executable.

**Fix:**
```bash
# Check the keybinding is registered
gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings

# Make sure the script is executable
chmod +x snap-trigger.sh

# Check the log for trigger events
tail ~/.snap/snap.log
```

### Overlay appears but annotations don't save

**Cause:** The `Enter` key or save button isn't connected, or the inbox directory isn't writable.

**Fix:**
```bash
# Check the inbox directory exists and is writable
ls -la ~/.snap/inbox/

# Check the log for save events
grep "Saved annotation" ~/.snap/snap.log

# Try clicking the green checkmark button instead of pressing Enter
```

### Not sure what is wrong

```bash
./snap-doctor.sh
```

It checks the build, takes a silent test capture with the right tool for your session, prints what GDK reports for your monitors, and verifies the hotkey and MCP registration.

### Screenshot appears zoomed in, tiny in the corner, or offset

**Cause:** the overlay window is not the size of the screen, or the display scale is not what the app was told.

**Fix:** press the hotkey once, then look at the last lines of `~/.snap/snap.log`. The app logs the monitor size and scale it was given, the window size it created, and what the page saw:

```
monitor Some("LG HDR 4K"): 7680x4320 physical, scale 2
sizing overlay to monitor: 3840x2160 logical
[ui] after show: viewport 1920x1080 dpr=2 screen=3840x2160 capture=3840x2160 draw=1920x1080@0,0
```

`viewport` should be your screen's logical size, `capture` its physical size, and `draw` should equal the viewport at `@0,0` for a single monitor. (GNOME's GDK reports the monitor in physical pixels, hence the doubled "physical" line; the compositor clamps the window, so this is harmless.) If they disagree, open an issue with those lines.

### MCP server not connected

**Fix:**
```bash
# Re-register
claude mcp remove snap
claude mcp add --transport stdio snap -- \
  $HOME/Desktop/snap/mcp-server/.venv/bin/python \
  $HOME/Desktop/snap/mcp-server/server.py

# Verify
claude mcp list
```

### Overlay window doesn't close after save

**Cause:** The process might be stuck.

**Fix:**
```bash
# Kill it
kill $(pgrep -f "target/release/snap")
rm -f /tmp/snap-overlay.lock
```

### Log file growing too large

Won't happen — both the Rust app and MCP server auto-rotate `~/.snap/snap.log` at 1MB, keeping one backup at `snap.log.old`.

---

## Uninstall

```bash
# Remove GNOME keybinding (Wayland)
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "[]"

# Remove systemd service (X11)
systemctl --user stop snap.service
systemctl --user disable snap.service
rm ~/.config/systemd/user/snap.service

# Remove MCP server from Claude Code
claude mcp remove snap

# Remove Snap data
rm -rf ~/.snap

# Remove Snap awareness from ~/.claude/CLAUDE.md
# (edit manually — remove the "Snap — Visual Annotations" section)

# Delete the repo
rm -rf ~/Desktop/snap
```
