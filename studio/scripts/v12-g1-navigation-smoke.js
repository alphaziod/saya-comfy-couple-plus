// G1 — timed navigation: opens a gallery image, chains
// previous/next, measures the latency of each navigation and counts the
// IPC calls per navigation (gallery_neighbors, detail, image). The durations
// go into the harness report (v12-native-smoke.py) for the before/after
// perf comparison. No generation, no GPU: 130-image fixture.
(async () => {
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [],
    url: location.href, invokeType: typeof window.__TAURI_INTERNALS__?.invoke };
  window.__SMOKE_STAGE = report;
  const internals = window.__TAURI_INTERNALS__;
  const invoke = internals.invoke;
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const wait = async (predicate, message, timeout = 40000) => {
    const end = Date.now() + timeout;
    while (!await predicate()) {
      if (Date.now() >= end) throw Error(message);
      await new Promise(resolve => setTimeout(resolve, 50));
    }
  };
  const button = (text, root = document) => [...root.querySelectorAll('button')].find(node => node.textContent.trim() === text);
  // Call counting: the app exposes an instrumentation point
  // (`window.__SAYA_IPC__` fed by api.ts) — the internal Tauri object being
  // non-writable, an invoke shim is impossible. Installed here, it counts each
  // action from now on.
  const probe = { counts: {}, ms: {} };
  window.__SAYA_IPC__ = probe;
  const counts = probe.counts;
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Gallery tab missing');
    await wait(() => button('Indexer / actualiser') && !button('Indexer / actualiser').disabled, 'Index button not ready');
    button('Indexer / actualiser').click();
    const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
    await wait(async () => (await api('gallery', { filters: {} })).total === 130 && !(await api('index_status')).running,
      'Expected 130 indexed fixture images', 90000);
    await wait(() => document.querySelectorAll('.image-card').length > 0, 'No gallery card rendered');
    // Let the initial grid finish its thumbnails so as not to count this
    // loading in the navigations.
    await wait(() => [...document.querySelectorAll('.image-card img')].slice(0, 6).every(img => img.complete), 'Initial thumbnails not ready', 60000);
    const viewport=document.querySelector('.stream-viewport');
    if(viewport){
      await wait(()=>[...document.querySelectorAll('.stream-thumb img')].length>6,'stream thumbnails');
      viewport.scrollTop=viewport.scrollHeight;
      await new Promise(r=>setTimeout(r,600));
      viewport.scrollTop=0;
      await wait(()=>document.querySelector('.stream-row')?.style.top==='0px','return to first row');
      await wait(()=>document.querySelector('.stream-row img')?.complete,'cached row returns decoded');
      report.checks.push('scroll out/back restores cached images');
    }
    const neighborsBefore = counts.gallery_neighbors ?? 0;
    const detailBefore = counts.detail ?? 0;
    // Open the first image (outside the measurement: it is the opening, not a navigation).
    document.querySelector('.image-card').click();
    await wait(() => document.querySelector('dialog.image-dialog[open]'), 'Viewer dialog missing');
    await wait(() => document.querySelector('[data-viewer-position]')?.textContent.includes('/'), 'Viewer position missing');
    const position = () => document.querySelector('[data-viewer-position]')?.textContent?.trim() ?? '';
    await wait(() => document.querySelector('dialog.image-dialog img')?.complete, 'Viewer preview not decoded', 60000);
    // Measurements: 8 × next then 4 × previous.
    const samples = [];
    const navigate = async which => {
      const from = position();
      const neighborsAtStart = counts.gallery_neighbors ?? 0;
      const detailAtStart = counts.detail ?? 0;
      const t0 = performance.now();
      button(which === 'next' ? 'Suivante →' : '← Précédente').click();
      await wait(() => position() !== from && !position().includes('Chargement'), `Navigation ${which} did not move from ${from}`, 30000);
      await wait(()=>document.querySelector('dialog.image-dialog img')?.complete,'next preview decoded');
      const ms = performance.now() - t0;
      samples.push({ which, ms,
        neighborsCalls: (counts.gallery_neighbors ?? 0) - neighborsAtStart,
        detailCalls: (counts.detail ?? 0) - detailAtStart });
    };
    for (let i = 0; i < 8; i++) await navigate('next');
    for (let i = 0; i < 4; i++) await navigate('previous');
    const times = samples.map(s => s.ms).sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const neighborsPerNav = samples.map(s => s.neighborsCalls);
    const detailPerNav = samples.map(s => s.detailCalls);
    report.checks.push({
      navigationSamples: samples.length,
      medianMs: Math.round(median * 10) / 10,
      minMs: Math.round(times[0] * 10) / 10,
      maxMs: Math.round(times[times.length - 1] * 10) / 10,
      neighborsCallsPerNavigation: neighborsPerNav,
      detailCallsPerNavigation: detailPerNav,
      totalNeighborsCalls: (counts.gallery_neighbors ?? 0) - neighborsBefore,
      totalDetailCalls: (counts.detail ?? 0) - detailBefore,
      thumbnailFetches: counts.image ?? 0,
    });
    // Locate buttons (point 5): present in the viewer.
    const pathNode = document.querySelector('[data-asset-path]');
    report.checks.push({
      pathShown: !!pathNode && (pathNode.textContent ?? '').includes('/'),
      copyPathButton: !!button('Copier le chemin'),
      openManagerButton: !!button("Ouvrir dans le gestionnaire"),
      shownPath: pathNode?.textContent?.trim() ?? null,
    });
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) { report.failure = String(error?.message ?? error); }
  window.__SMOKE_STAGE = report;
  await invoke('finish_smoke', { report }).catch(e => { window.__SMOKE_STAGE = { report, finishError: String(e) }; });
})();
