# Task specs

Design notes and task specs written for coding agents. They record intent at the time of writing; the code and the top-level [README](../README.md) are the source of truth for how Snap works now.

| File | Status |
|------|--------|
| [CLAUDE-CREATES-ANNOTATIONS.md](CLAUDE-CREATES-ANNOTATIONS.md) | Not implemented. Spec for a `create_annotation` tool so an agent can draw on a screenshot for the user. Read the corrections at the top first. |
| [CROSS-PLATFORM-SUPPORT.md](CROSS-PLATFORM-SUPPORT.md) | Mostly shipped: macOS and Windows capture, window context, and tray mode, plus the macOS LaunchAgent. Windows auto-start is not in the repo. |
| [MULTI-CLIENT-SUPPORT.md](MULTI-CLIENT-SUPPORT.md) | Partly shipped: `setup-mcp.sh` registers Claude Code, Claude Desktop, Cursor, and Windsurf, and [SETUP.md](../SETUP.md) covers Cline. The other clients listed are not set up or documented. |
| [ARCHIVE/](ARCHIVE/) | The original build plan (tasks 00 to 11), all completed. Kept for history; details such as the capture order have since changed. |
