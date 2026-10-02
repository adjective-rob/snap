# Snap -- Project Context for Claude Code

## What this is

A two-component system: a Tauri 2.x desktop app (Rust + vanilla JS) for screen annotation, and a Python MCP server that exposes the annotation inbox to coding agents.

## Architecture

- `app/` -- Tauri 2.x project. Vanilla HTML/CSS/JS frontend, no framework. Rust backend handles screen capture, window context, and file I/O.
- `mcp-server/` -- Python package using `fastmcp`. Stdio transport. Reads from `~/.snap/inbox/`.

## Key decisions

- Vanilla JS for the overlay frontend. No React, no build tooling beyond what Tauri provides.
- Screen capture on Wayland goes through the XDG desktop portal first (`org.freedesktop.portal.Screenshot` over D-Bus via `gio`). GNOME 50 stopped letting `gnome-screenshot` capture, so the portal is the only silent route there. Fallbacks run as subprocesses with a 10s timeout: `gnome-screenshot` (older GNOME), `grim` (wlroots), `scrot` (X11). Tries each in order, uses the first that works.
- Window context uses `xdotool` on X11. Returns None on Wayland.
- Annotations stored as PNG + sidecar JSON pairs in `~/.snap/inbox/`.
- MCP server is read-only. It never writes annotations, only reads and deletes.
- On Wayland: app runs in single-shot overlay mode, triggered by GNOME custom keybinding via `snap-trigger.sh`.
- On X11: app runs as a persistent tray app with built-in global hotkey via Tauri's global-shortcut plugin.

## Conventions

- Tauri commands use snake_case
- JS annotation objects match the sidecar JSON schema
- All file paths use `~/.snap/` as the root
- Error handling: never crash silently, always log to `~/.snap/snap.log`
- Log rotation at 1MB for both Rust and Python
- Git commits: short, direct messages

## Build

```bash
make build          # Build everything
make dev            # Run in dev mode
npx tauri build     # Rebuild Tauri app only (must use this, not cargo build alone)
```

Important: Always use `npx tauri build` or `make build`, never bare `cargo build`. The Tauri build embeds the frontend files into the binary. `cargo build` alone produces a binary with no frontend.
