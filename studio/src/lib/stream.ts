// Pure, dependency-free helpers for the streamed (virtualized) gallery and
// the character gallery view state. Kept import-free so Node tests can run
// them directly (same pattern as src/lib/gallery.ts).

// Bounded LRU for decoded thumbnail data URLs: scrolling thousands of
// entries keeps at most `capacity` decoded images in memory.
export class LruCache<V> {
  private entries = new Map<string, V>();
  readonly capacity: number;
  constructor(capacity: number) {
    if (!Number.isInteger(capacity) || capacity < 1) {
      throw new Error('Capacité LRU invalide');
    }
    this.capacity = capacity;
  }
  get(key: string): V | undefined {
    const hit = this.entries.get(key);
    if (hit === undefined) return undefined;
    // Refresh recency on every read.
    this.entries.delete(key);
    this.entries.set(key, hit);
    return hit;
  }
  set(key: string, value: V): void {
    this.entries.delete(key);
    this.entries.set(key, value);
    while (this.entries.size > this.capacity) {
      const oldest = this.entries.keys().next();
      if (oldest.done) break;
      this.entries.delete(oldest.value);
    }
  }
  get size(): number {
    return this.entries.size;
  }
}

// Thumbnails are bounded to the sliding window; 250 entries keep visible and
// prefetch rows hot without retaining a long scrolling history.
export const thumbCache = new LruCache<string>(250);

// Previews (1600 px) of the viewer: each entry weighs far more than a grid
// thumbnail, so the prefetch of neighbouring images lives in its own tiny LRU
// (8 entries) — sequential navigation swaps instantly without an unbounded
// RAM cache.
export const previewCache = new LruCache<string>(8);

// Session view state, deliberately outside components so tab navigation
// (cards -> sheet, gallery -> image -> back) can restore the position.
export interface StreamPosition {
  fingerprint: string;
  assetId: string | null;
  index: number;
  scrollTop: number;
  rowOffset?: number;
}
export const streamPosition: { current: StreamPosition | null } = { current: null };
export const peopleView: { selectedId: string | null } = { selectedId: null };

// The stream ignores the pagination offset: two identical filter sets must
// share one saved position whatever the offset field holds.
export function fingerprintFilters(filters: Record<string, unknown>): string {
  const { offset: _ignored, ...rest } = filters;
  return JSON.stringify(rest);
}

// Number of offset windows needed to hold at least `target` items.
export function windowsToCover(target: number, limit: number): number {
  if (!Number.isInteger(limit) || limit < 1) throw new Error('Limite de fenêtre invalide');
  if (target <= 0) return 0;
  return Math.ceil(target / limit);
}

// Stream loading decisions, kept pure so the four V12 audit defects stay
// covered by Node tests (stale queued windows, empty-result loop, refresh,
// deep restore).
export interface StreamLoadState {
  loaded: number;
  total: number | null; // null until the first window of a generation answers
  loading: boolean;
  restoring: boolean;
  error: boolean;
}

export function streamComplete(loaded: number, total: number | null): boolean {
  return total !== null && loaded >= total;
}

// `needed` = index of the last entry that must be present (rendered rows plus
// the prefetch margin). An empty result (total 0) is complete, never refetched.
export function shouldPrefetch(state: StreamLoadState, needed: number): boolean {
  if (state.loading || state.restoring || state.error || state.total === null) return false;
  if (streamComplete(state.loaded, state.total)) return false;
  return needed >= state.loaded;
}

// A window queued before a reset (filter change, refresh) or overtaken by an
// equivalent request must be dropped instead of appended at a stale offset.
export function acceptWindow(requestGeneration: number, currentGeneration: number, requestedOffset: number, loaded: number): boolean {
  return requestGeneration === currentGeneration && requestedOffset === loaded;
}

// Deep restore loads contiguously up to the saved index, bounded by total,
// and stops early once the anchor asset is present. No fixed window cap.
export function restoreNeedsMore(loadedIds: readonly string[], total: number, saved: { assetId: string | null; index: number }): boolean {
  if (loadedIds.length >= total) return false;
  if (saved.assetId && loadedIds.includes(saved.assetId)) return false;
  return loadedIds.length <= saved.index;
}

// Anchor asset first; when it vanished (deleted, filtered out by an
// annotation), fall back to the saved index, clamped to the loaded range.
export function restoreIndex(loadedIds: readonly string[], saved: { assetId: string | null; index: number }): number {
  if (!loadedIds.length) return 0;
  const found = saved.assetId ? loadedIds.indexOf(saved.assetId) : -1;
  return found >= 0 ? found : Math.min(Math.max(0, saved.index), loadedIds.length - 1);
}

export function restoreTop(index:number,columns:number,rowHeight:number,saved:StreamPosition):number {
  return Math.max(0,Math.floor(index/Math.max(1,columns))*rowHeight+Math.min(saved.rowOffset??0,Math.max(0,rowHeight-1)));
}
