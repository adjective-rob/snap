# Multi-Client Compatibility: Every Major AI Coding Tool

## Context

Snap's MCP server uses stdio transport via `fastmcp`. This is the most universally supported transport across MCP clients. However, each client has its own configuration format, quirks, and limitations. This task ensures Snap works out of the box with every major AI coding tool, and that the README documents setup for each.

The Snap MCP server does not need code changes for most clients. The work here is configuration, testing, and documentation.

---

## Supported Clients and Setup

### 1. Claude Code (CLI)

**Status:** Primary target. Already works.

```bash
claude mcp add --transport stdio snap -- python /path/to/snap/mcp-server/server.py
```

Verify: `claude mcp list` shows `snap` connected.

No issues expected.

---

### 2. Claude Desktop

**Status:** Should work. Needs config file entry.

Edit `~/.config/Claude/claude_desktop_config.json` (Linux) or `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "snap": {
      "command": "python",
      "args": ["/absolute/path/to/snap/mcp-server/server.py"],
      "env": {}
    }
  }
}
```

Restart Claude Desktop after adding.

**Potential issue:** Claude Desktop may not support image reading from file paths returned by tools. The tool returns a path string, and Claude Desktop's model reads it. If the model can't access local files in Claude Desktop, the user would need to manually paste the screenshot. Document this limitation if it exists.

---

### 3. Cursor

**Status:** Has native MCP support. Stdio works.

Create or edit `.cursor/mcp.json` in the project root (project-scoped) or configure globally via Cursor Settings > Tools & MCP:

```json
{
  "mcpServers": {
    "snap": {
      "command": "python",
      "args": ["/absolute/path/to/snap/mcp-server/server.py"]
    }
  }
}
```

**Notes:**
- Cursor uses its own config format, not the Claude Desktop format
- Cursor supports stdio transport natively
- After adding, the server should appear in the MCP panel. If it shows "Not connected", check that the Python path is correct and `fastmcp` is installed
- Cursor's agent mode must be active for MCP tools to be invoked

---

### 4. Windsurf

**Status:** Should work. Similar config to Cursor.

Add to Windsurf's MCP configuration (Settings > MCP or the equivalent config file):

```json
{
  "mcpServers": {
    "snap": {
      "command": "python",
      "args": ["/absolute/path/to/snap/mcp-server/server.py"]
    }
  }
}
```

**Notes:**
- Windsurf supports stdio MCP servers
- Some Windsurf versions require hardcoded paths (no environment variable expansion). Use absolute paths.
- Test after configuration to confirm tools are listed

---

### 5. VS Code + GitHub Copilot

**Status:** Supported since VS Code 1.101+. Needs `.vscode/mcp.json`.

Create `.vscode/mcp.json` in the project root:

```json
{
  "mcp": {
    "servers": {
      "snap": {
        "command": "python",
        "args": ["/absolute/path/to/snap/mcp-server/server.py"]
      }
    }
  }
}
```

**Notes:**
- Copilot must be in Agent Mode for MCP tools to be used
- The config format wraps servers inside an `mcp` key (different from Cursor/Windsurf)
- VS Code may prompt to "Start" the server after configuration

---

### 6. Cline (VS Code Extension)

**Status:** Should work. Cline has its own MCP config.

Cline settings > MCP Servers > Add:

```json
{
  "snap": {
    "command": "python",
    "args": ["/absolute/path/to/snap/mcp-server/server.py"]
  }
}
```

**Notes:**
- Cline's MCP implementation may lag behind the latest spec. If tool listing works but tool invocation fails, check Cline's version.
- Cline supports stdio natively

---

### 7. Roo Code

**Status:** Should work. Fork of Cline with similar MCP support.

Same configuration as Cline. Check Roo Code's settings for MCP server configuration.

---

### 8. ChatGPT (Desktop / Connectors)

**Status:** NOT compatible with stdio. ChatGPT requires remote MCP servers with OAuth 2.1.

Snap's MCP server is a local stdio server. ChatGPT cannot connect to it directly.

**Workaround:** If ChatGPT support is needed in the future, expose the MCP server over HTTP using `fastmcp`'s HTTP transport mode, add OAuth, and host it. This is out of scope for v1.

Document this limitation clearly.

---

### 9. JetBrains IDEs (IntelliJ, WebStorm, PyCharm)

**Status:** JetBrains 2025.2+ has MCP client support.

Settings > Tools > MCP Server > Add External Server:

```json
{
  "snap": {
    "command": "python",
    "args": ["/absolute/path/to/snap/mcp-server/server.py"],
    "transportType": "stdio"
  }
}
```

**Notes:**
- JetBrains auto-configuration feature can detect MCP servers if they're in the project
- Verify the MCP client is enabled in the IDE settings

---

### 10. Amazon Q Developer / CodeWhisperer

**Status:** Unknown MCP support status. If it supports stdio MCP servers, same config pattern applies. Skip for v1 and add when confirmed.

---

### 11. Aider (Terminal)

**Status:** Aider does not currently support MCP. It uses its own tool/function calling mechanism. Not compatible.

---

## Implementation Tasks

### Task 13a: Generate Per-Client Config Examples

Create a `configs/` directory in the repo with ready-to-copy config snippets for each supported client:

```
configs/
  claude-code.sh           # Shell command to register
  claude-desktop.json      # Config snippet
  cursor.json              # .cursor/mcp.json content
  windsurf.json            # Windsurf config
  vscode-copilot.json      # .vscode/mcp.json content
  cline.json               # Cline config
  jetbrains.json           # JetBrains config
```

Each file should have a comment at the top explaining where to place it and how to activate it.

Include a `__SNAP_SERVER_PATH__` placeholder that the user replaces with their actual path. Consider adding a setup script that auto-detects the path and generates configs:

```bash
# configs/setup.sh
SNAP_PATH="$(cd "$(dirname "$0")/.." && pwd)/mcp-server/server.py"
echo "Detected Snap server at: $SNAP_PATH"
echo ""
echo "Claude Code:"
echo "  claude mcp add --transport stdio snap -- python $SNAP_PATH"
echo ""
echo "Cursor (.cursor/mcp.json):"
cat <<EOF
{
  "mcpServers": {
    "snap": {
      "command": "python",
      "args": ["$SNAP_PATH"]
    }
  }
}
EOF
# ... etc for each client
```

### Task 13b: Update README with Multi-Client Setup

Add a "Setup" section to README.md with a table:

```markdown
## Connect to Your AI Tool

| Tool | Transport | Config |
|------|-----------|--------|
| Claude Code | stdio | `claude mcp add --transport stdio snap -- python ./mcp-server/server.py` |
| Claude Desktop | stdio | Edit `claude_desktop_config.json` ([example](configs/claude-desktop.json)) |
| Cursor | stdio | Add `.cursor/mcp.json` ([example](configs/cursor.json)) |
| Windsurf | stdio | MCP settings ([example](configs/windsurf.json)) |
| VS Code + Copilot | stdio | Add `.vscode/mcp.json` ([example](configs/vscode-copilot.json)) |
| Cline | stdio | MCP settings ([example](configs/cline.json)) |
| JetBrains | stdio | Settings > Tools > MCP ([example](configs/jetbrains.json)) |
| ChatGPT | -- | Not supported (requires remote OAuth server) |
| Aider | -- | Not supported (no MCP) |

Run `bash configs/setup.sh` to auto-generate configs with your local path.
```

### Task 13c: Test Tool Discovery Across Clients

For each supported client, verify:

1. The MCP server starts and connects without errors
2. All 5 tools are listed (check_new_annotations, get_latest_annotation, list_annotations, get_annotation, clear_inbox)
3. Tool descriptions render correctly in the client's tool panel
4. Calling `check_new_annotations()` returns a valid response
5. Calling `get_latest_annotation()` with a test file in the inbox returns metadata and a path

Create a test script that populates the inbox with a test annotation for verification:

```bash
# configs/test-annotation.sh
mkdir -p ~/.snap/inbox
TIMESTAMP=$(date -u +"%Y%m%d-%H%M%S")
echo "{\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"source\":{\"window_title\":\"Test Window\",\"url\":null,\"window_class\":\"test\",\"pid\":null,\"display\":\"primary\",\"resolution\":[1920,1080]},\"annotations\":[{\"type\":\"circle\",\"center\":[400,300],\"radius\":[50,50],\"color\":\"#FF3B30\",\"label\":\"test annotation\"}],\"image_filename\":\"snap-${TIMESTAMP}.png\"}" > ~/.snap/inbox/snap-${TIMESTAMP}.json
touch ~/.snap/inbox/snap-${TIMESTAMP}.png
echo "Test annotation created: snap-${TIMESTAMP}"
echo "Now ask your AI tool to check for new Snap annotations."
```

### Task 13d: Tool Description Optimization

Different LLMs interpret tool descriptions differently. The current tool descriptions are written for Claude. Review and optimize them to be clear and actionable for any LLM:

- Keep descriptions under 200 characters for the summary line (some clients truncate)
- Include the word "screenshot" or "screen" in key tool descriptions since other LLMs may not infer "annotation" means visual
- Make the return schema obvious: mention that `image_path` is a local file path to a PNG
- Add parameter descriptions to `list_annotations(last_n)` and `get_annotation(filename)` if not already present

Example optimization:

```python
@mcp.tool
def get_latest_annotation() -> dict:
    """Get the most recent annotated screenshot from the user's Snap inbox.
    Returns a JSON object with: image_path (local PNG file path to view),
    annotations (list of circles, arrows, text labels with positions and colors),
    and source (window title and app that was captured).
    The image shows the user's screen with visual markup indicating what needs attention."""
```

The key insight: Claude Code knows what "annotation" means in this context because you can put it in CLAUDE.md. GPT, Gemini, and other models inside Cursor/Windsurf/Copilot have no such context. The tool description IS the only context they get. Make it self-explanatory.

---

## Verification

- [ ] `configs/` directory exists with snippets for 7 clients
- [ ] `configs/setup.sh` runs and outputs correct configs with detected path
- [ ] `configs/test-annotation.sh` creates a valid test annotation
- [ ] README has multi-client setup table
- [ ] Tool descriptions work well with Claude, GPT-4, and Gemini (test in at least 2 different clients)
- [ ] ChatGPT and Aider are documented as unsupported with clear reasons

## Do Not Touch

- Do not modify the MCP server's transport (keep stdio)
- Do not add authentication to the local server (unnecessary for stdio)
- Do not change tool function signatures