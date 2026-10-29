<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { extensionFor, extensionMismatch } from '../lib/ui-helpers';
  // NON-destructive image editor: the original is loaded read-only,
  // blur strokes are replayed on a copy, export creates a NEW
  // file via editor_export (studio-core refuses to overwrite).
  import { onMount } from 'svelte';
  import { call, message, native } from '../lib/api';
  import Icon from './Icon.svelte';
  import { screenToImage, fitView, zoomAt, clampPan, MIN_ZOOM, MAX_ZOOM } from '../lib/blur-editor/coords';
  import { createStrokeId } from '../lib/blur-editor/strokes';
  import type { Stroke, StrokeDocument } from '../lib/blur-editor/strokes';
  import { initialHistory, pushHistory, undoHistory, redoHistory, canUndo, canRedo, HISTORY_LIMIT } from '../lib/blur-editor/history';
  import type { HistoryState } from '../lib/blur-editor/history';
  import { applyStrokeAsync, commonPrefix, strokeWindow, translateStroke } from '../lib/blur-editor/blur';
  import type { StrokeWindow } from '../lib/blur-editor/blur';
  import type { Raster } from '../lib/blur-editor/blur';
  import { scaleStroke, visibleWindow } from '../lib/blur-editor/preview';

  let { target = null, onclose = () => {} }: { target?: { id: string; path: string } | null; onclose?: () => void } = $props();

  let status = $state<'loading' | 'ready' | 'error'>('loading');
  let error = $state('');
  let notice = $state(''), exportError = $state('');
  let source = $state<{ path: string; width: number; height: number } | null>(null);
  let size = $state(60), intensity = $state(0.7), hardness = $state(0.5);
  let view = $state({ zoom: 1, x: 0, y: 0 });
  let cursor = $state<{ x: number; y: number } | null>(null);
  let comparing = $state(false), holding = $state(false);
  let showOriginal = $derived(comparing || holding);
  let history = $state<HistoryState<StrokeDocument>>(initialHistory({ version: 1, strokes: [] }));
  let destination = $state(''), format = $state<'png' | 'jpeg'>('png'), quality = $state(0.9), exporting = $state(false);
  let frame = $state<HTMLCanvasElement | undefined>(), host = $state<HTMLDivElement | undefined>();

  // Outside reactivity: rasters and working canvases, immutable between two renders.
  let base: Raster | null = null, baseImage: HTMLImageElement | null = null, work: HTMLCanvasElement | null = null;
  let applied: Stroke[] = [], drawing: Stroke | null = null;
  let panning: { sx: number; sy: number; x: number; y: number } | null = null;
  let dirty = true, rafId = 0, syncing = false;
  let processing = $state(false);
  let pendingFit = false;
  // Real-time preview: screen-resolution canvas recomputed during the
  // stroke, overlaid on the render; the full-resolution application stays
  // done on release (sync). Instrumentation: latency measured per preview
  // frame and per full-resolution application (status + __SAYA_BLUR_PERF__).
  let previewCanvas: HTMLCanvasElement | null = null;
  let previewWin: StrokeWindow | null = null, previewValid = false, previewGeneration = 0;
  let previewKey = '', previewBusy = false;
  // CPU-memory preview base: a single canvas read per working version
  // (the full-surface getImageData readback otherwise dominates the cost
  // of each frame under WebKitGTK). Later frames only copy
  // the brush window from this base.
  let previewBase: ImageData | null = null, previewBaseVersion = -1, previewBaseMs = 0;
  let workVersion = 0;
  let previewReadout = $state('');
  let perf = { previewFrames: 0, previewMsTotal: 0, previewMsLast: 0, commitMsLast: 0, previewPixels: 0, baseMs: 0 };
  function publishPerf() {
    const average = perf.previewFrames ? perf.previewMsTotal / perf.previewFrames : 0;
    previewReadout = perf.previewFrames
      ? $t("aperçu {mpx} Mpx · {ms} ms/trame (moy. {avg})", { mpx: Math.round(perf.previewPixels / 10000) / 100, ms: perf.previewMsLast.toFixed(1), avg: average.toFixed(1) })
      : '';
    (window as unknown as { __SAYA_BLUR_PERF__?: typeof perf & { previewMsAvg: number } }).__SAYA_BLUR_PERF__ = {
      ...perf,
      previewMsAvg: average,
    };
  }
  function invalidate() { dirty = true; }

  $effect(() => { const id = target?.id; if (id && native) void openImage(id); });
  $effect(() => { void showOriginal; void cursor; invalidate(); });
  $effect(() => {
    const canvas = frame, box = host;
    if (!canvas || !box) return;
    const observer = new ResizeObserver(resizeCanvas);
    observer.observe(box);
    const wheel = (event: WheelEvent) => {
      if (!base) return;
      event.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const factor = event.deltaY < 0 ? 1.1 : 1 / 1.1;
      view = clampPan(zoomAt(view, event.clientX - rect.left, event.clientY - rect.top, factor, MIN_ZOOM, MAX_ZOOM), base.width, base.height, canvas.clientWidth, canvas.clientHeight);
    };
    canvas.addEventListener('wheel', wheel, { passive: false });
    resizeCanvas();
    return () => { observer.disconnect(); canvas.removeEventListener('wheel', wheel); };
  });

  async function openImage(id: string) {
    status = 'loading'; error = ''; notice = ''; exportError = '';
    try {
      const info = await call<{ path: string; width: number; height: number; data: string }>('editor_load', { id });
      const image = new Image();
      await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error($t("Décodage de l’image impossible dans le canevas.")));
        image.src = info.data;
      });
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      if (!context) throw new Error($t("Canevas 2D indisponible."));
      context.drawImage(image, 0, 0);
      const data = context.getImageData(0, 0, canvas.width, canvas.height);
      base = { width: data.width, height: data.height, data: data.data };
      baseImage = image; work = canvas; applied = []; drawing = null; panning = null;
      previewCanvas = null; previewWin = null; previewValid = false; previewKey = '';
      previewBase = null; previewBaseVersion = -1;
      Object.assign(perf, { previewFrames: 0, previewMsTotal: 0, previewMsLast: 0, commitMsLast: 0, previewPixels: 0, baseMs: 0 });
      publishPerf();
      history = initialHistory({ version: 1, strokes: [] }, HISTORY_LIMIT);
      const stored = await call<{ document: StrokeDocument | null }>('blur_document', { asset: id });
      if (stored.document) history = initialHistory(stored.document, HISTORY_LIMIT);
      source = { path: info.path, width: info.width, height: info.height };
      destination = suggestDestination(info.path);
      pendingFit = true;
      view = fitView(base.width, base.height, host?.clientWidth || frame?.clientWidth || 800, host?.clientHeight || frame?.clientHeight || 520);
      status = 'ready'; invalidate();
    } catch (e) { status = 'error'; error = message(e); }
  }

  function onFormatChange() { destination = extensionFor(destination, format); }

  function suggestDestination(path: string): string {
    const dot = path.lastIndexOf('.'), separator = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    const stem = dot > separator ? path.slice(0, dot) : path;
    return `${stem} - retouche.png`;
  }

  function resizeCanvas() {
    if (!frame || !host) return;
    const width = host.clientWidth, height = host.clientHeight;
    if (width > 0 && height > 0 && (frame.width !== width || frame.height !== height)) {
      frame.width = width; frame.height = height;
      if (base) {
        view = pendingFit ? fitView(base.width, base.height, width, height) : clampPan(view, base.width, base.height, width, height);
        pendingFit = false;
      }
      invalidate();
    }
  }

  function paint() {
    const canvas = frame, context = canvas?.getContext('2d');
    if (!canvas || !context || !work || !baseImage) return;
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.imageSmoothingEnabled = view.zoom < 1;
    context.setTransform(view.zoom, 0, 0, view.zoom, view.x, view.y);
    context.drawImage(showOriginal ? baseImage : work, 0, 0);
    context.setTransform(1, 0, 0, 1, 0, 0);
    if (drawing && !showOriginal && previewValid && previewCanvas && previewWin) {
      // Live blur preview: window recomputed at screen resolution,
      // overlaid exactly where the full-resolution application will write.
      context.drawImage(previewCanvas, previewWin.x, previewWin.y, previewWin.w, previewWin.h, previewWin.x, previewWin.y, previewWin.w, previewWin.h);
    }
    if (cursor) {
      const radius = Math.max(2, (size / 2) * view.zoom);
      context.lineWidth = 1.5;
      context.strokeStyle = 'rgba(0,0,0,0.8)';
      context.beginPath(); context.arc(cursor.x, cursor.y, radius + 1, 0, Math.PI * 2); context.stroke();
      context.strokeStyle = '#e8f6ee';
      context.beginPath(); context.arc(cursor.x, cursor.y, radius, 0, Math.PI * 2); context.stroke();
    }
    if (drawing) {
      context.strokeStyle = 'rgba(232,246,238,0.7)';
      context.setLineDash([4, 4]);
      context.beginPath();
      drawing.points.forEach((point, index) => {
        const sx = point.x * view.zoom + view.x, sy = point.y * view.zoom + view.y;
        if (index === 0) context.moveTo(sx, sy); else context.lineTo(sx, sy);
      });
      context.stroke();
      context.setLineDash([]);
    }
  }

  function pointerDown(event: PointerEvent) {
    if (!base || status !== 'ready' || !frame) return;
    frame.setPointerCapture(event.pointerId);
    if (event.button === 1 || event.shiftKey) {
      panning = { sx: event.clientX, sy: event.clientY, x: view.x, y: view.y };
      return;
    }
    if (event.button !== 0) return;
    const rect = frame.getBoundingClientRect();
    const point = screenToImage(event.clientX - rect.left, event.clientY - rect.top, view);
    drawing = { id: createStrokeId(), size, intensity, hardness, points: [point] };
    previewValid = false; previewKey = ''; previewGeneration += 1;
    invalidate();
  }
  function pointerMove(event: PointerEvent) {
    if (!base || status !== 'ready' || !frame) return;
    const rect = frame.getBoundingClientRect();
    const sx = event.clientX - rect.left, sy = event.clientY - rect.top;
    cursor = { x: sx, y: sy };
    if (panning) {
      view = clampPan({ zoom: view.zoom, x: panning.x + (event.clientX - panning.sx), y: panning.y + (event.clientY - panning.sy) }, base.width, base.height, rect.width, rect.height);
    } else if (drawing) {
      const point = screenToImage(sx, sy, view);
      const last = drawing.points[drawing.points.length - 1];
      if (Math.hypot(point.x - last.x, point.y - last.y) * view.zoom > 2) drawing = { ...drawing, points: [...drawing.points, point] };
    }
    invalidate();
  }
  function pointerUp() {
    panning = null;
    if (!drawing) return;
    const stroke = drawing;
    drawing = null;
    commit(stroke);
  }
  async function persistDocument() {
    if (!target?.id) return;
    try { await call('blur_save', { asset: target.id, document: history.current }); }
    catch (e) { notice = $t("Sauvegarde des tracés impossible : {error}", { error: message(e) }); }
  }
  function commit(stroke: Stroke) {
    if (!work) return;
    history = pushHistory(history, { version: 1, strokes: [...history.current.strokes, stroke] }, HISTORY_LIMIT);
    void persistDocument();
    void sync();
  }
  function undo() { if (canUndo(history)) { history = undoHistory(history); void persistDocument(); void sync(); } }
  function redo() { if (canRedo(history)) { history = redoHistory(history); void persistDocument(); void sync(); } }
  function clearAll() { if (history.current.strokes.length) { history = pushHistory(history, { version: 1, strokes: [] }, HISTORY_LIMIT); void persistDocument(); void sync(); } }

  // Real-time preview of the current stroke, at screen resolution: the visible
  // base is snapshotted once per working version, each frame
  // copies the stroke window from this base (CPU, no readback), applies
  // the scaled stroke blur to it, then a single small
  // putImageData. A stale computation (new point, zoom, release) is
  // abandoned without writing via its generation number.
  async function computePreview() {
    const stroke = drawing;
    if (!stroke || !work || !frame || status !== 'ready' || !base) { previewValid = false; return; }
    const pw = frame.width, ph = frame.height;
    if (!(pw > 0) || !(ph > 0)) return;
    const key = `${stroke.id}|${stroke.points.length}|${view.zoom}|${view.x}|${view.y}`;
    if (key === previewKey && previewValid) return;
    if (previewBusy) return;
    previewBusy = true;
    previewKey = key;
    try {
      if (!previewCanvas || previewCanvas.width !== pw || previewCanvas.height !== ph) {
        previewCanvas = document.createElement('canvas');
        previewCanvas.width = pw; previewCanvas.height = ph;
        previewBase = null;
      }
      if (!previewBase || previewBaseVersion !== workVersion) {
        const context = previewCanvas.getContext('2d', { willReadFrequently: true });
        if (!context) return;
        context.setTransform(1, 0, 0, 1, 0, 0);
        context.clearRect(0, 0, pw, ph);
        context.imageSmoothingEnabled = view.zoom < 1;
        context.setTransform(view.zoom, 0, 0, view.zoom, view.x, view.y);
        context.drawImage(work, 0, 0);
        context.setTransform(1, 0, 0, 1, 0, 0);
        const baseStart = performance.now();
        previewBase = context.getImageData(0, 0, pw, ph);
        previewBaseVersion = workVersion;
        previewBaseMs = performance.now() - baseStart;
      }
      const scaled = scaleStroke(stroke, view);
      const win = strokeWindow(scaled, pw, ph);
      if (!win) { previewValid = false; return; }
      const start = performance.now();
      const scratch = new ImageData(win.w, win.h);
      for (let row = 0; row < win.h; row++) {
        const from = ((win.y + row) * pw + win.x) * 4;
        scratch.data.set(previewBase.data.subarray(from, from + win.w * 4), row * win.w * 4);
      }
      const copyDone = performance.now();
      const generation = ++previewGeneration;
      const done = await applyStrokeAsync(
        { width: scratch.width, height: scratch.height, data: scratch.data },
        translateStroke(scaled, -win.x, -win.y),
        () => generation !== previewGeneration || drawing !== stroke,
      );
      if (!done || generation !== previewGeneration || drawing !== stroke) return;
      const blurDone = performance.now();
      const context = previewCanvas.getContext('2d');
      context?.putImageData(scratch, win.x, win.y);
      previewWin = win; previewValid = true;
      perf.previewFrames += 1;
      perf.previewMsLast = performance.now() - start;
      perf.previewMsTotal += perf.previewMsLast;
      perf.previewPixels = win.w * win.h;
      perf.baseMs = Math.round(previewBaseMs * 10) / 10;
      publishPerf();
      (window as unknown as { __SAYA_BLUR_PARTS__?: string }).__SAYA_BLUR_PARTS__ =
        `win=${win.w}x${win.h} copy=${(copyDone - start).toFixed(1)}ms blur=${(blurDone - copyDone).toFixed(1)}ms put=${(performance.now() - blurDone).toFixed(1)}ms`;
      invalidate();
    } finally { previewBusy = false; }
  }

  // Synchronizes the working canvas with the current history. One loop
  // at a time; the history is re-read after each stroke, so undo/redo/new
  // stroke during a computation are taken into account without double application: a
  // result is only written if it still matches the history.
  async function sync() {    if (syncing) return;
    syncing = true; processing = true;
    try {
      while (base && work) {
        const canvas = work, origin = base;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) return;
        const strokes = history.current.strokes;
        const prefix = commonPrefix(applied, strokes);
        if (prefix === applied.length && prefix === strokes.length) break;
        const stale = () => work !== canvas || commonPrefix(history.current.strokes, strokes) !== strokes.length || history.current.strokes.length !== strokes.length;
        if (prefix === applied.length) {
          // Next stroke only: read/write of its window, not of the whole image.
          const stroke = strokes[prefix];
          const win = strokeWindow(stroke, canvas.width, canvas.height);
          if (win) {
            const start = performance.now();
            const data = context.getImageData(win.x, win.y, win.w, win.h);
            const done = await applyStrokeAsync({ width: data.width, height: data.height, data: data.data }, translateStroke(stroke, -win.x, -win.y), () => work !== canvas || history.current.strokes[prefix] !== stroke);
            if (work !== canvas) return;
            if (!done || history.current.strokes[prefix] !== stroke || commonPrefix(applied, history.current.strokes) !== applied.length) continue;
            context.putImageData(data, win.x, win.y);
            // Full-resolution application latency of a stroke (on release).
            perf.commitMsLast = performance.now() - start;
            publishPerf();
          }
          applied = [...applied, stroke];
          workVersion += 1;
        } else {
          // Step back: replay from the off-canvas base, then write the modified area.
          const data = new ImageData(new Uint8ClampedArray(origin.data), origin.width, origin.height);
          const raster = { width: data.width, height: data.height, data: data.data };
          let ok = true;
          for (const stroke of strokes) {
            if (!(await applyStrokeAsync(raster, stroke, stale))) { ok = false; break; }
          }
          if (work !== canvas) return;
          if (!ok || stale()) continue;
          let zone: StrokeWindow | null = null;
          for (const stroke of [...applied.slice(prefix), ...strokes.slice(prefix)]) {
            const w = strokeWindow(stroke, canvas.width, canvas.height);
            if (!w) continue;
            zone = zone ? unionWindows(zone, w) : w;
          }
          if (zone) context.putImageData(data, 0, 0, zone.x, zone.y, zone.w, zone.h);
          applied = [...strokes];
          workVersion += 1;
        }
        invalidate();
        await new Promise<void>(resolve => setTimeout(resolve, 0));
      }
    } finally { syncing = false; processing = false; invalidate(); }
  }
  function unionWindows(a: StrokeWindow, b: StrokeWindow): StrokeWindow {
    const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y);
    return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
  }
  async function settle() { while (syncing) await new Promise<void>(resolve => setTimeout(resolve, 20)); }

  function zoomBy(factor: number) {
    if (!frame || !base) return;
    const width = frame.clientWidth, height = frame.clientHeight;
    view = clampPan(zoomAt(view, width / 2, height / 2, factor, MIN_ZOOM, MAX_ZOOM), base.width, base.height, width, height);
  }
  function fit() { if (frame && base) view = fitView(base.width, base.height, frame.clientWidth, frame.clientHeight); }

  async function exportImage() {
    if (!work || !target || exporting || !destination.trim()) return;
    notice = ''; exportError = '';
    if (extensionMismatch(destination.trim(), format)) { exportError = $t("L’extension de la destination ne correspond pas au format {format}.", { format: format === 'png' ? 'PNG' : 'JPEG' }); return; }
    exporting = true;
    try {
      await settle();
      const url = work.toDataURL(format === 'png' ? 'image/png' : 'image/jpeg', quality);
      const result = await call<{ destination: string }>('editor_export', { id: target.id, destination, format, image: url.slice(url.indexOf(',') + 1) });
      notice = $t("Image exportée vers {path}. L'original est intact.", { path: result.destination });
    } catch (e) { exportError = message(e); } finally { exporting = false; }
  }

  onMount(() => {
    const key = (event: KeyboardEvent) => {
      const tag = (event.target as HTMLElement | null)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const keyName = event.key.toLowerCase();
      if (keyName === 'z' && !event.shiftKey) { event.preventDefault(); undo(); }
      else if (keyName === 'y' || (keyName === 'z' && event.shiftKey)) { event.preventDefault(); redo(); }
    };
    window.addEventListener('keydown', key);
    const loop = () => {
      if (dirty && status === 'ready') { dirty = false; paint(); }
      if (drawing && status === 'ready' && !showOriginal) void computePreview();
      rafId = requestAnimationFrame(loop);
    };
    rafId = requestAnimationFrame(loop);
    return () => { cancelAnimationFrame(rafId); window.removeEventListener('keydown', key); };
  });
</script>

<div class="wb editor-wb">
  <aside class="wb-panel left">
    <div class="wb-head"><button class="quiet back" title={$t("Retour à la galerie")} onclick={onclose}><Icon name="left"/>{$t("Retour à la galerie")}</button></div>
    {#if source}
    <div class="wb-scroll">
      <details class="sec" open>
        <summary><span class="sec-title">{$t("Pinceau de flou")}</span><span class="chip num">{size} px</span></summary>
        <div class="sec-body">
          <label class="slider">{$t("Taille du pinceau")}<input type="range" min="4" max="400" bind:value={size} /><span class="muted num">{$t("{n} px image", { n: size })}</span></label>
          <label class="slider">{$t("Intensité du flou")}<input type="range" min="0.05" max="1" step="0.05" bind:value={intensity} /><span class="muted num">{Math.round(intensity * 100)} %</span></label>
          <label class="slider">{$t("Dureté du bord")}<input type="range" min="0" max="1" step="0.05" bind:value={hardness} /><span class="muted num">{Math.round(hardness * 100)} %</span></label>
        </div>
      </details>
      <details class="sec" open>
        <summary title={$t("Maj + glisser ou clic central : déplacer · molette : zoom")}><span class="sec-title">{$t("Vue")}</span><span class="chip num">{Math.round(view.zoom * 100)} %</span></summary>
        <div class="sec-body">
          <div class="btn-row"><button onclick={() => zoomBy(1.25)}>{$t("Zoom +")}</button><button onclick={() => zoomBy(0.8)}>{$t("Zoom −")}</button><button onclick={fit}>{$t("Ajuster")}</button></div>
          <label class="checkbox"><input type="checkbox" bind:checked={comparing} />{$t("Comparer en continu")}</label>
          <button onpointerdown={() => (holding = true)} onpointerup={() => (holding = false)} onpointerleave={() => (holding = false)} onpointercancel={() => (holding = false)}>{$t("Comparer à l’original (maintenir)")}</button>
        </div>
      </details>
    </div>
    {/if}
  </aside>

  <section class="wb-center">
    {#if source}<div class="wb-bar">
      <strong class="file" title={source.path}>{source.path.split(/[\\/]/).pop()}</strong>
      <span class="grow"></span>
      <span class="muted num perf" data-editor-perf>{processing ? $t("Calcul…") + ' · ' : ''}{$tn("{n} tracé", "{n} tracés", history.current.strokes.length)} · {source.width} × {source.height}{#if previewReadout} · {previewReadout}{/if}{#if perf.commitMsLast > 0} · {$t("application {ms} ms", { ms: perf.commitMsLast.toFixed(0) })}{/if}</span>
    </div>{/if}
    {#if !native}<p class="center-msg muted" role="status">{$t("Ouvrir le binaire Saya Studio pour retoucher une image locale.")}</p>
    {:else if !target}<p class="center-msg muted">{$t("Aucune image choisie. Ouvrez la galerie, sélectionnez une image, puis « Retoucher ».")}</p>
    {:else if status === 'loading'}<p class="center-msg muted" role="status">{$t("Chargement de l’image…")}</p>
    {:else if status === 'error'}<p class="center-msg error" role="alert">{error}</p>
    {:else if source}
      <div class="editor-stage" bind:this={host}>
        <canvas bind:this={frame} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerUp} onpointerleave={() => { cursor = null; invalidate(); }}></canvas>
        {#if showOriginal}<span class="compare-tag">{$t("Original")}</span>{/if}
      </div>
    {/if}
  </section>

  <aside class="wb-panel right">
    {#if source}
    <div class="wb-head"><h2 title={$t("Retouche non destructive · l’original n’est jamais modifié, l’export crée un nouveau fichier.")}>{$t("Tracés de flou")}</h2><span class="chip num">{history.current.strokes.length}</span></div>
    <div class="wb-scroll">
      <div class="history-actions">
        <button onclick={undo} disabled={!canUndo(history)}>{$t("Annuler le tracé")}</button>
        <button onclick={redo} disabled={!canRedo(history)}>{$t("Rétablir")}</button>
        <button class="quiet" onclick={clearAll} disabled={!history.current.strokes.length}>{$t("Tout annuler")}</button>
      </div>
      <ol class="strokes">
        {#each history.current.strokes as stroke, index (stroke.id)}<li class="num"><span>{$t("Tracé {n}", { n: index + 1 })}</span><span class="muted">{Math.round(stroke.size)} px · {Math.round(stroke.intensity * 100)} %</span></li>{/each}
        {#if !history.current.strokes.length}<li class="muted">{$t("Aucun tracé. Peignez sur l’image pour flouter.")}</li>{/if}
      </ol>
      <details class="sec export" open>
        <summary title={$t("La destination ne doit pas encore exister : l’export refuse d’écraser un fichier, y compris l’original.")}><span class="sec-title">{$t("Exporter vers un nouveau fichier")}</span></summary>
        <div class="sec-body">
          <label>{$t("Fichier de destination")}<input bind:value={destination} spellcheck="false" /></label>
          <label>{$t("Format")}<select bind:value={format} onchange={onFormatChange}><option value="png">{$t("PNG (sans perte)")}</option><option value="jpeg">{$t("JPEG")}</option></select></label>
          {#if format === 'jpeg'}<label>{$t("Qualité JPEG")}<input type="range" min="0.5" max="1" step="0.05" bind:value={quality} /></label>{/if}
          <button class="primary" disabled={exporting || !destination.trim()} onclick={exportImage}>{exporting ? $t("Export en cours…") : $t("Exporter l’image retouchée")}</button>
          {#if notice}<p class="notice" role="status">{notice}</p>{/if}
          {#if exportError}<p class="error" role="alert">{exportError}</p>{/if}
        </div>
      </details>
    </div>
    {/if}
  </aside>
</div>

<style>
  .editor-wb{--wb-left:240px;--wb-right:280px;margin:-20px -24px;height:100vh;min-height:0}
  .back{gap:6px;display:inline-flex;align-items:center;padding:0 6px;color:var(--text)}
  .slider{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:2px 8px;color:var(--muted);font-size:12px}
  .slider input{grid-column:1/-1;grid-row:2;width:100%;padding:0;min-height:20px;background:transparent;border:0;accent-color:var(--accent)}
  .slider span{grid-row:1;grid-column:2}
  .btn-row{display:flex;gap:4px}.btn-row button{flex:1;padding:0 6px}
  .checkbox{color:var(--text);font-size:12.5px}
  .file{font-size:12.5px;font-weight:600;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .perf{font-size:11.5px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}
  .center-msg{margin:auto;padding:24px;text-align:center}
  .editor-stage{position:relative;flex:1;min-height:0;background:var(--canvas);overflow:hidden;touch-action:none}
  .editor-stage canvas{width:100%;height:100%;display:block;cursor:crosshair}
  .compare-tag{position:absolute;top:10px;inset-inline-end:12px;padding:2px 8px;border-radius:var(--radius);background:var(--raised);border:1px solid var(--line-strong);color:var(--text);font-size:11.5px;pointer-events:none}
  .history-actions{display:flex;flex-wrap:wrap;gap:4px;padding:10px 12px;border-bottom:1px solid var(--line)}
  .strokes{list-style:none;margin:0;padding:0;max-height:40vh;overflow:auto;border-bottom:1px solid var(--line)}
  .strokes li{display:flex;justify-content:space-between;gap:8px;padding:5px 12px;border-top:1px solid var(--line);font-size:12px}
  .strokes li:first-child{border-top:0}
  .export label{display:grid;gap:4px;color:var(--muted);font-size:12px}
  .export input,.export select{width:100%}
  .export .notice{background:none;border:0;padding:0;color:var(--ok);font-size:12px;margin:0;white-space:normal}
  .export .error{margin:0;font-size:12px}
  details.sec{margin:0}
  @media(max-width:900px){.editor-wb{grid-template-columns:minmax(0,1fr);grid-template-rows:auto minmax(320px,1fr) auto;height:auto;min-height:100vh}.editor-wb .wb-panel.left,.editor-wb .wb-panel.right{border:0}}
</style>
