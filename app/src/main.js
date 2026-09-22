const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;
const { listen } = window.__TAURI__.event;
import {
  mapCropToNative,
  fitLayout,
  imageTransform,
  annotationToSidecar,
} from "./export-scale.mjs";

// ----- State -----
let currentTool = "circle";
let currentColor = "#FF3B30";
let currentWidth = 4;
let markerCounter = 1;
let dimActive = false;
let isDrawing = false;
let drawStart = null;
let freehandPoints = [];
let annotations = [];
let backgroundImage = null;
let bgOffsetX = 0,
  bgOffsetY = 0,
  bgDrawW = 0,
  bgDrawH = 0;
let selectionPhase = false;
let selStart = null,
  selCurrent = null;
let cropSrc = null;
let windowContext = null;
let overlayActive = false;
let isSaving = false;

const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");
const textInput = document.getElementById("text-input");
const toolbar = document.getElementById("toolbar");

// ----- Canvas setup -----
// On HiDPI displays, the logical size (window.innerWidth) differs from physical pixels.
// We size the canvas to physical pixels for crisp rendering, then scale the context
// so all drawing coordinates use logical pixels.
const dpr = window.devicePixelRatio || 1;

// Fit the visible source region (the whole capture, or the cropped region)
// into the window, preserving aspect ratio and centering. bgOffset/bgDraw
// describe where that region lands in logical window pixels, and every
// window->image coordinate conversion goes through them (see imageTransform).
function sourceSize() {
  return {
    srcW: cropSrc ? cropSrc.w : backgroundImage.naturalWidth,
    srcH: cropSrc ? cropSrc.h : backgroundImage.naturalHeight,
  };
}

function computeBgLayout() {
  if (!backgroundImage) return;
  const layout = fitLayout({
    ...sourceSize(),
    winW: window.innerWidth,
    winH: window.innerHeight,
    dpr,
    // A full capture is never enlarged; a cropped region is zoomed to fill.
    allowZoom: Boolean(cropSrc),
  });
  bgOffsetX = layout.offsetX;
  bgOffsetY = layout.offsetY;
  bgDrawW = layout.drawW;
  bgDrawH = layout.drawH;
}

// Mapping from logical window coordinates to pixels of the exported image
// (the source region at native capture resolution) for the current layout.
function currentImageTransform() {
  return imageTransform({
    ...sourceSize(),
    offsetX: bgOffsetX,
    offsetY: bgOffsetY,
    drawW: bgDrawW,
    drawH: bgDrawH,
  });
}

function resizeCanvas() {
  const logicalW = window.innerWidth;
  const logicalH = window.innerHeight;
  canvas.width = logicalW * dpr;
  canvas.height = logicalH * dpr;
  canvas.style.width = logicalW + "px";
  canvas.style.height = logicalH + "px";
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  computeBgLayout();
  render();
}
window.addEventListener("resize", resizeCanvas);

// ----- Init -----
async function init() {
  resizeCanvas();

  await startCaptureSession();

  await listen("snap://start", async () => {
    await startCaptureSession();
  });
}

function resetSessionState() {
  currentTool = "circle";
  currentColor = "#FF3B30";
  currentWidth = 4;
  markerCounter = 1;
  dimActive = false;
  isDrawing = false;
  drawStart = null;
  freehandPoints = [];
  annotations = [];
  selectionPhase = false;
  selStart = null;
  selCurrent = null;
  cropSrc = null;
  overlayActive = false;
  isSaving = false;

  textInput.style.display = "none";
  textInput.value = "";

  document.querySelectorAll(".tool-btn").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.tool === "circle");
  });
  document.querySelectorAll(".color-btn").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.color === "#FF3B30");
  });
  document.querySelectorAll(".width-btn").forEach((btn, i) => {
    btn.classList.toggle("active", i === 1);
  });
  document.getElementById("btn-dim").classList.remove("active");
  toolbar.classList.remove("visible");
}

async function startCaptureSession() {
  resetSessionState();

  try {
    // Capture window context before anything else
    windowContext = await invoke("get_active_window_context");
  } catch (e) {
    console.warn("Could not capture window context:", e);
    windowContext = {
      window_title: null,
      url: null,
      window_class: null,
      pid: null,
    };
  }

  try {
    // Capture screen BEFORE showing window
    await invoke("capture_screen");
    const captureBase64 = await invoke("read_capture_base64");

    // Show window and force fullscreen
    const win = getCurrentWindow();
    await win.show();
    await win.setFullscreen(true);
    await win.setFocus();

    // Wait a tick for the window to resize, then set up canvas
    await new Promise((r) => setTimeout(r, 100));
    resizeCanvas();

    await loadBackgroundImage(captureBase64);
  } catch (e) {
    console.error("Screen capture failed:", e);
    await getCurrentWindow().show();
    showError("Screen capture failed: " + e);
    setTimeout(() => closeOverlay(), 3000);
    return;
  }

  overlayActive = true;

  if (navigator.userAgent.includes("Mac")) {
    selectionPhase = false;
    canvas.style.cursor = "default";
    toolbar.classList.add("visible");
  } else {
    selectionPhase = true;
    canvas.style.cursor = "crosshair";
  }
}

function showError(msg) {
  const w = window.innerWidth;
  const h = window.innerHeight;
  ctx.fillStyle = "rgba(0,0,0,0.85)";
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = "#FF3B30";
  ctx.font =
    "bold 20px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(msg, w / 2, h / 2);
  ctx.font = "14px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
  ctx.fillStyle = "rgba(255,255,255,0.5)";
  ctx.fillText("Closing in 3 seconds...", w / 2, h / 2 + 30);
}

function loadBackgroundImage(base64Data) {
  return new Promise((resolve, reject) => {
    backgroundImage = null;
    render();

    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      backgroundImage = img;
      computeBgLayout();
      render();
      resolve();
    };
    img.onerror = reject;
    img.src = "data:image/png;base64," + base64Data;
  });
}

// ----- Render -----
function render() {
  const logicalW = window.innerWidth;
  const logicalH = window.innerHeight;
  ctx.clearRect(0, 0, logicalW, logicalH);

  if (backgroundImage) {
    if (selectionPhase) {
      ctx.save();
      ctx.globalAlpha = 0.45;
      ctx.drawImage(backgroundImage, bgOffsetX, bgOffsetY, bgDrawW, bgDrawH);
      ctx.restore();

      if (selStart && selCurrent) {
        const r = selNormalized();
        if (r.w > 1 && r.h > 1) {
          const sx =
            ((r.x - bgOffsetX) / bgDrawW) * backgroundImage.naturalWidth;
          const sy =
            ((r.y - bgOffsetY) / bgDrawH) * backgroundImage.naturalHeight;
          const sw = (r.w / bgDrawW) * backgroundImage.naturalWidth;
          const sh = (r.h / bgDrawH) * backgroundImage.naturalHeight;
          ctx.drawImage(backgroundImage, sx, sy, sw, sh, r.x, r.y, r.w, r.h);
          ctx.strokeStyle = "white";
          ctx.lineWidth = 1.5;
          ctx.setLineDash([6, 3]);
          ctx.strokeRect(r.x, r.y, r.w, r.h);
          ctx.setLineDash([]);
          ctx.fillStyle = "rgba(0,0,0,0.65)";
          ctx.fillRect(r.x, r.y - 22, 74, 18);
          ctx.fillStyle = "white";
          ctx.font = "11px -apple-system, sans-serif";
          ctx.fillText(
            `${Math.round(r.w)} × ${Math.round(r.h)}`,
            r.x + 4,
            r.y - 8,
          );
        }
      } else {
        ctx.fillStyle = "rgba(0,0,0,0.55)";
        const label = "Drag to select region";
        ctx.font = "15px -apple-system, sans-serif";
        const tw = ctx.measureText(label).width;
        ctx.fillRect(
          logicalW / 2 - tw / 2 - 12,
          logicalH / 2 - 20,
          tw + 24,
          32,
        );
        ctx.fillStyle = "white";
        ctx.textAlign = "center";
        ctx.fillText(label, logicalW / 2, logicalH / 2 + 1);
        ctx.textAlign = "start";
      }
      return;
    }

    ctx.fillStyle = "#1a1a1a";
    ctx.fillRect(0, 0, logicalW, logicalH);
    if (cropSrc) {
      ctx.drawImage(
        backgroundImage,
        cropSrc.x,
        cropSrc.y,
        cropSrc.w,
        cropSrc.h,
        bgOffsetX,
        bgOffsetY,
        bgDrawW,
        bgDrawH,
      );
    } else {
      ctx.drawImage(backgroundImage, bgOffsetX, bgOffsetY, bgDrawW, bgDrawH);
    }
  }

  if (dimActive) {
    ctx.fillStyle = "rgba(0, 0, 0, 0.3)";
    ctx.fillRect(0, 0, logicalW, logicalH);
  }

  for (const a of annotations) {
    renderAnnotation(a);
  }
}

function selNormalized() {
  const x = Math.min(selStart.x, selCurrent.x);
  const y = Math.min(selStart.y, selCurrent.y);
  const w = Math.abs(selCurrent.x - selStart.x);
  const h = Math.abs(selCurrent.y - selStart.y);
  return { x, y, w, h };
}

function commitSelection(x, y, w, h) {
  // Selection is in window coords over the full-capture layout; convert to
  // capture pixels and clamp to the image (the drag may start in the letterbox).
  const raw = {
    x: ((x - bgOffsetX) / bgDrawW) * backgroundImage.naturalWidth,
    y: ((y - bgOffsetY) / bgDrawH) * backgroundImage.naturalHeight,
    w: (w / bgDrawW) * backgroundImage.naturalWidth,
    h: (h / bgDrawH) * backgroundImage.naturalHeight,
  };
  const clamped = mapCropToNative(raw, {
    previewWidth: backgroundImage.naturalWidth,
    previewHeight: backgroundImage.naturalHeight,
    nativeWidth: backgroundImage.naturalWidth,
    nativeHeight: backgroundImage.naturalHeight,
  });
  if (!clamped || clamped.w < 1 || clamped.h < 1) {
    selStart = null;
    selCurrent = null;
    render();
    return;
  }
  cropSrc = clamped;
  computeBgLayout();
  selectionPhase = false;
  selStart = null;
  selCurrent = null;
  canvas.style.cursor = "default";
  toolbar.classList.add("visible");
  render();
}

// Draws one annotation in logical window coordinates onto `c`. The on-screen
// canvas and the export canvas both use this; the export canvas just has a
// transform installed that maps window coords to image pixels.
function renderAnnotation(a, c = ctx) {
  c.save();
  c.strokeStyle = a.color;
  c.fillStyle = a.color;
  c.lineWidth = a.strokeWidth || currentWidth;
  c.lineCap = "round";
  c.lineJoin = "round";

  switch (a.type) {
    case "circle":
      c.beginPath();
      c.ellipse(
        a.cx,
        a.cy,
        Math.abs(a.rx),
        Math.abs(a.ry),
        0,
        0,
        Math.PI * 2,
      );
      c.stroke();
      break;

    case "rect":
      c.strokeRect(a.x, a.y, a.width, a.height);
      // Subtle fill
      c.fillStyle = a.color + "1A"; // 10% opacity
      c.fillRect(a.x, a.y, a.width, a.height);
      break;

    case "arrow":
      drawArrowOn(c, a.fromX, a.fromY, a.toX, a.toY, a.color, a.strokeWidth);
      break;

    case "freehand":
      if (a.points.length < 2) break;
      c.beginPath();
      c.moveTo(a.points[0].x, a.points[0].y);
      // Smooth the path with quadratic curves for a buttery feel
      for (let i = 1; i < a.points.length - 1; i++) {
        const midX = (a.points[i].x + a.points[i + 1].x) / 2;
        const midY = (a.points[i].y + a.points[i + 1].y) / 2;
        c.quadraticCurveTo(a.points[i].x, a.points[i].y, midX, midY);
      }
      // Last point
      const last = a.points[a.points.length - 1];
      c.lineTo(last.x, last.y);
      c.stroke();
      break;

    case "text": {
      const fontSize = a.fontSize || 16;
      c.font = `bold ${fontSize}px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`;
      // Dark outline for readability on any background
      c.strokeStyle = "rgba(0, 0, 0, 0.8)";
      c.lineWidth = 3;
      c.lineJoin = "round";
      c.strokeText(a.content, a.x, a.y);
      c.fillText(a.content, a.x, a.y);
      break;
    }

    case "marker": {
      const r = a.radius || 16;
      // Shadow for depth
      c.shadowColor = "rgba(0,0,0,0.4)";
      c.shadowBlur = 6;
      c.shadowOffsetY = 2;
      c.beginPath();
      c.arc(a.x, a.y, r, 0, Math.PI * 2);
      c.fill();
      c.shadowColor = "transparent";
      // White number
      c.fillStyle = "#FFFFFF";
      c.font = `bold ${r}px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`;
      c.textAlign = "center";
      c.textBaseline = "middle";
      c.fillText(String(a.number), a.x, a.y);
      c.textAlign = "start";
      c.textBaseline = "alphabetic";
      break;
    }
  }

  c.restore();
}

function drawArrowOn(c, fromX, fromY, toX, toY, color, width) {
  const headLen = 12 + width;
  const angle = Math.atan2(toY - fromY, toX - fromX);
  c.strokeStyle = color;
  c.lineWidth = width;
  c.beginPath();
  c.moveTo(fromX, fromY);
  c.lineTo(toX, toY);
  c.stroke();
  c.fillStyle = color;
  c.beginPath();
  c.moveTo(toX, toY);
  c.lineTo(
    toX - headLen * Math.cos(angle - Math.PI / 6),
    toY - headLen * Math.sin(angle - Math.PI / 6),
  );
  c.lineTo(
    toX - headLen * Math.cos(angle + Math.PI / 6),
    toY - headLen * Math.sin(angle + Math.PI / 6),
  );
  c.closePath();
  c.fill();
}

function drawArrow(fromX, fromY, toX, toY, color, width) {
  drawArrowOn(ctx, fromX, fromY, toX, toY, color, width);
}

// ----- Mouse events -----
canvas.addEventListener("mousedown", (e) => {
  if (e.target !== canvas) return;
  const x = e.offsetX;
  const y = e.offsetY;

  if (selectionPhase) {
    selStart = { x, y };
    selCurrent = { x, y };
    return;
  }

  if (currentTool === "text") {
    showTextInput(x, y);
    return;
  }

  if (currentTool === "marker") {
    annotations.push({
      type: "marker",
      x,
      y,
      number: markerCounter++,
      color: currentColor,
      radius: 16,
    });
    render();
    return;
  }

  isDrawing = true;
  drawStart = { x, y };

  if (currentTool === "freehand") {
    freehandPoints = [{ x, y }];
  }
});

canvas.addEventListener("mousemove", (e) => {
  if (selectionPhase) {
    if (selStart) {
      selCurrent = { x: e.offsetX, y: e.offsetY };
      render();
    }
    return;
  }
  if (!isDrawing) return;
  const x = e.offsetX;
  const y = e.offsetY;

  if (currentTool === "freehand") {
    freehandPoints.push({ x, y });
    // Live preview
    render();
    ctx.save();
    ctx.strokeStyle = currentColor;
    ctx.lineWidth = currentWidth;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(freehandPoints[0].x, freehandPoints[0].y);
    for (let i = 1; i < freehandPoints.length - 1; i++) {
      const midX = (freehandPoints[i].x + freehandPoints[i + 1].x) / 2;
      const midY = (freehandPoints[i].y + freehandPoints[i + 1].y) / 2;
      ctx.quadraticCurveTo(
        freehandPoints[i].x,
        freehandPoints[i].y,
        midX,
        midY,
      );
    }
    const last = freehandPoints[freehandPoints.length - 1];
    ctx.lineTo(last.x, last.y);
    ctx.stroke();
    ctx.restore();
    return;
  }

  // Live preview for shape tools
  render();
  ctx.save();
  ctx.strokeStyle = currentColor;
  ctx.lineWidth = currentWidth;

  if (currentTool === "circle") {
    const rx = Math.abs(x - drawStart.x) / 2;
    const ry = e.shiftKey ? rx : Math.abs(y - drawStart.y) / 2;
    const cx = drawStart.x + (x - drawStart.x) / 2;
    const cy = drawStart.y + (y - drawStart.y) / 2;
    ctx.beginPath();
    ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
    ctx.stroke();
  } else if (currentTool === "rect") {
    let w = x - drawStart.x;
    let h = e.shiftKey ? w : y - drawStart.y;
    ctx.strokeRect(drawStart.x, drawStart.y, w, h);
    // Preview fill
    ctx.fillStyle = currentColor + "1A";
    ctx.fillRect(drawStart.x, drawStart.y, w, h);
  } else if (currentTool === "arrow") {
    drawArrow(drawStart.x, drawStart.y, x, y, currentColor, currentWidth);
  }

  ctx.restore();
});

canvas.addEventListener("mouseup", (e) => {
  if (selectionPhase) {
    if (selStart) {
      const r = selNormalized();
      if (r.w > 10 && r.h > 10) {
        commitSelection(r.x, r.y, r.w, r.h);
      } else {
        selStart = null;
        selCurrent = null;
        render();
      }
    }
    return;
  }
  if (!isDrawing) return;
  isDrawing = false;

  const x = e.offsetX;
  const y = e.offsetY;

  if (currentTool === "circle") {
    const rx = Math.abs(x - drawStart.x) / 2;
    const ry = e.shiftKey ? rx : Math.abs(y - drawStart.y) / 2;
    const cx = drawStart.x + (x - drawStart.x) / 2;
    const cy = drawStart.y + (y - drawStart.y) / 2;
    if (rx > 2 || ry > 2) {
      annotations.push({
        type: "circle",
        cx,
        cy,
        rx,
        ry,
        color: currentColor,
        strokeWidth: currentWidth,
      });
    }
  } else if (currentTool === "rect") {
    let w = x - drawStart.x;
    let h = e.shiftKey ? w : y - drawStart.y;
    if (Math.abs(w) > 2 || Math.abs(h) > 2) {
      annotations.push({
        type: "rect",
        x: drawStart.x,
        y: drawStart.y,
        width: w,
        height: h,
        color: currentColor,
        strokeWidth: currentWidth,
      });
    }
  } else if (currentTool === "arrow") {
    const dist = Math.hypot(x - drawStart.x, y - drawStart.y);
    if (dist > 5) {
      annotations.push({
        type: "arrow",
        fromX: drawStart.x,
        fromY: drawStart.y,
        toX: x,
        toY: y,
        color: currentColor,
        strokeWidth: currentWidth,
      });
    }
  } else if (currentTool === "freehand") {
    if (freehandPoints.length > 2) {
      // Simplify the path — keep every Nth point for smoother storage
      const simplified = simplifyPoints(freehandPoints, 2);
      annotations.push({
        type: "freehand",
        points: simplified,
        color: currentColor,
        strokeWidth: currentWidth,
      });
    }
    freehandPoints = [];
  }

  drawStart = null;
  render();
});

// Point simplification — reduces density while keeping shape
function simplifyPoints(points, tolerance) {
  if (points.length < 3) return [...points];
  const result = [points[0]];
  for (let i = 1; i < points.length - 1; i++) {
    const prev = result[result.length - 1];
    const dist = Math.hypot(points[i].x - prev.x, points[i].y - prev.y);
    if (dist >= tolerance) {
      result.push(points[i]);
    }
  }
  result.push(points[points.length - 1]);
  return result;
}

// ----- Text input -----
function showTextInput(x, y) {
  textInput.style.display = "block";
  textInput.style.left = x + "px";
  textInput.style.top = y + "px";
  textInput.style.color = currentColor;
  textInput.value = "";
  textInput.focus();

  textInput._x = x;
  textInput._y = y;
}

textInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    const content = textInput.value.trim();
    if (content) {
      annotations.push({
        type: "text",
        x: textInput._x,
        y: textInput._y + 16, // offset for baseline
        content,
        color: currentColor,
        fontSize: 16,
      });
    }
    textInput.style.display = "none";
    textInput.value = "";
    render();
    e.stopPropagation();
  } else if (e.key === "Escape") {
    textInput.style.display = "none";
    textInput.value = "";
    e.stopPropagation();
  }
});

// ----- Toolbar events -----
document.querySelectorAll(".tool-btn").forEach((btn) => {
  btn.addEventListener("click", () => {
    document
      .querySelectorAll(".tool-btn")
      .forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    currentTool = btn.dataset.tool;
  });
});

document.querySelectorAll(".color-btn").forEach((btn) => {
  btn.addEventListener("click", () => {
    document
      .querySelectorAll(".color-btn")
      .forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    currentColor = btn.dataset.color;
  });
});

document.querySelectorAll(".width-btn").forEach((btn) => {
  btn.addEventListener("click", () => {
    document
      .querySelectorAll(".width-btn")
      .forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    currentWidth = parseInt(btn.dataset.width);
  });
});

document.getElementById("btn-dim").addEventListener("click", () => {
  dimActive = !dimActive;
  document.getElementById("btn-dim").classList.toggle("active", dimActive);
  render();
});

document.getElementById("btn-undo").addEventListener("click", undo);
document.getElementById("btn-clear").addEventListener("click", clearAll);
document.getElementById("btn-save").addEventListener("click", save);

// ----- Toolbar dragging -----
let toolbarDragging = false;
let toolbarOffset = { x: 0, y: 0 };

document.getElementById("toolbar-drag").addEventListener("mousedown", (e) => {
  toolbarDragging = true;
  const rect = toolbar.getBoundingClientRect();
  toolbarOffset.x = e.clientX - rect.left;
  toolbarOffset.y = e.clientY - rect.top;
  e.preventDefault();
  e.stopPropagation();
});

document.addEventListener("mousemove", (e) => {
  if (!toolbarDragging) return;
  let newX = e.clientX - toolbarOffset.x;
  let newY = e.clientY - toolbarOffset.y;
  // Keep within viewport
  const tRect = toolbar.getBoundingClientRect();
  newX = Math.max(0, Math.min(newX, window.innerWidth - tRect.width));
  newY = Math.max(0, Math.min(newY, window.innerHeight - tRect.height));
  toolbar.style.left = newX + "px";
  toolbar.style.top = newY + "px";
  toolbar.style.transform = "none";
});

document.addEventListener("mouseup", () => {
  toolbarDragging = false;
});

// ----- Keyboard shortcuts -----
document.addEventListener("keydown", (e) => {
  // Ignore if typing in text input
  if (textInput.style.display === "block") return;

  switch (e.key) {
    case "Enter":
      e.preventDefault();
      save();
      return;
    case "Escape":
      if (isDrawing) {
        isDrawing = false;
        drawStart = null;
        freehandPoints = [];
        render();
      } else {
        closeOverlay();
      }
      return;
  }

  switch (e.key.toLowerCase()) {
    case "c":
      selectTool("circle");
      break;
    case "r":
      selectTool("rect");
      break;
    case "a":
      selectTool("arrow");
      break;
    case "f":
      selectTool("freehand");
      break;
    case "t":
      selectTool("text");
      break;
    case "n":
      selectTool("marker");
      break;
    case "d":
      dimActive = !dimActive;
      document.getElementById("btn-dim").classList.toggle("active", dimActive);
      render();
      break;
    case "z":
      if (e.ctrlKey || e.metaKey) {
        undo();
        e.preventDefault();
      }
      break;
    // enter and escape handled above
  }
});

function selectTool(tool) {
  currentTool = tool;
  document.querySelectorAll(".tool-btn").forEach((b) => {
    b.classList.toggle("active", b.dataset.tool === tool);
  });
}

// ----- Actions -----
function undo() {
  if (annotations.length > 0) {
    const removed = annotations.pop();
    if (removed.type === "marker") {
      markerCounter = Math.max(1, markerCounter - 1);
    }
    render();
  }
}

function clearAll() {
  annotations = [];
  markerCounter = 1;
  render();
}

async function closeOverlay() {
  // Tell the Rust backend the overlay is no longer active
  try {
    await invoke("mark_overlay_closed");
  } catch (_) {}

  // Keep Linux overlay-mode behavior intact: it should still exit after one shot.
  if (navigator.userAgent.includes("Mac")) {
    await getCurrentWindow().hide();
    return;
  }

  await getCurrentWindow().destroy();
}

async function save() {
  if (isSaving) return;
  // Nothing to save until a capture is loaded (e.g. Enter pressed while the
  // capture-failed message is showing).
  if (!backgroundImage) return;
  isSaving = true;

  // Visual feedback — flash the save button
  const saveBtn = document.getElementById("btn-save");
  saveBtn.classList.add("saving");

  // Everything below is expressed in exported-image pixels: the source region
  // (whole capture or crop) at native capture resolution. The same transform
  // positions the drawn annotations in the PNG and the coordinates in the
  // sidecar, so an agent reading the JSON can locate things in the image.
  const t = currentImageTransform();

  // Build sidecar metadata
  const metadata = {
    timestamp: new Date().toISOString(),
    source: {
      window_title: windowContext?.window_title || null,
      url: windowContext?.url || null,
      window_class: windowContext?.window_class || null,
      pid: windowContext?.pid || null,
      session_type: windowContext?.session_type || null,
      display: "primary",
      resolution: [window.screen.width, window.screen.height],
      capture_size: [backgroundImage.naturalWidth, backgroundImage.naturalHeight],
      crop: cropSrc ? { x: cropSrc.x, y: cropSrc.y, w: cropSrc.w, h: cropSrc.h } : null,
    },
    image_size: [t.exportW, t.exportH],
    coordinate_space: "image_pixels",
    annotations: annotations.map((a) => annotationToSidecar(a, t)),
  };

  // Export the annotated image. When nothing was drawn and no region was
  // selected, the raw capture file is byte-identical to what we'd export, so
  // Rust copies it directly and skips the encode + IPC round trip.
  let imageBase64 = null;
  if (annotations.length > 0 || cropSrc) {
    try {
      // backgroundImage was loaded from a data URL, so drawing it onto an
      // offscreen canvas does not taint it and toDataURL works.
      const exportCanvas = document.createElement("canvas");
      exportCanvas.width = t.exportW;
      exportCanvas.height = t.exportH;
      const ectx = exportCanvas.getContext("2d");

      if (cropSrc) {
        ectx.drawImage(
          backgroundImage,
          cropSrc.x,
          cropSrc.y,
          cropSrc.w,
          cropSrc.h,
          0,
          0,
          t.exportW,
          t.exportH,
        );
      } else {
        ectx.drawImage(backgroundImage, 0, 0);
      }

      // Window coords -> image pixels. The canvas bounds clip anything drawn
      // in the letterbox outside the image.
      ectx.setTransform(t.sx, 0, 0, t.sy, -bgOffsetX * t.sx, -bgOffsetY * t.sy);
      for (const a of annotations) {
        renderAnnotation(a, ectx);
      }
      ectx.setTransform(1, 0, 0, 1, 0, 0);

      const dataUrl = exportCanvas.toDataURL("image/png");
      imageBase64 = dataUrl.split(",")[1];
    } catch (e) {
      console.error("Canvas export failed, saving raw capture instead:", e);
      // The raw capture is the uncropped screen, so re-express the sidecar in
      // full-capture pixels (same scale, shifted by the crop origin).
      const ox = cropSrc ? cropSrc.x : 0;
      const oy = cropSrc ? cropSrc.y : 0;
      const tRaw = {
        ...t,
        point: (x, y) => {
          const [px, py] = t.point(x, y);
          return [px + ox, py + oy];
        },
      };
      metadata.image_size = [backgroundImage.naturalWidth, backgroundImage.naturalHeight];
      metadata.source.crop = null;
      metadata.annotations = annotations.map((a) => annotationToSidecar(a, tRaw));
      metadata.export_warning = "annotation compositing failed; image is the raw capture";
    }
  }

  try {
    await invoke("save_annotation", {
      metadataJson: JSON.stringify(metadata, null, 2),
      imageBase64: imageBase64,
    });
  } catch (e) {
    console.error("Save failed:", e);
    isSaving = false;
    saveBtn.classList.remove("saving");
    ctx.save();
    ctx.fillStyle = "rgba(255, 59, 48, 0.9)";
    ctx.font = "bold 16px sans-serif";
    ctx.textAlign = "center";
    ctx.fillText(
      "Save failed: " + e,
      window.innerWidth / 2,
      window.innerHeight - 40,
    );
    ctx.restore();
    return;
  }

  await closeOverlay();
}

// ----- Boot -----
init();
