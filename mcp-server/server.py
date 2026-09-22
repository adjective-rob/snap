import json
import os
import time
from pathlib import Path

from fastmcp import FastMCP
from fastmcp.tools.tool import ToolResult
from fastmcp.utilities.types import Image
from mcp.types import TextContent

mcp = FastMCP(name="snap-mcp")

INBOX = Path.home() / ".snap" / "inbox"
STATE_FILE = Path.home() / ".snap" / ".last_read"
LOG_FILE = Path.home() / ".snap" / "snap.log"


# ----- Helpers -----


def _log(msg: str):
    """Append a log line. Rotates at 1MB."""
    try:
        LOG_FILE.parent.mkdir(parents=True, exist_ok=True)
        # Rotate if over 1MB
        if LOG_FILE.exists() and LOG_FILE.stat().st_size > 1_000_000:
            backup = LOG_FILE.with_suffix(".log.old")
            LOG_FILE.rename(backup)
        with open(LOG_FILE, "a") as f:
            ts = time.strftime("%Y-%m-%d %H:%M:%S", time.gmtime())
            f.write(f"[{ts}] [mcp] {msg}\n")
    except Exception:
        pass


def _load_annotation(json_path: Path) -> dict:
    """Load a sidecar JSON and attach the image path."""
    try:
        with open(json_path) as f:
            data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        return {
            "filename": json_path.stem,
            "error": f"corrupt or unreadable: {e}",
            "image_path": None,
        }

    png_path = json_path.with_suffix(".png")
    data["image_path"] = str(png_path) if png_path.exists() else None
    data["filename"] = json_path.stem

    if not png_path.exists():
        data["warning"] = "image file missing"

    return data


def _image_block(data: dict, max_image_bytes: int):
    """Return an MCP image content block for an annotation, or None.

    The PNG is sent as a proper `image` content block rather than a base64
    string inside the JSON: clients hand image blocks to the model's vision
    input, whereas a base64 string is just tokenized as text the model cannot
    see. On failure a note is added to `data` explaining why no image is attached.
    """
    image_path = data.get("image_path")
    if not image_path:
        return None
    try:
        size = Path(image_path).stat().st_size
        if size > max_image_bytes:
            data["image_warning"] = (
                f"image not attached: {size} bytes exceeds max_image_bytes={max_image_bytes}; "
                "read it from image_path instead"
            )
            return None
        return Image(path=image_path).to_image_content()
    except OSError as e:
        data["image_warning"] = f"image not attached: {e}"
        return None


def _annotation_result(
    payload: dict, images_for: list[dict], include_image: bool, max_image_bytes: int
) -> ToolResult:
    """Build a tool result: JSON text (and structured content) plus optional image blocks.

    `images_for` lists the annotation dicts (contained in `payload`) whose PNGs
    should be attached. Image blocks are collected before the JSON is rendered
    so any `image_warning` they add is included in the text.
    """
    blocks = []
    if include_image:
        for ann in images_for:
            block = _image_block(ann, max_image_bytes)
            if block is not None:
                blocks.append(block)
    content = [TextContent(type="text", text=json.dumps(payload, indent=2))] + blocks
    return ToolResult(content=content, structured_content=payload)


def _list_annotations_sorted() -> list[Path]:
    """Return all sidecar JSON files sorted newest first."""
    if not INBOX.exists():
        return []
    return sorted(INBOX.glob("snap-*.json"), reverse=True)


def _get_last_read() -> float:
    """Return the timestamp of the last read, or 0 if never read."""
    try:
        if STATE_FILE.exists():
            return float(STATE_FILE.read_text().strip())
    except (ValueError, OSError):
        pass
    return 0.0


def _mark_read():
    """Update the last-read timestamp to now."""
    try:
        STATE_FILE.parent.mkdir(parents=True, exist_ok=True)
        STATE_FILE.write_text(str(time.time()))
    except OSError:
        pass


# ----- MCP Tools -----


@mcp.tool
def check_new_annotations() -> dict:
    """Check if there are new screen annotations since the last time Claude Code
    looked. Returns the count of new annotations and their filenames. Call this
    at the start of UI tasks to see if the user has provided visual feedback."""
    _log("check_new_annotations called")
    last_read = _get_last_read()
    files = _list_annotations_sorted()
    new_files = []
    for f in files:
        try:
            if f.stat().st_mtime > last_read:
                new_files.append(f.stem)
            else:
                break
        except OSError:
            continue

    total = len(files)
    result = {
        "new_count": len(new_files),
        "new_annotations": new_files,
        "has_new": len(new_files) > 0,
        "total_in_inbox": total,
    }

    if total > 100:
        result["note"] = f"inbox has {total} files, consider clearing old annotations"

    return result


@mcp.tool
def get_latest_annotation(
    include_image: bool = True, max_image_bytes: int = 5_000_000
) -> ToolResult:
    """Get the most recent screen annotation. Returns structured metadata
    (annotation positions in image pixels, labels, colors, source window
    context, image_path) and, by default, the annotated screenshot itself as an
    image content block. Agents that cannot receive images can read the file at
    image_path instead."""
    _log("get_latest_annotation called")
    files = _list_annotations_sorted()
    if not files:
        return ToolResult(structured_content={"error": "No annotations in inbox"})
    data = _load_annotation(files[0])
    _mark_read()
    return _annotation_result(data, [data], include_image, max_image_bytes)


@mcp.tool
def list_annotations(
    last_n: int = 5, include_image: bool = False, max_image_bytes: int = 5_000_000
) -> ToolResult:
    """List recent screen annotations. Returns metadata for the N most recent
    annotations, newest first, under the "annotations" key. Each entry includes
    the image_path of the annotated screenshot; pass include_image=true to also
    attach the screenshots as image content blocks (in the same order)."""
    _log(f"list_annotations called (last_n={last_n})")
    files = _list_annotations_sorted()[:last_n]
    results = [_load_annotation(f) for f in files]
    _mark_read()
    payload = {"count": len(results), "annotations": results}
    return _annotation_result(payload, results, include_image, max_image_bytes)


@mcp.tool
def get_annotation(
    filename: str, include_image: bool = True, max_image_bytes: int = 5_000_000
) -> ToolResult:
    """Get a specific annotation by filename (without extension), with its
    screenshot attached as an image content block by default.
    Example: get_annotation('snap-20260408-142300')"""
    _log(f"get_annotation called: {filename}")
    json_path = INBOX / f"{filename}.json"
    if not json_path.exists():
        return ToolResult(structured_content={"error": f"Annotation '{filename}' not found"})
    data = _load_annotation(json_path)
    return _annotation_result(data, [data], include_image, max_image_bytes)


@mcp.tool
def clear_inbox() -> dict:
    """Delete all processed annotations from the inbox. Use after Claude Code
    has addressed all pending annotations."""
    _log("clear_inbox called")
    files = _list_annotations_sorted()
    png_count = 0
    json_count = 0
    errors = []

    for json_path in files:
        png_path = json_path.with_suffix(".png")
        try:
            if png_path.exists():
                png_path.unlink()
                png_count += 1
            json_path.unlink()
            json_count += 1
        except OSError as e:
            errors.append(f"{json_path.name}: {e}")

    result = {"deleted_json": json_count, "deleted_png": png_count}
    if errors:
        result["errors"] = errors

    # Reset the read marker
    try:
        if STATE_FILE.exists():
            STATE_FILE.unlink()
    except OSError:
        pass

    return result


# ----- Entry point -----


def main():
    _log("snap server starting")
    mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
