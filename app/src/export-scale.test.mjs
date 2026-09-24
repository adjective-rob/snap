import test from "node:test";
import assert from "node:assert/strict";
import {
  mapCropToNative,
  fitLayout,
  cropScreenRect,
  imageTransform,
  annotationToSidecar,
} from "./export-scale.mjs";

test("mapCropToNative scales crop from preview to native capture pixels", () => {
  const crop = { x: 100, y: 50, w: 400, h: 200 };
  const mapped = mapCropToNative(crop, {
    previewWidth: 1920,
    previewHeight: 1080,
    nativeWidth: 3840,
    nativeHeight: 2160,
  });

  assert.deepEqual(mapped, { x: 200, y: 100, w: 800, h: 400 });
});

test("mapCropToNative clamps crop to native bounds", () => {
  const crop = { x: -10, y: 1000, w: 2000, h: 300 };
  const mapped = mapCropToNative(crop, {
    previewWidth: 1000,
    previewHeight: 1000,
    nativeWidth: 2000,
    nativeHeight: 2000,
  });

  assert.deepEqual(mapped, { x: 0, y: 2000, w: 2000, h: 0 });
});

test("fitLayout: full capture matching the window fills it exactly", () => {
  const l = fitLayout({ srcW: 1920, srcH: 1080, winW: 1920, winH: 1080 });
  assert.deepEqual(l, { offsetX: 0, offsetY: 0, drawW: 1920, drawH: 1080 });
});

test("fitLayout: HiDPI full capture is shown at logical size", () => {
  const l = fitLayout({ srcW: 3840, srcH: 2160, winW: 1920, winH: 1080, dpr: 2 });
  assert.deepEqual(l, { offsetX: 0, offsetY: 0, drawW: 1920, drawH: 1080 });
});

test("fitLayout: wide crop is letterboxed, not stretched", () => {
  // A 4:1 region in a 16:9 window must keep its aspect ratio.
  const l = fitLayout({ srcW: 800, srcH: 200, winW: 1920, winH: 1080, allowZoom: true });
  assert.equal(l.drawW, 1920);
  assert.equal(l.drawH, 480);
  assert.equal(l.offsetX, 0);
  assert.equal(l.offsetY, 300);
});

test("fitLayout: small region without zoom is centered at 1:1 logical", () => {
  // macOS interactive capture of a 400x300 logical region on a retina display.
  const l = fitLayout({ srcW: 800, srcH: 600, winW: 1440, winH: 900, dpr: 2 });
  assert.deepEqual(l, { offsetX: 520, offsetY: 300, drawW: 400, drawH: 300 });
});

test("fitLayout: capture wider than the window is scaled down", () => {
  // Two side-by-side 1080p monitors captured as one image, shown on one.
  const l = fitLayout({ srcW: 3840, srcH: 1080, winW: 1920, winH: 1080 });
  assert.deepEqual(l, { offsetX: 0, offsetY: 270, drawW: 1920, drawH: 540 });
});

test("imageTransform maps window corners of the drawn image to image corners", () => {
  const src = { srcW: 800, srcH: 600 };
  const l = fitLayout({ ...src, winW: 1440, winH: 900, dpr: 2 });
  const t = imageTransform({ ...src, ...l });

  assert.equal(t.exportW, 800);
  assert.equal(t.exportH, 600);
  assert.deepEqual(t.point(l.offsetX, l.offsetY), [0, 0]);
  assert.deepEqual(t.point(l.offsetX + l.drawW, l.offsetY + l.drawH), [800, 600]);
  // Scale is uniform because aspect ratio is preserved.
  assert.equal(t.sx, t.sy);
  assert.equal(t.length(10), 20);
});

test("imageTransform: letterboxed crop maps into crop pixels, not window pixels", () => {
  const src = { srcW: 800, srcH: 200 };
  const l = fitLayout({ ...src, winW: 1920, winH: 1080, allowZoom: true });
  const t = imageTransform({ ...src, ...l });

  // Centre of the window is the centre of the crop.
  assert.deepEqual(t.point(960, 540), [400, 100]);
  // A point in the letterbox above the image lands at a negative y.
  assert.deepEqual(t.point(960, 0), [400, -125]);
});

test("annotationToSidecar converts every annotation type into image pixels", () => {
  const src = { srcW: 3840, srcH: 2160 };
  const l = fitLayout({ ...src, winW: 1920, winH: 1080, dpr: 2 });
  const t = imageTransform({ ...src, ...l });

  assert.deepEqual(
    annotationToSidecar({ type: "circle", cx: 100, cy: 50, rx: 20, ry: -10, color: "#f00" }, t),
    { type: "circle", center: [200, 100], radius: [40, 20], color: "#f00", label: null },
  );
  assert.deepEqual(
    annotationToSidecar({ type: "rect", x: 100, y: 100, width: -50, height: -20, color: "#f00" }, t),
    { type: "rect", position: [100, 160], size: [100, 40], color: "#f00", label: null },
  );
  assert.deepEqual(
    annotationToSidecar({ type: "arrow", fromX: 1, fromY: 2, toX: 3, toY: 4, color: "#f00" }, t),
    { type: "arrow", from: [2, 4], to: [6, 8], color: "#f00", label: null },
  );
  assert.deepEqual(
    annotationToSidecar({ type: "freehand", points: [{ x: 1, y: 1 }, { x: 2, y: 3 }], color: "#f00" }, t),
    { type: "freehand", points: [{ x: 2, y: 2 }, { x: 4, y: 6 }], color: "#f00", label: null },
  );
  assert.deepEqual(
    annotationToSidecar({ type: "text", x: 10, y: 20, content: "hi", fontSize: 16, color: "#f00" }, t),
    { type: "text", position: [20, 40], content: "hi", font_size: 32, color: "#f00" },
  );
  assert.deepEqual(
    annotationToSidecar({ type: "marker", x: 10, y: 20, number: 3, radius: 16, color: "#f00" }, t),
    { type: "marker", position: [20, 40], number: 3, radius: 32, color: "#f00" },
  );
});

test("cropScreenRect: a region stays where it was on screen, at the same scale", () => {
  // 4K capture on a 2x display: the capture is drawn 1:1 at logical size.
  const src = { srcW: 3840, srcH: 2160 };
  const full = fitLayout({ ...src, winW: 1920, winH: 1080, dpr: 2 });
  const crop = { x: 400, y: 200, w: 800, h: 600 };
  const r = cropScreenRect(full, crop, src.srcW, src.srcH);
  assert.deepEqual(r, { offsetX: 200, offsetY: 100, drawW: 400, drawH: 300 });

  // And the export transform of that region maps its screen corner to (0,0)
  // and keeps the capture's native pixel density.
  const t = imageTransform({ srcW: crop.w, srcH: crop.h, ...r });
  assert.deepEqual(t.point(200, 100), [0, 0]);
  assert.deepEqual(t.point(600, 400), [800, 600]);
  assert.equal(t.length(10), 20);
});

test("cropScreenRect: letterboxed capture keeps the letterbox offset", () => {
  const src = { srcW: 3840, srcH: 1080 };
  const full = fitLayout({ ...src, winW: 1920, winH: 1080 });
  const r = cropScreenRect(full, { x: 0, y: 0, w: 3840, h: 1080 }, src.srcW, src.srcH);
  assert.deepEqual(r, full);
});
