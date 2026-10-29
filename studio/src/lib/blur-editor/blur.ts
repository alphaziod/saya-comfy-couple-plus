// Pixel-localised blur brush: pure logic with no DOM (Node --test).
// A Raster structure is structurally compatible with the canvas 2D ImageData
// ({width, height, data: Uint8ClampedArray}): the component passes its
// ImageData directly, Node tests build the same structure.

import type { Stroke, StrokeDocument, StrokePoint } from './strokes';

export interface Raster { width: number; height: number; data: Uint8ClampedArray }

// Maximum radius of the blur kernel (box blur window 2r+1), reached at intensity 1.
export const MAX_BLUR_RADIUS = 12;

export function blurKernelRadius(intensity: number): number {
  return Math.max(0, Math.round(intensity * MAX_BLUR_RADIUS));
}

// Brush weight by distance from the centre: 1 inside the plateau,
// linear falloff down to 0 at the edge; hardness 1 = sharp edge.
export function falloff(distance: number, radius: number, hardness: number): number {
  if (!(radius > 0) || !(distance < radius)) return 0;
  const inner = radius * Math.min(1, Math.max(0, hardness));
  if (distance <= inner) return 1;
  const ramp = radius - inner;
  return ramp < 1e-9 ? 1 : (radius - distance) / ramp;
}

function distanceToSegment(x: number, y: number, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax, dy = by - ay;
  const lengthSquared = dx * dx + dy * dy;
  const t = lengthSquared > 0 ? Math.min(1, Math.max(0, ((x - ax) * dx + (y - ay) * dy) / lengthSquared)) : 0;
  return Math.hypot(x - (ax + t * dx), y - (ay + t * dy));
}

export function distanceToStroke(x: number, y: number, points: readonly StrokePoint[]): number {
  if (!points.length) return Infinity;
  let best = Infinity;
  for (let i = 1; i < points.length; i++) {
    best = Math.min(best, distanceToSegment(x, y, points[i - 1].x, points[i - 1].y, points[i].x, points[i].y));
  }
  const last = points[points.length - 1];
  return Math.min(best, Math.hypot(x - last.x, y - last.y));
}

export function cloneRaster(raster: Raster): Raster {
  return { width: raster.width, height: raster.height, data: new Uint8ClampedArray(raster.data) };
}

// Separable box blur with clamped edges, sliding window, over the inclusive region
// [x0..x1]×[y0..y1]. Returns a new structure bounded to the region.
export function blurRegion(raster: Raster, x0: number, y0: number, x1: number, y1: number, radius: number): Raster {
  const width = x1 - x0 + 1, height = y1 - y0 + 1;
  const region = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    const source = ((y0 + y) * raster.width + x0) * 4;
    region.set(raster.data.subarray(source, source + width * 4), y * width * 4);
  }
  if (radius <= 0) return { width, height, data: region };
  const horizontal = new Uint8ClampedArray(region.length);
  boxPass(region, horizontal, width, height, radius, true);
  const vertical = new Uint8ClampedArray(region.length);
  boxPass(horizontal, vertical, width, height, radius, false);
  return { width, height, data: vertical };
}

function boxPass(source: Uint8ClampedArray, target: Uint8ClampedArray, width: number, height: number, radius: number, horizontal: boolean): void {
  const outer = horizontal ? height : width;
  const inner = horizontal ? width : height;
  const stride = horizontal ? 4 : width * 4;
  const line = horizontal ? width * 4 : 4;
  const windowSize = radius * 2 + 1;
  for (let o = 0; o < outer; o++) {
    const base = o * line;
    const sums = [0, 0, 0, 0];
    for (let k = -radius; k <= radius; k++) {
      const index = base + Math.min(inner - 1, Math.max(0, k)) * stride;
      for (let c = 0; c < 4; c++) sums[c] += source[index + c];
    }
    for (let i = 0; i < inner; i++) {
      const targetIndex = base + i * stride;
      for (let c = 0; c < 4; c++) target[targetIndex + c] = sums[c] / windowSize;
      const addIndex = base + Math.min(inner - 1, i + radius + 1) * stride;
      const removeIndex = base + Math.max(0, i - radius) * stride;
      for (let c = 0; c < 4; c++) {
        sums[c] += source[addIndex + c] - source[removeIndex + c];
      }
    }
  }
}

function strokeBounds(stroke: Stroke, padding: number): { minX: number; minY: number; maxX: number; maxY: number } {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const point of stroke.points) {
    minX = Math.min(minX, point.x); minY = Math.min(minY, point.y);
    maxX = Math.max(maxX, point.x); maxY = Math.max(maxY, point.y);
  }
  return { minX: minX - padding, minY: minY - padding, maxX: maxX + padding, maxY: maxY + padding };
}

// Slow reference (O(bbox × points)) kept for equivalence tests.
// Applies the stroke blur on the raster (mutates the given raster, never
// the base original: the component works on a copy). Strokes outside the
// bounds are ignored; strokes partially outside the bounds are clipped.
export function applyStrokeReference(raster: Raster, stroke: Stroke): void {
  const radius = stroke.size / 2;
  const kernel = blurKernelRadius(stroke.intensity);
  if (!stroke.points.length || !(radius > 0) || kernel === 0) return;
  const bounds = strokeBounds(stroke, radius + 1);
  const x0 = Math.max(0, Math.floor(bounds.minX)), y0 = Math.max(0, Math.floor(bounds.minY));
  const x1 = Math.min(raster.width - 1, Math.ceil(bounds.maxX)), y1 = Math.min(raster.height - 1, Math.ceil(bounds.maxY));
  if (x0 > x1 || y0 > y1) return;
  const ex0 = Math.max(0, x0 - kernel), ey0 = Math.max(0, y0 - kernel);
  const ex1 = Math.min(raster.width - 1, x1 + kernel), ey1 = Math.min(raster.height - 1, y1 + kernel);
  const blurred = blurRegion(raster, ex0, ey0, ex1, ey1, kernel);
  for (let y = y0; y <= y1; y++) {
    for (let x = x0; x <= x1; x++) {
      const weight = stroke.intensity * falloff(distanceToStroke(x + 0.5, y + 0.5, stroke.points), radius, stroke.hardness);
      if (weight <= 0) continue;
      const source = ((y - ey0) * blurred.width + (x - ex0)) * 4;
      const target = (y * raster.width + x) * 4;
      for (let c = 0; c < 4; c++) raster.data[target + c] = raster.data[target + c] * (1 - weight) + blurred.data[source + c] * weight;
    }
  }
}

export interface StrokeWindow { x: number; y: number; w: number; h: number }

// Image window (clipped to the bounds) to read/write for a stroke: brush bbox
// + blur radius margin. Null if the stroke has no effect.
export function strokeWindow(stroke: Stroke, width: number, height: number): StrokeWindow | null {
  const radius = stroke.size / 2;
  const kernel = blurKernelRadius(stroke.intensity);
  if (!stroke.points.length || !(radius > 0) || kernel === 0) return null;
  const b = strokeBounds(stroke, radius + 1);
  const x0 = Math.max(0, Math.floor(b.minX) - kernel), y0 = Math.max(0, Math.floor(b.minY) - kernel);
  const x1 = Math.min(width - 1, Math.ceil(b.maxX) + kernel), y1 = Math.min(height - 1, Math.ceil(b.maxY) + kernel);
  if (Math.ceil(b.minX) > width - 1 || Math.floor(b.maxX) < 0 || Math.ceil(b.minY) > height - 1 || Math.floor(b.maxY) < 0) return null;
  if (x0 > x1 || y0 > y1) return null;
  return { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 };
}

export function translateStroke(stroke: Stroke, dx: number, dy: number): Stroke {
  return { ...stroke, points: stroke.points.map(p => ({ x: p.x + dx, y: p.y + dy })) };
}

const BAND_ROWS = 64;

// Stroke work split into row bands (generator: yields between bands).
// Per band: coverage mask (max of the weights per segment, each in its
// small box, per capsule interval), then separable blur only on
// the rectangle actually covered. Results are written at the end so that
// all bands read the pre-stroke image (like the reference).
function* strokeJob(raster: Raster, stroke: Stroke): Generator<void, void, void> {
  const radius = stroke.size / 2;
  const kernel = blurKernelRadius(stroke.intensity);
  if (!stroke.points.length || !(radius > 0) || kernel === 0) return;
  const bounds = strokeBounds(stroke, radius + 1);
  const x0 = Math.max(0, Math.floor(bounds.minX)), y0 = Math.max(0, Math.floor(bounds.minY));
  const x1 = Math.min(raster.width - 1, Math.ceil(bounds.maxX)), y1 = Math.min(raster.height - 1, Math.ceil(bounds.maxY));
  if (x0 > x1 || y0 > y1) return;
  const W = raster.width, H = raster.height, src = raster.data;
  const pts = stroke.points;
  const segs: number[][] = [];
  if (pts.length === 1) segs.push([pts[0].x, pts[0].y, pts[0].x, pts[0].y]);
  for (let i = 1; i < pts.length; i++) segs.push([pts[i - 1].x, pts[i - 1].y, pts[i].x, pts[i].y]);
  const rSq = radius * radius;
  const inner = radius * Math.min(1, Math.max(0, stroke.hardness));
  const ramp = radius - inner;
  const bw = x1 - x0 + 1;
  const mask = new Float32Array(bw * Math.min(BAND_ROWS, y1 - y0 + 1));
  const results: { x: number; y: number; w: number; h: number; data: Uint8ClampedArray }[] = [];
  const winSize = kernel * 2 + 1;

  for (let by0 = y0; by0 <= y1; by0 += BAND_ROWS) {
    const by1 = Math.min(y1, by0 + BAND_ROWS - 1);
    mask.fill(0, 0, bw * (by1 - by0 + 1));
    let minX = Infinity, maxX = -Infinity;
    for (const [ax, ay, bx, by] of segs) {
      if (Math.min(ay, by) - radius > by1 + 1 || Math.max(ay, by) + radius < by0) continue;
      const dx = bx - ax, dy = by - ay, len2 = dx * dx + dy * dy;
      const ys = Math.max(by0, Math.floor(Math.min(ay, by) - radius)), ye = Math.min(by1, Math.ceil(Math.max(ay, by) + radius));
      for (let y = ys; y <= ye; y++) {
        const py = y + 0.5;
        // interval of t where the segment is within radius in ordinate
        let tLo = 0, tHi = 1;
        if (dy !== 0) {
          const ta = (py - radius - ay) / dy, tb = (py + radius - ay) / dy;
          tLo = Math.max(0, Math.min(ta, tb)); tHi = Math.min(1, Math.max(ta, tb));
          if (tLo > tHi) continue;
        } else if (Math.abs(py - ay) >= radius) continue;
        const xa = ax + tLo * dx, xb = ax + tHi * dx;
        const xs = Math.max(x0, Math.floor(Math.min(xa, xb) - radius - 1)), xe = Math.min(x1, Math.ceil(Math.max(xa, xb) + radius + 1));
        const row = (y - by0) * bw - x0;
        for (let x = xs; x <= xe; x++) {
          const px = x + 0.5;
          const t = len2 > 0 ? Math.min(1, Math.max(0, ((px - ax) * dx + (py - ay) * dy) / len2)) : 0;
          const ex = px - (ax + t * dx), ey = py - (ay + t * dy);
          const d2 = ex * ex + ey * ey;
          if (d2 >= rSq) continue;
          const d = Math.sqrt(d2);
          const w = d <= inner ? 1 : ramp < 1e-9 ? 1 : (radius - d) / ramp;
          if (w > mask[row + x]) mask[row + x] = w;
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
        }
      }
    }
    if (minX > maxX) { yield; continue; }
    // separable blur over [minX..maxX] × [by0..by1], read margin = kernel
    const rw = maxX - minX + 1, rh = by1 - by0 + 1;
    const hy0 = Math.max(0, by0 - kernel), hy1 = Math.min(H - 1, by1 + kernel);
    const hrows = hy1 - hy0 + 1;
    const horizontal = new Uint8ClampedArray(rw * hrows * 4);
    const sums = [0, 0, 0, 0];
    for (let r = 0; r < hrows; r++) {
      const base = (hy0 + r) * W * 4;
      sums[0] = sums[1] = sums[2] = sums[3] = 0;
      for (let k = minX - kernel; k <= minX + kernel; k++) {
        const idx = base + Math.min(W - 1, Math.max(0, k)) * 4;
        sums[0] += src[idx]; sums[1] += src[idx + 1]; sums[2] += src[idx + 2]; sums[3] += src[idx + 3];
      }
      let out = r * rw * 4;
      for (let x = minX; x <= maxX; x++) {
        horizontal[out] = sums[0] / winSize; horizontal[out + 1] = sums[1] / winSize;
        horizontal[out + 2] = sums[2] / winSize; horizontal[out + 3] = sums[3] / winSize;
        out += 4;
        const a = base + Math.min(W - 1, x + kernel + 1) * 4, d = base + Math.max(0, x - kernel) * 4;
        sums[0] += src[a] - src[d]; sums[1] += src[a + 1] - src[d + 1]; sums[2] += src[a + 2] - src[d + 2]; sums[3] += src[a + 3] - src[d + 3];
      }
    }
    // vertical pass + weighted blend into the band's result buffer
    const out = new Uint8ClampedArray(rw * rh * 4);
    const col = new Float64Array(rw * 4);
    for (let y = by0; y <= by1; y++) {
      col.fill(0);
      for (let k = -kernel; k <= kernel; k++) {
        const hr = Math.min(H - 1, Math.max(0, y + k)) - hy0;
        const o = hr * rw * 4;
        for (let i = 0; i < rw * 4; i++) col[i] += horizontal[o + i];
      }
      const mrow = (y - by0) * bw - x0;
      for (let x = minX; x <= maxX; x++) {
        const o = ((y - by0) * rw + (x - minX)) * 4, t = (y * W + x) * 4, c = (x - minX) * 4;
        const weight = stroke.intensity * mask[mrow + x];
        if (weight <= 0) { out[o] = src[t]; out[o + 1] = src[t + 1]; out[o + 2] = src[t + 2]; out[o + 3] = src[t + 3]; continue; }
        for (let q = 0; q < 4; q++) out[o + q] = src[t + q] * (1 - weight) + blurQ(col[c + q] / winSize) * weight;
      }
    }
    results.push({ x: minX, y: by0, w: rw, h: rh, data: out });
    yield;
  }
  for (const r of results) {
    for (let y = 0; y < r.h; y++) {
      const t = ((r.y + y) * W + r.x) * 4;
      raster.data.set(r.data.subarray(y * r.w * 4, (y + 1) * r.w * 4), t);
    }
  }
}

// Reproduces the rounding of the reference's Uint8ClampedArray write (floating value of the
// vertical blur rounded to nearest even then clamped).
const quant = new Uint8ClampedArray(1);
function blurQ(v: number): number { quant[0] = v; return quant[0]; }

// Applies the stroke blur (mutates the raster). Cost proportional to the
// area swept by the brush.
export function applyStroke(raster: Raster, stroke: Stroke): void {
  const job = strokeJob(raster, stroke);
  while (!job.next().done);
}

// Variant that yields to the browser between bands (ms budget per slice).
// Returns false if `cancelled()` becomes true (the raster is then left unmodified).
export async function applyStrokeAsync(raster: Raster, stroke: Stroke, cancelled: () => boolean = () => false, budgetMs = 12): Promise<boolean> {
  const job = strokeJob(raster, stroke);
  let sliceStart = performance.now();
  for (;;) {
    if (cancelled()) return false;
    if (job.next().done) return true;
    if (performance.now() - sliceStart > budgetMs) {
      await new Promise<void>(resolve => setTimeout(resolve, 0));
      sliceStart = performance.now();
    }
  }
}

export function renderDocument(base: Raster, document: StrokeDocument): Raster {
  const raster = cloneRaster(base);
  for (const stroke of document.strokes) applyStroke(raster, stroke);
  return raster;
}

// Length of the common prefix (by identifier and parameters): lets the
// component apply only the new strokes without replaying everything.
export function commonPrefix(applied: readonly Stroke[], next: readonly Stroke[]): number {
  const same = (a: Stroke, b: Stroke) => a.id === b.id && a.size === b.size && a.intensity === b.intensity && a.hardness === b.hardness && a.points.length === b.points.length && a.points.every((p, i) => p.x === b.points[i].x && p.y === b.points[i].y);
  let count = 0;
  while (count < applied.length && count < next.length && same(applied[count], next[count])) count++;
  return count;
}
