// Real-time stroke preview at screen resolution: pure functions with no DOM
// (Node --test). The component draws the visible area at screen scale,
// applies the scaled stroke blur on that small window, then
// overlays the result; the full-resolution application is still done on
// release through the existing path (blur.ts).

import type { Stroke, StrokePoint } from './strokes';
import type { View } from './coords';

// Stroke transposed into screen space: diameter and points follow the zoom,
// the screen origin (pan) offsets the points. The screen stroke stays bounded by the
// same guarantees as the model (finiteness); input points are validated.
export function scaleStroke(stroke: Stroke, view: View): Stroke {
  if (!(view.zoom > 0) || !Number.isFinite(view.x) || !Number.isFinite(view.y)) {
    throw new Error('Vue de prévisualisation invalide.');
  }
  const points: StrokePoint[] = stroke.points.map((point) => ({
    x: point.x * view.zoom + view.x,
    y: point.y * view.zoom + view.y,
  }));
  return { ...stroke, size: stroke.size * view.zoom, points };
}

// Image window visible on screen, clipped to the image bounds: the source
// area to copy into the preview canvas. Null if nothing is visible.
export function visibleWindow(
  view: View,
  canvasWidth: number,
  canvasHeight: number,
  imageWidth: number,
  imageHeight: number,
): { x: number; y: number; w: number; h: number } | null {
  if (canvasWidth <= 0 || canvasHeight <= 0 || !(view.zoom > 0)) return null;
  const x0 = Math.max(0, Math.floor(-view.x / view.zoom));
  const y0 = Math.max(0, Math.floor(-view.y / view.zoom));
  const x1 = Math.min(imageWidth, Math.ceil((canvasWidth - view.x) / view.zoom));
  const y1 = Math.min(imageHeight, Math.ceil((canvasHeight - view.y) / view.zoom));
  if (x0 >= x1 || y0 >= y1) return null;
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

// Estimated cost of a preview frame: proportional to the screen area
// swept by the brush, not to the image resolution. Used for the
// before/after measurement (full resolution = same formula without zoom, image radius).
export function previewBudget(stroke: Stroke, view: View): number {
  const screenRadius = (stroke.size * view.zoom) / 2;
  let length = 0;
  for (let i = 1; i < stroke.points.length; i++) {
    length += Math.hypot(stroke.points[i].x - stroke.points[i - 1].x, stroke.points[i].y - stroke.points[i - 1].y);
  }
  const screenLength = length * view.zoom;
  const area = 2 * screenRadius * screenLength + Math.PI * screenRadius * screenRadius;
  return Math.max(0, Math.round(area));
}
