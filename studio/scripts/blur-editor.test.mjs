import { test } from 'node:test';
import assert from 'node:assert/strict';
import { screenToImage, imageToScreen, fitView, zoomAt, clampPan } from '../src/lib/blur-editor/coords.ts';
import { initialHistory, pushHistory, undoHistory, redoHistory, canUndo, canRedo, HISTORY_LIMIT } from '../src/lib/blur-editor/history.ts';
import { validateStroke, parseDocument, serializeDocument } from '../src/lib/blur-editor/strokes.ts';
import { blurKernelRadius, falloff, distanceToStroke, cloneRaster, applyStroke, applyStrokeReference, applyStrokeAsync, strokeWindow, translateStroke, renderDocument, commonPrefix, MAX_BLUR_RADIUS } from '../src/lib/blur-editor/blur.ts';
import { scaleStroke, visibleWindow, previewBudget } from '../src/lib/blur-editor/preview.ts';

function solidRaster(width, height, [r, g, b]) {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < width * height; i++) { data[i * 4] = r; data[i * 4 + 1] = g; data[i * 4 + 2] = b; data[i * 4 + 3] = 255; }
  return { width, height, data };
}
function fillSquare(raster, x0, y0, size, [r, g, b]) {
  const copy = cloneRaster(raster);
  for (let y = y0; y < y0 + size; y++) for (let x = x0; x < x0 + size; x++) { const i = (y * raster.width + x) * 4; copy.data[i] = r; copy.data[i + 1] = g; copy.data[i + 2] = b; }
  return copy;
}
function stroke(overrides = {}) {
  return { id: 's1', size: 8, intensity: 0.9, hardness: 0.5, points: [{ x: 10, y: 10 }], ...overrides };
}

test('coordinate mapping roundtrips and stays independent of zoom and pan', () => {
  for (const view of [{ zoom: 1, x: 0, y: 0 }, { zoom: 0.37, x: -120.5, y: 88 }, { zoom: 4, x: 640, y: -31.25 }]) {
    const point = screenToImage(321.7, 88.25, view);
    const back = imageToScreen(point.x, point.y, view);
    assert.ok(Math.abs(back.x - 321.7) < 1e-9 && Math.abs(back.y - 88.25) < 1e-9);
  }
  assert.deepEqual(screenToImage(100, 50, { zoom: 2, x: 10, y: 20 }), { x: 45, y: 15 });
});

test('fitView centers the image and zoomAt keeps the point under the cursor pinned', () => {
  const fitted = fitView(2000, 1000, 800, 400, 20);
  assert.equal(fitted.zoom, 0.36);
  assert.equal(fitted.x, 40); assert.equal(fitted.y, 20);
  const view = { zoom: 1, x: -50, y: -25 };
  const zoomed = zoomAt(view, 150, 75, 2);
  const before = screenToImage(150, 75, view), after = screenToImage(150, 75, zoomed);
  assert.ok(Math.abs(before.x - after.x) < 1e-9 && Math.abs(before.y - after.y) < 1e-9);
  assert.equal(zoomAt(view, 0, 0, 100, 0.05, 16).zoom, 16);
  assert.equal(zoomAt(view, 0, 0, 0.0001, 0.05, 16).zoom, 0.05);
});

test('clampPan keeps the image visible and centers images smaller than the viewport', () => {
  const moved = clampPan({ zoom: 1, x: 500, y: -4000 }, 800, 600, 400, 300);
  assert.deepEqual(moved, { zoom: 1, x: 0, y: -300 });
  const centered = clampPan({ zoom: 0.2, x: -90, y: 55 }, 800, 600, 400, 300);
  assert.deepEqual(centered, { zoom: 0.2, x: 120, y: 90 });
});

test('bounded history pushes, undoes, redoes and drops the oldest state', () => {
  let history = initialHistory('v0');
  assert.ok(!canUndo(history) && !canRedo(history));
  for (const value of ['v1', 'v2', 'v3']) history = pushHistory(history, value);
  assert.deepEqual(history.past, ['v0', 'v1', 'v2']);
  history = undoHistory(history);
  assert.equal(history.current, 'v2');
  assert.deepEqual(history.future, ['v3']);
  history = undoHistory(history); history = undoHistory(history);
  assert.equal(history.current, 'v0'); assert.ok(!canUndo(history) && canRedo(history));
  history = redoHistory(history); history = redoHistory(history);
  assert.equal(history.current, 'v2'); assert.ok(canRedo(history));
  history = redoHistory(history);
  assert.ok(!canRedo(history));
  history = pushHistory(history, 'v4');
  assert.ok(!canRedo(history)); assert.deepEqual(history.past, ['v0', 'v1', 'v2', 'v3']);
  history = initialHistory('v0');
  for (let i = 1; i <= HISTORY_LIMIT + 3; i++) history = pushHistory(history, `v${i}`);
  assert.equal(history.past.length, HISTORY_LIMIT);
  assert.equal(history.past[0], 'v3');
  for (let i = 0; i < HISTORY_LIMIT; i++) history = undoHistory(history);
  assert.equal(history.current, 'v3'); assert.ok(!canUndo(history));
  assert.throws(() => initialHistory('v0', 0));
  assert.deepEqual(undoHistory(initialHistory('v0')), initialHistory('v0'));
});

test('stroke documents roundtrip and reject malformed payloads', () => {
  const document = { version: 1, strokes: [stroke({ points: [{ x: 1.5, y: 2 }, { x: 8, y: 9 }] })] };
  assert.deepEqual(parseDocument(serializeDocument(document)), document);
  for (const [raw, label] of [
    ['not json', 'json'],
    ['{}', 'version'],
    ['{"version":2,"strokes":[]}', 'version'],
    ['{"version":1}', 'strokes'],
    [JSON.stringify({ version: 1, strokes: [{ id: '', size: 8, intensity: 0.5, hardness: 0.5, points: [{ x: 1, y: 1 }] }] }), 'id'],
    [JSON.stringify({ version: 1, strokes: [{ id: 'a', size: 0, intensity: 0.5, hardness: 0.5, points: [{ x: 1, y: 1 }] }] }), 'taille'],
    [JSON.stringify({ version: 1, strokes: [{ id: 'a', size: 8, intensity: 1.5, hardness: 0.5, points: [{ x: 1, y: 1 }] }] }), 'intensité'],
    [JSON.stringify({ version: 1, strokes: [{ id: 'a', size: 8, intensity: 0.5, hardness: 'net', points: [{ x: 1, y: 1 }] }] }), 'dureté'],
    [JSON.stringify({ version: 1, strokes: [{ id: 'a', size: 8, intensity: 0.5, hardness: 0.5, points: [] }] }), 'point'],
    [JSON.stringify({ version: 1, strokes: [{ id: 'a', size: 8, intensity: 0.5, hardness: 0.5, points: [{ x: Infinity, y: 1 }] }] }), 'coordonnée'],
  ]) {
    assert.throws(() => parseDocument(raw), label);
  }
  assert.throws(() => validateStroke(null));
  assert.deepEqual(parseDocument(serializeDocument(document)).strokes[0].points[1], { x: 8, y: 9 });
});

test('blur radius, falloff and stroke distance behave within their bounds', () => {
  assert.equal(blurKernelRadius(0), 0);
  assert.equal(blurKernelRadius(1), MAX_BLUR_RADIUS);
  assert.equal(blurKernelRadius(0.5), Math.round(MAX_BLUR_RADIUS / 2));
  assert.equal(falloff(4, 4, 0.5), 0);
  assert.equal(falloff(5, 4, 0.5), 0);
  assert.equal(falloff(2, 4, 1), 1);
  assert.equal(falloff(3.9, 4, 1), 1);
  assert.equal(falloff(0, 4, 0), 1);
  assert.ok(falloff(3, 4, 0.5) > 0 && falloff(3, 4, 0.5) < 1);
  assert.ok(falloff(3, 4, 0.9) > falloff(3, 4, 0.5));
  assert.equal(distanceToStroke(13, 14, [{ x: 10, y: 10 }]), 5);
  assert.equal(distanceToStroke(10, 9, [{ x: 10, y: 10 }, { x: 10, y: 20 }]), 1);
  assert.equal(distanceToStroke(13, 15, [{ x: 10, y: 10 }, { x: 10, y: 20 }]), 3);
});

test('applyStroke keeps dimensions, leaves intensity 0 untouched and blurs inside the footprint only', () => {
  const raster = fillSquare(solidRaster(21, 21, [40, 40, 40]), 9, 9, 3, [240, 240, 240]);
  const before = cloneRaster(raster);
  const zero = cloneRaster(raster);
  applyStroke(zero, stroke({ intensity: 0 }));
  assert.deepEqual(Array.from(zero.data), Array.from(before.data));
  assert.equal(zero.width, 21); assert.equal(zero.height, 21);
  const edited = cloneRaster(raster);
  applyStroke(edited, stroke({ size: 8, intensity: 1, hardness: 1, points: [{ x: 10.5, y: 10.5 }] }));
  assert.equal(edited.width, 21); assert.equal(edited.height, 21);
  assert.notDeepEqual(Array.from(edited.data), Array.from(before.data));
  const center = (10 * 21 + 10) * 4;
  assert.ok(edited.data[center] < before.data[center] && edited.data[center] > 40);
  assert.equal(edited.data[center + 3], 255);
  assert.deepEqual(Array.from(edited.data.subarray(0, (4 * 21 + 4) * 4)), Array.from(before.data.subarray(0, (4 * 21 + 4) * 4)));
  assert.equal(edited.data[(5 * 21 + 10) * 4], before.data[(5 * 21 + 10) * 4]);
});

test('hardness bounds the brush edge: hard cut at the radius, gradual with softness', () => {
  const raster = fillSquare(solidRaster(31, 31, [10, 10, 10]), 11, 11, 9, [240, 240, 240]);
  const hard = cloneRaster(raster);
  applyStroke(hard, stroke({ size: 12, intensity: 1, hardness: 1, points: [{ x: 15.5, y: 15.5 }] }));
  const center = (15 * 31 + 15) * 4, plateau = (12 * 31 + 12) * 4, outside = (9 * 31 + 15) * 4, insideEdge = (10 * 31 + 15) * 4;
  assert.ok(hard.data[plateau] > 10);
  assert.equal(hard.data[outside], raster.data[outside]);
  assert.ok(hard.data[insideEdge] > 10);
  const soft = cloneRaster(raster);
  applyStroke(soft, stroke({ size: 12, intensity: 1, hardness: 0, points: [{ x: 15.5, y: 15.5 }] }));
  const atEdge = (10 * 31 + 15) * 4;
  assert.ok(soft.data[center] > 10);
  assert.ok(soft.data[atEdge] > 10 && soft.data[atEdge] < hard.data[atEdge]);
  assert.equal(soft.data[outside], raster.data[outside]);
});

test('out-of-bounds strokes are clipped or ignored and rendering replays documents deterministically', () => {
  const raster = fillSquare(solidRaster(10, 10, [0, 0, 0]), 0, 0, 1, [200, 200, 200]);
  const outside = cloneRaster(raster);
  applyStroke(outside, stroke({ points: [{ x: 50, y: 50 }] }));
  assert.deepEqual(Array.from(outside.data), Array.from(raster.data));
  const partial = cloneRaster(raster);
  assert.doesNotThrow(() => applyStroke(partial, stroke({ size: 12, intensity: 1, hardness: 1, points: [{ x: -2, y: -2 }] })));
  assert.equal(partial.width, 10); assert.equal(partial.height, 10);
  assert.notEqual(partial.data[0], raster.data[0]);
  const document = { version: 1, strokes: [stroke({ id: 'a' }), stroke({ id: 'b', points: [{ x: 2, y: 2 }, { x: 6, y: 6 }] })] };
  const first = renderDocument(raster, document);
  const second = renderDocument(raster, document);
  assert.equal(first.width, 10); assert.equal(first.height, 10);
  assert.deepEqual(Array.from(first.data), Array.from(second.data));
  assert.notDeepEqual(Array.from(first.data), Array.from(raster.data));
  const replay = cloneRaster(raster);
  for (const s of document.strokes) applyStroke(replay, s);
  assert.deepEqual(Array.from(replay.data), Array.from(first.data));
});

test('commonPrefix detects identical prefixes for incremental replay', () => {
  const a = [stroke({ id: 'a' }), stroke({ id: 'b' })];
  assert.equal(commonPrefix(a, [...a, stroke({ id: 'c' })]), 2);
  assert.equal(commonPrefix(a, [stroke({ id: 'a' })]), 1);
  assert.equal(commonPrefix(a, [stroke({ id: 'a', size: 9 }), stroke({ id: 'b' })]), 0);
  assert.equal(commonPrefix(a, []), 0);
  assert.equal(commonPrefix([], a), 0);
  assert.equal(commonPrefix(a, a.map(s => ({ ...s, points: [...s.points] }))), 2);
});

function lcg(seed) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }
function noiseRaster(width, height, rnd) {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < data.length; i++) data[i] = Math.floor(rnd() * 256);
  return { width, height, data };
}
function maxDiff(a, b) { let m = 0; for (let i = 0; i < a.data.length; i++) m = Math.max(m, Math.abs(a.data[i] - b.data[i])); return m; }

test('fast applyStroke matches the slow reference on random images and strokes (<= 1 level)', () => {
  const rnd = lcg(12345);
  for (let round = 0; round < 25; round++) {
    const width = 20 + Math.floor(rnd() * 90), height = 15 + Math.floor(rnd() * 90);
    const fast = noiseRaster(width, height, rnd), slow = cloneRaster(fast);
    for (let k = 0; k < 4; k++) {
      const count = 1 + Math.floor(rnd() * 12);
      const points = [];
      for (let i = 0; i < count; i++) points.push({ x: rnd() * (width + 30) - 15, y: rnd() * (height + 30) - 15 });
      const s = { id: `r${round}-${k}`, size: 1 + rnd() * 70, intensity: rnd() < 0.1 ? 1 : rnd(), hardness: rnd() < 0.2 ? 1 : rnd(), points };
      applyStroke(fast, s); applyStrokeReference(slow, s);
    }
    assert.ok(maxDiff(fast, slow) <= 1, `round ${round}: diff ${maxDiff(fast, slow)}`);
  }
});

test('stroke applied on a strokeWindow crop equals the full-raster application', () => {
  const rnd = lcg(777);
  const full = noiseRaster(120, 90, rnd);
  const s = { id: 'w', size: 24, intensity: 0.8, hardness: 0.4, points: [{ x: 30.3, y: 20 }, { x: 70, y: 55.5 }, { x: 110, y: 40 }] };
  const expected = cloneRaster(full); applyStroke(expected, s);
  const win = strokeWindow(s, full.width, full.height);
  const crop = { width: win.w, height: win.h, data: new Uint8ClampedArray(win.w * win.h * 4) };
  for (let y = 0; y < win.h; y++) crop.data.set(full.data.subarray(((win.y + y) * full.width + win.x) * 4, ((win.y + y) * full.width + win.x + win.w) * 4), y * win.w * 4);
  applyStroke(crop, translateStroke(s, -win.x, -win.y));
  const actual = cloneRaster(full);
  for (let y = 0; y < win.h; y++) actual.data.set(crop.data.subarray(y * win.w * 4, (y + 1) * win.w * 4), ((win.y + y) * full.width + win.x) * 4);
  assert.deepEqual(Array.from(actual.data), Array.from(expected.data));
  assert.equal(strokeWindow({ ...s, points: [{ x: 500, y: 500 }] }, 120, 90), null);
});

test('applyStrokeAsync equals applyStroke and cancellation leaves the raster untouched', async () => {
  const rnd = lcg(99);
  const a = noiseRaster(200, 300, rnd), b = cloneRaster(a), c = cloneRaster(a);
  const s = { id: 'a', size: 40, intensity: 0.9, hardness: 0.5, points: [{ x: 20, y: 20 }, { x: 180, y: 280 }] };
  applyStroke(a, s);
  assert.equal(await applyStrokeAsync(b, s, () => false, 0), true);
  assert.deepEqual(Array.from(b.data), Array.from(a.data));
  const before = Array.from(c.data);
  assert.equal(await applyStrokeAsync(c, s, () => true), false);
  assert.deepEqual(Array.from(c.data), before);
});

test('benchmark: 6000x4000 diagonal stroke, 120 points, size 60 finishes under 1500 ms', () => {
  const width = 6000, height = 4000;
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < data.length; i++) data[i] = (Math.imul(i, 2654435761) >>> 24) & 255;
  const points = [];
  for (let i = 0; i < 120; i++) points.push({ x: 300 + i * (3600 / 119), y: 300 + i * (2400 / 119) });
  const started = performance.now();
  applyStroke({ width, height, data }, { id: 'bench', size: 60, intensity: 0.7, hardness: 0.5, points });
  const elapsed = performance.now() - started;
  console.log(`benchmark applyStroke 6000x4000/120pts: ${elapsed.toFixed(0)} ms`);
  assert.ok(elapsed < 1500, `trop lent : ${elapsed} ms`);
});

// --- Real-time preview at screen resolution (preview.ts) ---

test('scaleStroke maps the image stroke into screen space (zoom + pan, diameter scaled)', () => {
  const view = { zoom: 0.25, x: -120, y: 40 };
  const scaled = scaleStroke({ id: 's', size: 120, intensity: 0.7, hardness: 0.5, points: [{ x: 400, y: 800 }, { x: 1200, y: 1600 }] }, view);
  assert.equal(scaled.size, 30);
  assert.deepEqual(scaled.points, [{ x: -20, y: 240 }, { x: 180, y: 440 }]);
  assert.throws(() => scaleStroke({ id: 's', size: 1, intensity: 1, hardness: 1, points: [{ x: 0, y: 0 }] }, { zoom: 0, x: 0, y: 0 }));
});

test('visibleWindow clamps the visible image region and rejects off-screen views', () => {
  const win = visibleWindow({ zoom: 0.5, x: -100, y: -200 }, 800, 600, 4000, 3000);
  assert.deepEqual(win, { x: 200, y: 400, w: 1600, h: 1200 });
  assert.deepEqual(visibleWindow({ zoom: 1, x: 0, y: 0 }, 800, 600, 400, 300), { x: 0, y: 0, w: 400, h: 300 });
  assert.equal(visibleWindow({ zoom: 1, x: 5000, y: 5000 }, 800, 600, 400, 300), null);
});

test('latency before/after: screen-resolution preview stroke is much cheaper than full-resolution application', () => {
  // Preview: a 2048x3072 image fitted into a 1536x864 screen (fitted zoom).
  const imageWidth = 2048, imageHeight = 3072;
  const canvasWidth = 1536, canvasHeight = 864;
  const zoom = Math.min((canvasWidth - 48) / imageWidth, (canvasHeight - 48) / imageHeight);
  const view = { zoom, x: (canvasWidth - imageWidth * zoom) / 2, y: (canvasHeight - imageHeight * zoom) / 2 };
  const points = Array.from({ length: 120 }, (_, i) => ({ x: 200 + i * 12, y: 300 + i * 18 }));
  const stroke = { id: 'bench', size: 120, intensity: 0.7, hardness: 0.5, points };
  // JIT warm-up on a small raster.
  applyStroke(solidRaster(64, 64, [10, 20, 30]), { ...stroke, points: [{ x: 10, y: 10 }, { x: 40, y: 40 }] });
  // BEFORE (equivalent): applying the stroke at full image resolution.
  const checker = (w, h) => { const data = new Uint8ClampedArray(w * h * 4); for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) { const i = (y * w + x) * 4; const v = (x + y) % 2 ? 200 : 40; data[i] = v; data[i + 1] = v; data[i + 2] = v; data[i + 3] = 255; } return data; };
  const full = cloneRaster({ width: imageWidth, height: imageHeight, data: checker(imageWidth, imageHeight) });
  const fullStart = performance.now();
  applyStroke(full, stroke);
  const fullMs = performance.now() - fullStart;
  // AFTER: same stroke at screen resolution (cropped visible window), which is what
  // a preview frame computes during the drag.
  const win = visibleWindow(view, canvasWidth, canvasHeight, imageWidth, imageHeight);
  assert.ok(win, 'preview window expected');
  const scaled = scaleStroke(stroke, view);
  const local = translateStroke(scaled, -win.x, -win.y);
  const screen = { width: win.w, height: win.h, data: checker(win.w, win.h) };
  // Pixel in the middle of the stroke and pixel far from the brush, before applying.
  const mid = points[60];
  const mx = Math.round(mid.x * zoom + view.x - win.x), my = Math.round(mid.y * zoom + view.y - win.y);
  const centerBefore = screen.data[(my * win.w + mx) * 4];
  const cornerBefore = screen.data[0];
  const previewStart = performance.now();
  applyStroke(screen, local);
  const previewMs = performance.now() - previewStart;
  assert.notEqual(screen.data[(my * win.w + mx) * 4], centerBefore, 'preview pixel at stroke center unchanged');
  assert.equal(screen.data[0], cornerBefore, 'pixel away from the brush must stay untouched');
  // Expected ratio ≈ (zoom)^2: the preview must stay clearly
  // cheaper. 2x margin for machine variance.
  assert.ok(previewMs < fullMs, `preview ${previewMs.toFixed(1)}ms should beat full ${fullMs.toFixed(1)}ms`);
  console.log(`latence flou — pleine résolution image (${imageWidth}x${imageHeight}, pinceau 120px) : ${fullMs.toFixed(1)} ms · aperçu écran (${win.w}x${win.h}, pinceau ${(120 * zoom).toFixed(0)}px) : ${previewMs.toFixed(1)} ms · gain ${(fullMs / previewMs).toFixed(1)}x`);
});
