(async () => {
  // Requires the harness option --large WIDTHxHEIGHT (images/large.png).
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [], timings: {} };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const frame = () => new Promise(resolve => requestAnimationFrame(() => resolve()));
  const step = name => { window.__SMOKE_STEP = name + '@' + Date.now(); };
  const wait = async (predicate, message, timeout = 20000) => {
    step(message);
    const end = Date.now() + timeout;
    while (!await predicate()) { if (Date.now() >= end) throw Error(message); await sleep(50); }
  };
  const button = text => [...document.querySelectorAll('button')].find(node => node.textContent.trim() === text);
  const click = async text => { await wait(() => button(text) && !button(text).disabled, 'Button missing or disabled: ' + text); button(text).click(); };
  const setInput = (input, value) => { input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); };
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    await wait(() => innerWidth === 1920 && innerHeight === 1080, 'Expected Niri fullscreen viewport 1920x1080');
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Initial gallery missing');
    await click('Indexer / actualiser');
    await wait(async () => (await api('gallery', { filters: {} })).total === 131 && !(await api('index_status')).running, 'Expected 131 indexed images (fixture + large)', 120000);
    // The gallery opens by default in virtualised stream view; pages mode
    // renders deterministic cards for the search that follows.
    await wait(() => document.querySelector('[data-stream-toggle]'), 'Gallery toolbar missing');
    if (document.querySelector('[data-stream-toggle]').checked) document.querySelector('[data-stream-toggle]').click();
    await wait(() => document.querySelector('.image-grid'), 'Pages grid missing');
    const large = (await api('gallery', { filters: { search: 'large.png' } })).items[0];
    assert(large && large.width >= 4000, 'Large image not indexed: ' + JSON.stringify(large));
    const original = await api('editor_load', { id: large.id });

    // Gallery -> detail -> Retoucher.
    setInput(document.querySelector('input[placeholder="Rechercher…"]'), 'large.png');
    await wait(() => [...document.querySelectorAll('.image-card')].some(card => card.textContent.includes('large')), 'Search did not expose large (card label shows the stem without extension)');
    [...document.querySelectorAll('.image-card')].find(card => card.textContent.includes('large')).click();
    await click('Retoucher (flou localisé)');
    let start = performance.now();
    await wait(() => document.querySelector('h1')?.textContent === 'Éditeur', 'Editor page missing');
    await wait(() => button('Tout annuler') && button('Annuler le tracé')?.disabled && document.querySelector('.editor-stage canvas')?.width > 0, 'Editor not ready', 60000);
    await sleep(300);
    report.timings.openMs = Math.round(performance.now() - start);
    const canvas = document.querySelector('.editor-stage canvas');
    assert(canvas.width === canvas.clientWidth && canvas.height === canvas.clientHeight, 'Canvas backing store not sized to stage');

    // One long stroke across the image, timed until the undo button is enabled.
    const rect = canvas.getBoundingClientRect();
    const pointer = (type, x, y, buttons = 1) => canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: rect.left + x, clientY: rect.top + y, button: 0, buttons, pointerId: 1, pointerType: 'mouse', isPrimary: true }));
    const perfOf = () => window.__SAYA_BLUR_PERF__;
    const stroke = async (points) => {
      pointer('pointerdown', points[0][0], points[0][1]);
      for (const [x, y] of points.slice(1)) { pointer('pointermove', x, y); }
      // Real-time preview: at least one frame computed DURING the drag,
      // at screen resolution (never the image resolution).
      const beforeFrames = perfOf()?.previewFrames ?? 0;
      const beforeCommit = perfOf()?.commitMsLast ?? 0;
      await wait(() => (perfOf()?.previewFrames ?? 0) > beforeFrames, 'No real-time preview frame during stroke', 10000);
      const preview = perfOf();
      assert(preview.previewPixels > 0 && preview.previewPixels <= canvas.width * canvas.height,
        `Preview raster not screen-sized: ${preview.previewPixels} vs canvas ${canvas.width * canvas.height}`);
      report.timings.previewParts = (window).__SAYA_BLUR_PARTS__ ?? '';
      const t = performance.now();
      pointer('pointerup', points.at(-1)[0], points.at(-1)[1], 0);
      await frame();
      // Full-resolution application on release: latency measured.
      await wait(() => (perfOf()?.commitMsLast ?? 0) !== beforeCommit, 'Full-resolution commit not measured', 30000);
      return { up: performance.now() - t, previewMsLast: preview.previewMsLast, previewMsAvg: preview.previewMsAvg };
    };
    const line = (x0, y0, x1, y1, n) => Array.from({ length: n }, (_, i) => [x0 + (x1 - x0) * i / (n - 1), y0 + (y1 - y0) * i / (n - 1)]);
    const w = rect.width, h = rect.height;
    report.timings.strokeMs = [];
    const first = await stroke(line(w * 0.2, h * 0.3, w * 0.8, h * 0.7, 60));
    report.timings.strokeMs.push(Math.round(first.up));
    report.timings.previewMsLast = Math.round(first.previewMsLast * 10) / 10;
    report.timings.previewMsAvg = Math.round(first.previewMsAvg * 10) / 10;
    await wait(() => !button('Annuler le tracé').disabled, 'Stroke not committed');
    report.timings.strokeMs.push(Math.round(await stroke(line(w * 0.3, h * 0.8, w * 0.7, h * 0.2, 60)).then(r => r.up)));
    report.timings.strokeMs.push(Math.round(await stroke(line(w * 0.1, h * 0.5, w * 0.9, h * 0.5, 120)).then(r => r.up)));
    report.timings.commitMsLast = Math.round((perfOf()?.commitMsLast ?? 0));
    start = performance.now();
    await click('Annuler le tracé');
    await frame();
    report.timings.undoMs = Math.round(performance.now() - start);
    start = performance.now();
    await click('Rétablir');
    await frame();
    report.timings.redoMs = Math.round(performance.now() - start);
    assert(button('Rétablir').disabled, 'Redo still enabled after redo');
    const persisted = await api('blur_document', { asset: large.id });
    assert(persisted.document?.strokes?.length === 3, 'Blur strokes were not persisted: ' + JSON.stringify(persisted));

    // Export to a new file next to the fixture, then prove the original is intact.
    const dir = original.path.slice(0, original.path.lastIndexOf('/'));
    const destination = dir + '/large - smoke-export.png';
    document.querySelector('details:has(.filters)')?.setAttribute('open', '');
    const destinationInput = [...document.querySelectorAll('label')].find(label => label.textContent.startsWith('Fichier de destination'))?.querySelector('input');
    assert(destinationInput, 'Destination input missing');
    setInput(destinationInput, destination);
    start = performance.now();
    await click('Exporter l’image retouchée');
    await wait(() => document.querySelector('.notice')?.textContent.includes('Image exportée'), 'Export notice missing', 90000);
    report.timings.exportMs = Math.round(performance.now() - start);
    const after = await api('editor_load', { id: large.id });
    assert(after.data === original.data, 'Original changed after export');
    let refused = false;
    try { await api('editor_export', { id: large.id, destination, format: 'png', image: 'AA==' }); } catch { refused = true; }
    assert(refused, 'Second export to the same destination was not refused');
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.checks.push({ image: [large.width, large.height], strokes: 3, undoRedo: true, persisted: persisted.sha256, exported: destination, originalIntact: true, overwriteRefused: true, realTimePreview: { frames: perfOf()?.previewFrames ?? 0, previewMsAvg: report.timings.previewMsAvg, commitMsLast: report.timings.commitMsLast } });
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(error => {
    window.__SMOKE_STAGE = { ...report, finishError: String(error) };
  });
})();
