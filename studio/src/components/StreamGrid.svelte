<script lang="ts">
  import { t } from '../lib/i18n';
  import { tick } from 'svelte';
  import { call, message, type Asset, type Filters } from '../lib/api';
  import { fingerprintFilters, restoreTop, streamPosition, type StreamPosition } from '../lib/stream';
  import CachedThumb from './CachedThumb.svelte';
  import { reviewLabels } from '../lib/gallery';
  // `refresh` changes after annotations, indexing or associations: the stream
  // reloads around the same anchor.
  // `size` is the minimum thumbnail width (slider in the gallery bar);
  // `currentId` outlines the image shown in the inspector.
  let { filters, onopen, refresh = 0, selectedIds=[],selectionMode=false,size=184,currentId=null }: { filters: Filters; onopen: (asset: Asset) => void; refresh?: number;selectedIds?:string[];selectionMode?:boolean;size?:number;currentId?:string|null } = $props();
  // Metadata is a bounded sliding window around the viewport. Thumbnails stay
  // lazy and LRU-bounded; at most ~250 entries are kept before/after.
  const WINDOW = 250;
  const PREFETCH = 250;
  const MAX_WINDOW = 1000;
  const OVERSCAN = 6;
  const GAP = 8;
  const CAPTION = 22;
  let items = $state<(Asset | null)[]>([]);
  let base = $state(0); // absolute index of items[0]
  let total = $state<number | null>(null);
  let loading = $state(false), restoring = $state(false), error = $state(''), retry = $state(0);
  let requests = $state(0);
  let viewport = $state<HTMLDivElement | null>(null);
  let spacerWidth = $state(0), viewHeight = $state(600), scrollTop = $state(0);
  let initializedFor = '', refreshedFor = 0, generation = 0;
  const fingerprint = $derived(fingerprintFilters({ ...filters }));
  const metrics = $derived.by(() => {
    const columns = Math.max(1, Math.floor((spacerWidth + GAP) / Math.max(80, size)));
    const cardWidth = Math.max(80, Math.floor((spacerWidth - GAP * (columns - 1)) / Math.max(1, columns)));
    const imageHeight = Math.round(cardWidth * 1.25);
    return { columns, cardWidth, imageHeight, rowHeight: imageHeight + CAPTION + GAP };
  });
  const totalRows = $derived(Math.ceil((total ?? 0) / Math.max(1, metrics.columns)));
  const startRow = $derived(Math.max(0, Math.floor(scrollTop / Math.max(1, metrics.rowHeight)) - OVERSCAN));
  const endRow = $derived(Math.min(totalRows, Math.ceil((scrollTop + viewHeight) / Math.max(1, metrics.rowHeight)) + OVERSCAN));
  const renderedRows = $derived(Array.from({ length: Math.max(0, endRow - startRow) }, (_, i) => startRow + i));
  const loadedCount = $derived(items.filter(Boolean).length);
  const allLoaded = $derived(total !== null && base === 0 && items.length >= total);
  let chain: Promise<void> = Promise.resolve();
  function merge(offset: number, page: Asset[], pageTotal: number) {
    const start = Math.min(base, offset);
    const end = Math.max(base + items.length, offset + page.length);
    const merged: (Asset | null)[] = Array(Math.max(0, end - start)).fill(null);
    for (let i = 0; i < items.length; i++) merged[base + i - start] = items[i];
    for (let i = 0; i < page.length; i++) merged[offset + i - start] = page[i];
    base = start; items = merged; total = pageTotal;
  }
  function fetchWindow(offset: number, limit = WINDOW, replace = false): Promise<void> {
    const gen = generation;
    loading = true;
    const run = chain.then(async () => {
      if (gen !== generation) return;
      error = ''; requests++;
      try {
        const page = await call<{ items: Asset[]; total: number }>('gallery_window', { filters: { ...$state.snapshot(filters), offset }, limit });
        if (gen !== generation) return;
        if (replace) { base = offset; items = []; }
        merge(offset, page.items, page.total);
        if (!page.items.length && offset >= page.total) total = page.total;
      } catch (e) { if (gen === generation) error = message(e); }
      finally { if (gen === generation) loading = false; }
    });
    chain = run.then(() => undefined, () => undefined);
    return run;
  }
  async function restore(saved: StreamPosition) {
    restoring = true;
    const gen = generation;
    try {
      let index = saved.index;
      if (saved.assetId) {
        try {
          const n = await call<{ position: number; total: number; found: boolean }>('gallery_neighbors', { filters: { ...$state.snapshot(filters), offset: 0 }, id: saved.assetId, fallback: saved.index });
          if (gen !== generation) return;
          total = n.total;
          index = n.position;
        } catch { /* fall back to the saved index */ }
      }
      const start = Math.max(0, index - Math.floor(WINDOW / 2));
      await fetchWindow(start, WINDOW);
      if (gen !== generation) return;
      await tick();
      const node = viewport;
      if (!node) return;
      const target = saved.assetId || saved.index ? restoreTop(index, metrics.columns, metrics.rowHeight, saved) : saved.scrollTop;
      node.scrollTo({ top: Math.max(0, target), behavior: 'instant' as ScrollBehavior });
      scrollTop = node.scrollTop;
      trim();
    } finally { if (gen === generation) restoring = false; }
  }
  function reset(saved: StreamPosition | null) {
    ++generation;
    items = []; base = 0; total = null; error = ''; loading = false; restoring = false; scrollTop = 0;
    if (saved && (saved.index > 0 || saved.scrollTop > 0)) restore(saved);
    else fetchWindow(0);
  }
  function trim() {
    if (!items.length || total === null) return;
    const columns = Math.max(1, metrics.columns);
    const first = Math.floor(scrollTop / Math.max(1, metrics.rowHeight)) * columns;
    const rows = Math.ceil(viewHeight / Math.max(1, metrics.rowHeight)) + OVERSCAN * 2;
    const keepStart = Math.max(0, first - PREFETCH);
    const keepEnd = Math.min(total, first + PREFETCH + rows * columns);
    if (keepStart < base + items.length && base < keepStart) {
      const drop = keepStart - base;
      items = items.slice(drop); base += drop;
    }
    if (keepEnd > base && base + items.length > keepEnd) {
      items = items.slice(0, Math.max(0, keepEnd - base));
    }
    // Hard cap in case a viewport is very tall or a resize races the scroll.
    if (items.length > MAX_WINDOW) {
      const drop = items.length - MAX_WINDOW;
      base += drop; items = items.slice(drop);
    }
  }
  $effect(() => {
    const node = viewport;
    if (!node) return;
    const update = () => { spacerWidth = node.clientWidth; viewHeight = node.clientHeight; };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  });
  $effect(() => {
    const fp = fingerprint;
    if (fp === initializedFor) return;
    initializedFor = fp;
    refreshedFor = refresh;
    const saved = streamPosition.current;
    if (saved && saved.fingerprint !== fp) streamPosition.current = null;
    reset(saved && saved.fingerprint === fp ? saved : null);
  });
  $effect(() => {
    const value = refresh;
    if (value === refreshedFor || !initializedFor) return;
    refreshedFor = value;
    const saved = streamPosition.current;
    reset(saved && saved.fingerprint === initializedFor ? saved : null);
  });
  // Keep the viewport covered, with ~PREFETCH entries before and after.
  $effect(() => {
    void retry;
    if (loading || restoring || error || total === null) return;
    if (total === 0) return;
    const columns = Math.max(1, metrics.columns);
    const first = Math.floor(scrollTop / Math.max(1, metrics.rowHeight)) * columns;
    const last = Math.ceil((scrollTop + viewHeight) / Math.max(1, metrics.rowHeight)) * columns;
    const needStart = Math.max(0, first - PREFETCH);
    const needEnd = Math.min(total - 1, last + PREFETCH);
    // A jump outside the loaded window (or an empty trimmed window) must
    // refocus around the viewport, never append/trim to an empty range.
    if (items.length === 0 || first < base || first >= base + items.length) {
      const target = Math.max(0, first - Math.floor(WINDOW / 2));
      fetchWindow(target, WINDOW, true);
    } else if (needStart < base) fetchWindow(Math.max(0, needStart - Math.floor(WINDOW / 2)));
    else if (needEnd >= base + items.length && base + items.length < total) fetchWindow(base + items.length);
  });
  function onScroll() {
    const node = viewport;
    if (!node || !items.length) return;
    scrollTop = node.scrollTop;
    trim();
    const columns = Math.max(1, metrics.columns);
    const first = Math.floor(node.scrollTop / Math.max(1, metrics.rowHeight)) * columns;
    const asset = items[first - base] ?? null;
    streamPosition.current = { fingerprint, assetId: asset?.id ?? null, index: first, scrollTop: node.scrollTop, rowOffset: node.scrollTop % metrics.rowHeight };
  }
  function assetAt(index: number): Asset | null {
    return items[index - base] ?? null;
  }
</script>
<div class="stream">
<div class="stream-status" role="status" data-stream-status data-loaded={loadedCount} data-base={base} data-total={total ?? ''} data-complete={allLoaded} data-requests={requests}>
  {#if restoring}{$t("Retour à votre position…")}{:else if loading}{$t("Chargement…")}{/if}
</div>
{#if error}<p class="error stream-error" role="alert">{error} <button class="quiet" onclick={() => { error = ''; retry++; if (total === null) reset(null); }}>{$t("Réessayer")}</button></p>{/if}
<div class="stream-viewport" bind:this={viewport} onscroll={onScroll} aria-busy={loading || restoring}>
  <div class="stream-spacer" style="height:{totalRows * metrics.rowHeight}px">
    {#each renderedRows as row (row)}
      <div class="stream-row" style="top:{row * metrics.rowHeight}px;height:{metrics.rowHeight - GAP}px;gap:{GAP}px">
        {#each Array.from({ length: metrics.columns }, (_, c) => row * metrics.columns + c) as index (index)}
          {@const asset = assetAt(index)}
          {#if asset}
            <button class="image-card stream-card" class:selected={selectedIds.includes(asset.id)} class:current={currentId === asset.id} aria-pressed={selectionMode?selectedIds.includes(asset.id):undefined} data-stream-asset={asset.id} style="width:{metrics.cardWidth}px" onclick={() => onopen(asset)} title={asset.path}>
              <div class="stream-thumb" style="height:{metrics.imageHeight}px"><CachedThumb id={asset.id} alt={asset.path} /></div>
              <div class="caption"><span>{asset.favorite ? '★ ' : ''}{asset.path.split(/[\\/]/).pop()?.replace(/\.(png|jpe?g|webp)$/i,'')}</span><small class="num">{asset.width ?? '?'} × {asset.height ?? '?'}{asset.review !== 'unreviewed' ? ` · ${$t(reviewLabels[asset.review] ?? asset.review)}` : ''}{asset.metadataState === 'absent' ? ' · ' + $t("sans métadonnées") : ''}</small></div>
            </button>
          {:else if total === null || index < total}
            <div class="image-card stream-placeholder" style="width:{metrics.cardWidth}px;height:{metrics.imageHeight}px" aria-hidden="true"></div>
          {/if}
        {/each}
      </div>
    {/each}
  </div>
  {#if total === 0 && !loading && !restoring && !error}<div class="empty-state">{$t("Aucune image dans cette sélection.")}</div>{/if}
</div>
</div>
<style>
  .stream{position:relative;display:flex;flex-direction:column;flex:1;min-height:0;height:100%}
  .stream-status{position:absolute;z-index:2;inset-inline-end:14px;bottom:10px;margin:0;font-size:11px;color:var(--muted);pointer-events:none}
  .stream-status:empty{display:none}
  .stream-error{margin:0;padding:6px 12px;border-bottom:1px solid var(--line);background:var(--bg)}
  .stream-viewport{flex:1;height:auto;min-height:0;margin:0;border:0;border-radius:0;background:var(--canvas);overflow:auto;padding:12px 12px 0}
  .stream-row{padding:0}
  .stream-card{display:flex;flex-direction:column;gap:0;padding:0;min-height:0;border:0;border-radius:var(--r-img);background:transparent;overflow:visible;text-align:start}
  .stream-card:hover:not(:disabled){background:transparent}
  .stream-thumb{width:100%;border-radius:var(--r-img);overflow:hidden;background:var(--canvas)}
  .stream-thumb :global(img){width:100%;height:100%;object-fit:contain;display:block}
  .stream-thumb :global(.image-placeholder){height:100%;min-height:0;font-size:11px;color:var(--faint)}
  .stream-card.current .stream-thumb{outline:1px solid var(--sel);outline-offset:2px}
  .stream-card.selected .stream-thumb{outline:2px solid var(--accent);outline-offset:2px}
  .stream-card.selected{outline:0}
  .stream-card:focus-visible{outline:1px solid var(--sel);outline-offset:2px}
  .caption{display:flex;align-items:baseline;gap:6px;height:22px;padding:4px 2px 0;min-width:0;color:var(--muted);font-size:11px;line-height:16px}
  .caption span{flex:1;min-width:0;padding:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:11px;line-height:16px}
  /* Details take no room at rest, so a long "· sans métadonnées" never squeezes the name. */
  .caption small{display:none;flex:0 1 auto;max-width:60%;min-width:0;padding:0;font-size:10.5px;line-height:16px;color:var(--faint);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .stream-card:hover .caption{color:var(--text)}
  .stream-card:hover .caption small,.stream-card:focus-visible .caption small{display:block}
  .stream-placeholder{border:0;border-radius:var(--r-img);background:var(--panel);opacity:.5}
  .empty-state{padding:80px 20px;text-align:center;color:var(--muted)}
</style>
