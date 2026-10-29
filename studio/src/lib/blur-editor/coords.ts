// Screen → image pixel mapping, independent of the DOM: tested outside the browser.
// Convention: screen position = image position × zoom + (x, y).

export interface View { zoom: number; x: number; y: number }

export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 16;

export function screenToImage(sx: number, sy: number, view: View): { x: number; y: number } {
  return { x: (sx - view.x) / view.zoom, y: (sy - view.y) / view.zoom };
}
export function imageToScreen(ix: number, iy: number, view: View): { x: number; y: number } {
  return { x: ix * view.zoom + view.x, y: iy * view.zoom + view.y };
}

export function fitView(imageWidth: number, imageHeight: number, viewportWidth: number, viewportHeight: number, margin = 24): View {
  if (!(imageWidth > 0) || !(imageHeight > 0) || viewportWidth <= margin * 2 || viewportHeight <= margin * 2) return { zoom: 1, x: 0, y: 0 };
  const zoom = Math.min((viewportWidth - margin * 2) / imageWidth, (viewportHeight - margin * 2) / imageHeight, MAX_ZOOM);
  return { zoom, x: (viewportWidth - imageWidth * zoom) / 2, y: (viewportHeight - imageHeight * zoom) / 2 };
}

export function zoomAt(view: View, screenX: number, screenY: number, factor: number, min = MIN_ZOOM, max = MAX_ZOOM): View {
  const zoom = Math.min(max, Math.max(min, view.zoom * factor));
  if (zoom === view.zoom) return view;
  const point = screenToImage(screenX, screenY, view);
  return { zoom, x: screenX - point.x * zoom, y: screenY - point.y * zoom };
}

// The image stays at least partially visible; an image smaller than the
// view is re-centred rather than panned.
export function clampPan(view: View, imageWidth: number, imageHeight: number, viewportWidth: number, viewportHeight: number): View {
  let x = view.x, y = view.y;
  const width = imageWidth * view.zoom, height = imageHeight * view.zoom;
  x = width <= viewportWidth ? (viewportWidth - width) / 2 : Math.min(0, Math.max(viewportWidth - width, x));
  y = height <= viewportHeight ? (viewportHeight - height) / 2 : Math.min(0, Math.max(viewportHeight - height, y));
  return { zoom: view.zoom, x, y };
}
