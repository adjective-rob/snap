# Task: Add `create_annotation` Tool to Snap MCP Server

## Corrections to this spec (review, 2026-09-22)

Apply these on top of the code below when implementing:

1. **Capture tool order.** `_capture_screen` below only tries `grim` (Wayland) or `scrot` (X11). `grim` needs the wlr-screencopy protocol, which GNOME's Mutter does not implement, so it fails on GNOME Wayland even when installed. Mirror the order the Rust side already uses in `capture_screen()` in `app/src-tauri/src/lib.rs`: on Wayland try `gnome-screenshot --file`, then `grim`, then `scrot`; on X11 try `scrot`, then `gnome-screenshot`. Try each in turn and use the first that produces a non-empty file. macOS: `screencapture -x`.
2. **Filter agent-authored files out of "new".** `check_new_annotations` reports every sidecar newer than the last read, so the agent's own output would show up as new user feedback. Skip sidecars whose `source.author == "claude-code"` in `check_new_annotations` (and default `list_annotations` to the same filter, with an `include_agent_created` flag).
3. **Sidecar shape and the read-only rule.** Write the same sidecar shape the app writes (`image_size`, `coordinate_space: "image_pixels"`, annotation coordinates in PNG pixels) so the readers stay uniform, and update the "MCP server is read-only" line in `CLAUDE.md` to say the server writes only agent-authored annotations tagged `source.author`.

## Why

Snap currently works in one direction: user annotates screenshots for Claude Code. This task adds the reverse channel. Claude Code captures a screenshot, draws circles/arrows/labels on it programmatically, and saves the result to the Snap inbox. The user opens the annotated image to see exactly what Claude Code is pointing at.

Use case: Claude Code finds a layout bug at specific coordinates, draws a red circle around it with a label saying "overflow here", and tells the user to check their Snap inbox. No more "the element at approximately 200px from the top" in plain text.

## Dependencies

Add Pillow to the project. It handles image loading and drawing.

```bash
pip install Pillow
```

If the project has a `requirements.txt` or `pyproject.toml`, add `Pillow>=10.0.0` there too.

## What to Add

All changes go in `server.py`. Two things: a helper function `_draw_annotations` and a new MCP tool `create_annotation`.

### 1. Add imports at the top of server.py

```python
from PIL import Image, ImageDraw, ImageFont
import subprocess
```

### 2. Add the drawing helper after the existing helpers

```python
def _draw_annotations(image_path: str, annotations: list[dict], output_path: Path):
    """Composite annotations onto an image and save the result."""
    img = Image.open(image_path).convert("RGBA")
    overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)

    # Try to load a readable font, fall back to default
    font = None
    font_large = None
    for font_path in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ]:
        if Path(font_path).exists():
            font = ImageFont.truetype(font_path, 16)
            font_large = ImageFont.truetype(font_path, 24)
            break
    if font is None:
        font = ImageFont.load_default()
        font_large = font

    for ann in annotations:
        a_type = ann.get("type", "circle")
        color = ann.get("color", "#FF0000")
        label = ann.get("label", "")
        x = ann.get("x", 0)
        y = ann.get("y", 0)
        width = ann.get("width", 100)
        height = ann.get("height", 100)
        line_width = ann.get("line_width", 3)

        if a_type == "circle":
            bbox = [x - width // 2, y - height // 2, x + width // 2, y + height // 2]
            draw.ellipse(bbox, outline=color, width=line_width)

        elif a_type == "rect":
            bbox = [x, y, x + width, y + height]
            draw.rectangle(bbox, outline=color, width=line_width)

        elif a_type == "arrow":
            x2 = ann.get("x2", x + 100)
            y2 = ann.get("y2", y)
            draw.line([(x, y), (x2, y2)], fill=color, width=line_width)
            # Arrowhead
            import math
            angle = math.atan2(y2 - y, x2 - x)
            head_len = 15
            for offset in [2.5, -2.5]:
                hx = x2 - head_len * math.cos(angle + offset)
                hy = y2 - head_len * math.sin(angle + offset)
                draw.line([(x2, y2), (int(hx), int(hy))], fill=color, width=line_width)

        elif a_type == "text":
            draw.text((x, y), label, fill=color, font=font_large)
            label = ""  # already drawn, skip the label below

        if label and a_type != "text":
            label_y = y - height // 2 - 22 if a_type == "circle" else y - 22
            draw.text((x, max(0, label_y)), label, fill=color, font=font)

    composited = Image.alpha_composite(img, overlay).convert("RGB")
    composited.save(output_path, "PNG")
```

### 3. Add the `capture_screen` helper

Claude Code needs to take a screenshot before annotating. This uses `scrot` (X11) or `grim` (Wayland).

```python
def _capture_screen() -> Path:
    """Take a screenshot of the current screen. Returns the path to the PNG."""
    timestamp = time.strftime("%Y%m%d-%H%M%S", time.localtime())
    capture_path = INBOX / f"capture-{timestamp}.png"
    INBOX.mkdir(parents=True, exist_ok=True)

    wayland = os.environ.get("WAYLAND_DISPLAY")
    try:
        if wayland:
            subprocess.run(["grim", str(capture_path)], check=True, capture_output=True)
        else:
            subprocess.run(["scrot", str(capture_path)], check=True, capture_output=True)
    except FileNotFoundError:
        tool = "grim" if wayland else "scrot"
        raise RuntimeError(f"{tool} not installed. Run: sudo apt install {tool}")
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Screen capture failed: {e.stderr.decode()}")

    return capture_path
```

### 4. Add the MCP tools

```python
@mcp.tool
def capture_screen() -> dict:
    """Take a screenshot of the current screen. Returns the path to the
    captured PNG. Use this before create_annotation to get a fresh
    screenshot to annotate."""
    _log("capture_screen called")
    try:
        path = _capture_screen()
        return {"image_path": str(path), "status": "captured"}
    except RuntimeError as e:
        return {"error": str(e)}


@mcp.tool
def create_annotation(
    image_path: str,
    annotations: list[dict],
    message: str = "",
) -> dict:
    """Create an annotated image for the user to view. Claude Code calls this
    to visually communicate layout issues, point at specific elements, or
    highlight areas of interest.

    Args:
        image_path: Absolute path to the source image (from capture_screen
                    or any PNG/JPG on disk).
        annotations: List of annotation dicts. Each has:
            - type: "circle" | "arrow" | "rect" | "text"
            - x, y: Position (center for circle, top-left for rect/text,
                     start for arrow)
            - width, height: Size (for circle and rect)
            - x2, y2: End point (for arrow)
            - color: Hex color string, default "#FF0000"
            - label: Text label to display near the annotation
            - line_width: Stroke width, default 3
        message: Optional text note saved in the sidecar JSON.

    Returns:
        Dict with image_path and json_path of the saved annotation.
    """
    _log(f"create_annotation called: {image_path}, {len(annotations)} annotations")

    if not Path(image_path).exists():
        return {"error": f"Source image not found: {image_path}"}

    timestamp = time.strftime("%Y%m%d-%H%M%S", time.localtime())
    filename = f"snap-{timestamp}"
    output_png = INBOX / f"{filename}.png"
    output_json = INBOX / f"{filename}.json"

    INBOX.mkdir(parents=True, exist_ok=True)

    try:
        _draw_annotations(image_path, annotations, output_png)
    except Exception as e:
        return {"error": f"Drawing failed: {e}"}

    sidecar = {
        "source": {
            "image_path": image_path,
            "author": "claude-code",
        },
        "annotations": annotations,
        "message": message,
        "timestamp": timestamp,
    }

    with open(output_json, "w") as f:
        json.dump(sidecar, f, indent=2)

    return {
        "image_path": str(output_png),
        "json_path": str(output_json),
        "filename": filename,
        "annotation_count": len(annotations),
        "message": message,
    }
```

## Verification

1. Restart Claude Code (or run `/mcp` to reconnect)
2. In Claude Code, test the full loop:

```
Use the snap tools to capture my screen, then circle the top-left corner with a red circle labeled "test"
```

Expected: Claude Code calls `capture_screen`, gets a path, then calls `create_annotation` with that path and one circle annotation. Check `~/.snap/inbox/` for the resulting PNG and JSON.

3. Open the PNG. You should see your desktop screenshot with a red circle and "test" label in the top-left area.

4. Verify `get_latest_annotation` returns the new annotation (confirms the inbox format is compatible).

## Do Not Touch

- Existing tools (`check_new_annotations`, `get_latest_annotation`, `list_annotations`, `get_annotation`, `clear_inbox`)
- The `INBOX` path or sidecar JSON schema (new annotations follow the same convention)
- The `main()` entry point

## Notes

- The `math` import inside the arrow drawing is intentional. It keeps it scoped to where it's used. Move it to the top-level imports if you prefer.
- The `author` field in the sidecar JSON is set to `"claude-code"` so you can distinguish agent-created annotations from user-created ones.
- If `scrot` or `grim` isn't installed, the tool returns a clear error message instead of crashing.