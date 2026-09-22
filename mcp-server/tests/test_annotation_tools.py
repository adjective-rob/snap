"""Tool-level tests for the annotation readers, run through an in-memory MCP client
so what is asserted is exactly what a real client receives."""

import asyncio
import base64
import importlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

from fastmcp import Client


class AnnotationToolTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.inbox = self.home / ".snap" / "inbox"
        self.inbox.mkdir(parents=True, exist_ok=True)

        self.png_bytes = b"\x89PNG\r\n\x1a\nnot-a-real-png"
        self.older = self._write("snap-20990101-000000-000", self.png_bytes)
        self.newer = self._write("snap-20990102-000000-000", self.png_bytes)

        self.old_home = os.environ.get("HOME")
        os.environ["HOME"] = str(self.home)

        if "server" in sys.modules:
            del sys.modules["server"]
        self.server = importlib.import_module("server")

    def tearDown(self):
        if self.old_home is None:
            os.environ.pop("HOME", None)
        else:
            os.environ["HOME"] = self.old_home

    def _write(self, stem: str, png: bytes | None) -> Path:
        base = self.inbox / stem
        if png is not None:
            base.with_suffix(".png").write_bytes(png)
        base.with_suffix(".json").write_text(json.dumps({"annotations": [], "stem": stem}))
        return base

    def _call(self, name: str, **args):
        async def run():
            async with Client(self.server.mcp) as client:
                return await client.call_tool(name, args)

        return asyncio.run(run())

    @staticmethod
    def _images(result):
        return [b for b in result.content if b.type == "image"]

    @staticmethod
    def _text_json(result):
        return json.loads(next(b.text for b in result.content if b.type == "text"))

    def test_latest_returns_image_block_and_metadata(self):
        result = self._call("get_latest_annotation")

        data = self._text_json(result)
        self.assertEqual(data["filename"], self.newer.name)
        self.assertEqual(data["image_path"], str(self.newer.with_suffix(".png")))
        self.assertEqual(result.structured_content["filename"], self.newer.name)

        images = self._images(result)
        self.assertEqual(len(images), 1)
        self.assertEqual(images[0].mimeType, "image/png")
        self.assertEqual(base64.b64decode(images[0].data), self.png_bytes)
        # The PNG must not also be inlined as text in the JSON.
        self.assertNotIn("image_base64", data)

    def test_latest_without_image(self):
        result = self._call("get_latest_annotation", include_image=False)
        self.assertEqual(self._images(result), [])
        self.assertEqual(self._text_json(result)["filename"], self.newer.name)

    def test_latest_skips_oversized_image_with_warning(self):
        result = self._call("get_latest_annotation", max_image_bytes=4)
        self.assertEqual(self._images(result), [])
        self.assertIn("image_warning", self._text_json(result))

    def test_latest_with_missing_png_warns_and_has_no_image(self):
        self._write("snap-20990103-000000-000", None)
        result = self._call("get_latest_annotation")
        data = self._text_json(result)
        self.assertIsNone(data["image_path"])
        self.assertEqual(data["warning"], "image file missing")
        self.assertEqual(self._images(result), [])

    def test_latest_on_empty_inbox_reports_error(self):
        for p in self.inbox.iterdir():
            p.unlink()
        result = self._call("get_latest_annotation")
        self.assertEqual(result.structured_content, {"error": "No annotations in inbox"})

    def test_list_defaults_to_metadata_only(self):
        result = self._call("list_annotations", last_n=5)
        data = self._text_json(result)
        self.assertEqual(data["count"], 2)
        self.assertEqual(
            [a["filename"] for a in data["annotations"]], [self.newer.name, self.older.name]
        )
        self.assertEqual(self._images(result), [])

    def test_list_can_attach_images_in_order(self):
        result = self._call("list_annotations", last_n=5, include_image=True)
        self.assertEqual(len(self._images(result)), 2)

    def test_get_annotation_by_name(self):
        result = self._call("get_annotation", filename=self.older.name)
        self.assertEqual(self._text_json(result)["filename"], self.older.name)
        self.assertEqual(len(self._images(result)), 1)

    def test_get_annotation_missing(self):
        result = self._call("get_annotation", filename="snap-nope")
        self.assertIn("error", result.structured_content)


if __name__ == "__main__":
    unittest.main()
