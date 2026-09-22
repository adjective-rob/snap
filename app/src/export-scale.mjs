// Pure coordinate helpers shared by the overlay (main.js) and its tests.
// No DOM or Tauri access here so the math can be unit-tested under Node.

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

// Map a crop rectangle expressed in preview-image pixels onto native-image
// pixels, rounding to whole pixels and clamping inside the native image.
export function mapCropToNative(crop, dims) {
  const {
    previewWidth,
    previewHeight,
    nativeWidth,
    nativeHeight,
  } = dims;

  if (!crop || !previewWidth || !previewHeight || !nativeWidth || !nativeHeight) {
    return null;
  }

  const sx = nativeWidth / previewWidth;
  const sy = nativeHeight / previewHeight;

  const x = clamp(Math.round(crop.x * sx), 0, nativeWidth);
  const y = clamp(Math.round(crop.y * sy), 0, nativeHeight);
  const w = clamp(Math.round(crop.w * sx), 0, nativeWidth - x);
  const h = clamp(Math.round(crop.h * sy), 0, nativeHeight - y);

  return { x, y, w, h };
}

// Fit a source region of srcW x srcH native pixels into a winW x winH
// logical-pixel window, preserving aspect ratio and centering. `dpr` converts
// native pixels to logical pixels; with allowZoom=false the region is never
// drawn larger than its logical size.
export function fitLayout({ srcW, srcH, winW, winH, dpr = 1, allowZoom = false }) {
  const logicalW = srcW / dpr;
  const logicalH = srcH / dpr;
  const maxScale = allowZoom ? Infinity : 1;
  const scale = Math.min(winW / logicalW, winH / logicalH, maxScale);
  const drawW = logicalW * scale;
  const drawH = logicalH * scale;
  return {
    offsetX: Math.round((winW - drawW) / 2),
    offsetY: Math.round((winH - drawH) / 2),
    drawW,
    drawH,
  };
}

// Given where the source region is drawn in the window (from fitLayout),
// return a transform from logical window coordinates to pixels of the
// exported image, which is the source region at native resolution.
export function imageTransform({ srcW, srcH, offsetX, offsetY, drawW, drawH }) {
  const sx = srcW / drawW;
  const sy = srcH / drawH;
  return {
    exportW: srcW,
    exportH: srcH,
    sx,
    sy,
    point: (x, y) => [Math.round((x - offsetX) * sx), Math.round((y - offsetY) * sy)],
    length: (v) => Math.round(v * sx),
  };
}

// Sidecar JSON representation of an in-memory annotation, with every
// coordinate and size converted through `t` into exported-image pixels.
export function annotationToSidecar(a, t) {
  switch (a.type) {
    case "circle":
      return {
        type: "circle",
        center: t.point(a.cx, a.cy),
        radius: [t.length(Math.abs(a.rx)), t.length(Math.abs(a.ry))],
        color: a.color,
        label: null,
      };
    case "rect": {
      // Normalize so position is the top-left corner and size is positive.
      const x0 = Math.min(a.x, a.x + a.width);
      const y0 = Math.min(a.y, a.y + a.height);
      return {
        type: "rect",
        position: t.point(x0, y0),
        size: [t.length(Math.abs(a.width)), t.length(Math.abs(a.height))],
        color: a.color,
        label: null,
      };
    }
    case "arrow":
      return {
        type: "arrow",
        from: t.point(a.fromX, a.fromY),
        to: t.point(a.toX, a.toY),
        color: a.color,
        label: null,
      };
    case "freehand":
      return {
        type: "freehand",
        points: a.points.map((p) => {
          const [x, y] = t.point(p.x, p.y);
          return { x, y };
        }),
        color: a.color,
        label: null,
      };
    case "text":
      return {
        type: "text",
        position: t.point(a.x, a.y),
        content: a.content,
        font_size: t.length(a.fontSize || 16),
        color: a.color,
      };
    case "marker":
      return {
        type: "marker",
        position: t.point(a.x, a.y),
        number: a.number,
        radius: t.length(a.radius || 16),
        color: a.color,
      };
    default:
      return a;
  }
}
