# Snap

**Visual annotation layer for agentic coding.**

Annotate your screen and feed marked-up screenshots directly into Claude Code (or any MCP-compatible AI agent). See a bug? Circle it, label it, hit enter. Your agent sees exactly what you see.

```
You see a bug  -->  Ctrl+Shift+S  -->  Circle it, type "fix this"  -->  Enter
                                                                          |
                                                            ~/.snap/inbox/
                                                         snap-20260409-040216.png
                                                         snap-20260409-040216.json
                                                                          |
                                              Claude Code reads it via MCP  -->  Fix applied
```

---

## How It Works

Snap is two components:

1. **Annotation overlay** (Tauri 2.x, Rust + vanilla JS) — a global-hotkey-triggered fullscreen overlay that captures your screen, lets you draw on it, and saves annotated screenshots + structured metadata to `~/.snap/inbox/`.

2. **MCP server** (Python, fastmcp) — exposes the inbox to AI agents via stdio transport. Claude Code calls `get_latest_annotation()` and sees your marked-up screenshot with structured annotation data.

### The Flow

1. Press `Ctrl+Shift+S` from anywhere
2. Your screen freezes into an annotation canvas
3. Circle things, draw arrows, type labels, number issues
4. Hit Enter (or click the green checkmark)
5. Annotated PNG + structured JSON metadata drops into `~/.snap/inbox/`
6. In Claude Code: "check my latest snap annotation"
7. Claude Code reads the image and metadata, understands what you marked, makes the fix

### What Gets Saved

Every save produces a matched pair:

**`snap-{timestamp}.png`** — Your screen capture with all annotations composited directly into the image. What you drew is what the agent sees.

**`snap-{timestamp}.json`** — Structured metadata:
```json
{
  "timestamp": "2026-04-09T04:02:15.897Z",
  "source": {
    "window_title": "localhost:3000/dashboard - Brave",
    "window_class": "Brave-browser",
    "pid": 12345,
    "display": "primary",
    "resolution": [3840, 2160]
  },
  "annotations": [
    {
      "type": "circle",
      "center": [340, 220],
      "radius": [45, 45],
      "color": "#FF3B30",
      "label": null
    },
    {
      "type": "text",
      "position": [350, 310],
      "content": "fix this alignment",
      "color": "#FF3B30"
    }
  ],
  "image_filename": "snap-20260409-040215.png"
}
```

---

## Annotation Tools

| Tool | Shortcut | Description |
|------|----------|-------------|
| Circle | `C` | Click-drag to draw ellipses. Shift constrains to circle. |
| Rectangle | `R` | Click-drag to draw boxes. Shift constrains to square. 10% fill for visibility. |
| Arrow | `A` | Click start, drag to end. Arrowhead on the endpoint. |
| Freehand | `F` | Click-drag to draw smoothed paths. Quadratic curve interpolation. |
| Text | `T` | Click to place a label. Type your instruction. Enter to confirm. |
| Numbered Marker | `N` | Click to place auto-incrementing circled numbers (1, 2, 3...). |

### Controls

| Key | Action |
|-----|--------|
| `Ctrl+Shift+S` | Open annotation overlay (global, works from any app) |
| `D` | Toggle dim layer (darkens background for contrast) |
| `Ctrl+Z` | Undo last annotation |
| `Enter` | Save annotated screenshot and close |
| `Escape` | Cancel (discard) and close |

### Toolbar

- **6 color swatches**: Red (default), Blue, Green, Yellow, White, Black
- **3 stroke widths**: Thin (2px), Medium (4px, default), Thick (6px)
- **Draggable**: Grab the handle on the left to reposition the toolbar
- **Dim toggle**: Adds a dark overlay behind annotations for readability on busy screens

---

## Quick Start

```bash
# Install system deps (Ubuntu/Debian)
make deps

# Build the Tauri app + MCP server
make build

# Register with all your AI tools (Claude Code, Claude Desktop, Cursor, Windsurf)
./setup-mcp.sh

# Configure the hotkey (see SETUP.md for X11/Sway/Hyprland)
# GNOME Wayland:
chmod +x snap-trigger.sh
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings \
  "['/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/snap/']"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/snap/ \
  name "Snap Annotation"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/snap/ \
  command "$(pwd)/snap-trigger.sh"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/snap/ \
  binding "<Control><Shift>s"
```

Press `Ctrl+Shift+S`, draw on your screen, hit Enter. Then tell your agent: *"check my latest snap annotation"*

For detailed setup instructions, see **[SETUP.md](SETUP.md)**.

---

## macOS Quick Install

```bash
# 1) Build app
make build

# 2) Install into /Applications
cp -R app/src-tauri/target/release/bundle/macos/snap.app /Applications/

# 3) Launch
open -a /Applications/snap.app
```

First run:
- Press `Ctrl+Shift+S` once to trigger permission prompts.
- Allow Screen Recording/Automation/Accessibility if asked.

You can also add this to start-up to ensure it runs headless on each login. 

If permissions get stuck:

```bash
tccutil reset All com.adjective.snap
open -a /Applications/snap.app
```

---

## Example Workflow

You're vibe engineering a web app. You open it in the browser, scroll through, and spot three issues.

**1. Snap the bugs** (takes ~15 seconds total)

```
Ctrl+Shift+S  →  circle the broken button, type "this should be blue"  →  Enter
Ctrl+Shift+S  →  arrow at the spacing gap, type "16px here"            →  Enter
Ctrl+Shift+S  →  number markers 1, 2, 3 on misaligned cards            →  Enter
```

Each save drops a PNG+JSON pair into `~/.snap/inbox/`. Three screenshots, three pairs.

**2. Tell your agent** (one prompt)

```
> fix the frontend issues I just annotated
```

If your CLAUDE.md has the Snap awareness instructions (see [SETUP.md](SETUP.md#add-snap-awareness-to-claudemd)), the agent automatically calls `check_new_annotations()` at the start of any UI task. It sees 3 new annotations, reads all 3 images and metadata, and starts fixing.

Without the CLAUDE.md instructions, just say:

```
> check my snap annotations and fix all of them
```

**3. Agent reads and fixes**

The agent calls `list_annotations(3)`, gets back:
- Screenshot 1: red circle on a button + text "this should be blue"
- Screenshot 2: red arrow at a gap + text "16px here"
- Screenshot 3: numbered markers on 3 misaligned cards

It reads each image, sees exactly what you circled, and applies the fixes.

**4. Clear the inbox**

After verifying the fixes:

```
> looks good, clear the snap inbox
```

The agent calls `clear_inbox()`. The queue is empty, ready for the next round.

**The inbox is a conveyor belt, not a filing cabinet.** Annotations go in, the agent processes them, you clear it.

---

## MCP Tools

The MCP server exposes 5 tools to any MCP-compatible AI agent:

| Tool | Description |
|------|-------------|
| `check_new_annotations()` | Are there new annotations since the agent last checked? Returns count and filenames. |
| `get_latest_annotation()` | Get the most recent annotation's image path + full metadata. |
| `list_annotations(last_n)` | List the N most recent annotations with metadata. |
| `get_annotation(filename)` | Get a specific annotation by its filename (without extension). |
| `clear_inbox()` | Delete all processed annotations from the inbox. |

### How Agents Interpret Annotations

The following conventions are documented in your `CLAUDE.md` so agents know the visual language:

- **Red circles/rectangles** = "this area has a problem"
- **Arrows** = "this should move/connect there"
- **Text labels** = literal instructions ("fix this", "16px gap", "wrong color")
- **Numbered markers** = ordered list of issues (fix #1 first, then #2, etc.)
- **Freehand marks** = emphasis, "look at this general area"

---

## Architecture

```
snap/
  app/                      Tauri 2.x annotation overlay
    src/                    Vanilla HTML/CSS/JS frontend
      index.html            Toolbar + canvas markup
      main.js               Drawing engine, save logic, DPI handling
      styles.css            Toolbar styling, animations
    src-tauri/              Rust backend
      src/
        main.rs             App lifecycle (tray mode on X11, overlay mode on Wayland)
        lib.rs              Screen capture, window context, save handler, logging
      Cargo.toml            Rust dependencies
      tauri.conf.json       Window config, asset protocol, permissions
      capabilities/         Tauri 2 permission definitions
      icons/                Tray icon
    package.json            Node dependencies (@tauri-apps/api, @tauri-apps/cli)

  mcp-server/               Python MCP server
    server.py               5 tools: check, get_latest, list, get, clear
    pyproject.toml           Package metadata + fastmcp dependency
    .venv/                  Python virtual environment (created during setup)

  snap-trigger.sh           GNOME hotkey trigger script (Wayland)
  snap.service              systemd user service (X11 tray mode)
  Makefile                  Build, install, start/stop commands
  CLAUDE.md                 Project instructions for Claude Code
  SETUP.md                  Detailed setup guide
  LICENSE                   MIT
```

### Platform Support

| Platform | Screen Capture | Window Context | Global Hotkey |
|----------|---------------|----------------|---------------|
| Linux X11 | `scrot` | `xdotool` (title, class, PID) | Tauri global-shortcut plugin (tray mode) |
| Linux Wayland (GNOME) | `gnome-screenshot` | Not available | GNOME custom keybinding → `snap-trigger.sh` |
| Linux Wayland (wlroots) | `grim` | Not available | Compositor keybinding → `snap-trigger.sh` |
| macOS | Planned | Planned | Planned |
| Windows | Planned | Planned | Planned |

The capture system tries tools in order of preference and falls back gracefully. On Wayland with GNOME, it tries `gnome-screenshot` first, then `grim`, then `scrot`.

### HiDPI / 4K Display Support

Snap correctly handles high-DPI displays. The canvas renders at physical pixel resolution for crisp annotations, while all drawing coordinates use logical pixels. The export pipeline produces a 1080p composited PNG regardless of display resolution, keeping file sizes reasonable for IPC and agent consumption.

---

## Data & Privacy

- All data stays local. Screenshots are saved to `~/.snap/inbox/` and nowhere else.
- The MCP server is read-only over stdio. It never writes to the inbox, only reads and deletes.
- No network calls. No telemetry. No cloud.
- The screen capture uses your system's native screenshot tool (`gnome-screenshot`, `scrot`, or `grim`).
- Log file at `~/.snap/snap.log` (auto-rotates at 1MB). Contains timestamps and event names only, no image data.

---

## Built By

[Rob Murtha — Adjective LLC

Contributors
Alec Lucas — macOS port](https://adjective.us)

## License

MIT — see [LICENSE](LICENSE).
