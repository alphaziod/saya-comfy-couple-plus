// G1 — organised delivery (brief point 4): prepares and submits a full workflow
// to the simulated engine (no GPU), waits for delivery by the monitor,
// then proves that the image joined <root>/<day>/final/, that provenance and
// capture are known in the gallery, that the path is displayed with its buttons
// (point 5) and that a second delivery of the SAME content creates no
// doublon (content_sha256).
(async () => {
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [],
    url: location.href, invokeType: typeof window.__TAURI_INTERNALS__?.invoke };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const wait = async (predicate, message, timeout = 40000) => {
    const end = Date.now() + timeout;
    while (!await predicate()) {
      if (Date.now() >= end) throw Error(message);
      await new Promise(resolve => setTimeout(resolve, 200));
    }
  };
  const button = text => [...document.querySelectorAll('button')].find(node => node.textContent.trim() === text);
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    const captureRoot = window.__V12_CAPTURE_ROOT__;
    assert(typeof captureRoot === 'string' && captureRoot.length > 0, 'Capture root missing from harness');
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Gallery tab missing');
    await wait(() => {
      const node = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Indexer / actualiser');
      return node && !node.disabled;
    }, 'Index button not ready');
    [...document.querySelectorAll('button')].find(node => node.textContent.trim() === 'Indexer / actualiser').click();
    await wait(async () => (await api('gallery', { filters: {} })).total === 130 && !(await api('index_status')).running,
      'Expected 130 indexed fixture images', 90000);
    const prefs = await api('preferences');
    assert(prefs.captureRoot === captureRoot, 'captureRoot preference not persisted: ' + JSON.stringify(prefs.captureRoot));
    // Delivery 1: preparation + submission of the full workflow.
    const prepared = await api('prepare', { id: 'smoke-capture-wf', edits: [], characters: [] });
    assert(prepared.jobId && prepared.snapshotHash, 'Prepare failed: ' + JSON.stringify(prepared));
    await api('submit', { job: prepared.jobId, hash: prepared.snapshotHash, confirmed: true, allow_removals: false });
    // The monitor delivers (buffer → <day>/final/ → index) in the background.
    await wait(async () => (await api('gallery', { filters: {} })).total === 131,
      'Delivered image missing from gallery', 60000);
    const page = await api('gallery', { filters: {} });
    const captured = page.items.find(a => a.captureRole);
    assert(captured, 'No asset carries capture provenance');
    assert(captured.captureRole === 'final', 'Expected final role, got ' + captured.captureRole);
    const day = new Date().toISOString().slice(0, 10);
    assert(captured.captureDay === day, 'Expected UTC day ' + day + ', got ' + captured.captureDay);
    assert(captured.capturePhase === null, 'Plain job has no phase');
    // 'all' view and main view show the same image: a final is
    // never an intermediate pass.
    const all = await api('gallery', { filters: { capture: 'all' } });
    assert(all.total === 131, 'capture=all must not change a final-only set');
    const detail = await api('detail', { id: captured.id });
    assert(typeof detail.path === 'string' && detail.path.startsWith(captureRoot),
      'Absolute path must live in the capture root: ' + detail.path);
    assert(detail.path.includes('/final/'), 'Organized under <jour>/final/: ' + detail.path);
    report.checks.push({ delivered: true, captureRole: captured.captureRole, day: captured.captureDay,
      organizedPath: detail.path, galleryTotal: page.total });
    // Viewer: path displayed + locate buttons (point 5).
    const firstCard = document.querySelector('.image-card');
    // Open the capture: filter by exact search of the organised name.
    const search = document.querySelector('input[placeholder="Rechercher…"]');
    search.value = captured.path.split('/').pop();
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(() => document.querySelectorAll('.image-card').length === 1, 'Search did not isolate the capture');
    document.querySelector('.image-card').click();
    await wait(() => document.querySelector('dialog.image-dialog[open]'), 'Viewer dialog missing');
    await wait(() => document.querySelector('[data-asset-path]')?.textContent?.trim().length > 0, 'Path not shown');
    const shown = document.querySelector('[data-asset-path]')?.textContent?.trim();
    assert(shown === detail.path, 'Shown path differs from detail path: ' + shown);
    assert(button('Copier le chemin'), 'Copy path button missing');
    assert(button('Ouvrir dans le gestionnaire'), 'Open in manager button missing');
    report.checks.push({ pathShown: shown, copyPathButton: true, openManagerButton: true });
    // Zero duplicates: same workflow, same seed → same content → same asset.
    const second = await api('prepare', { id: 'smoke-capture-wf', edits: [], characters: [] });
    await api('submit', { job: second.jobId, hash: second.snapshotHash, confirmed: true, allow_removals: false });
    await wait(async () => {
      const jobs = await api('jobs');
      return jobs.every(job => job.state !== 'queued' && job.state !== 'running' && job.state !== 'submitting');
    }, 'Second delivery never settled', 60000);
    await new Promise(resolve => setTimeout(resolve, 2500));
    const after = await api('gallery', { filters: {} });
    assert(after.total === 131, 'Duplicate content must not add a second asset: ' + after.total);
    const secondDetail = await api('detail', { id: captured.id });
    const jobs = await api('jobs');
    const delivered = jobs.filter(job => (job.outputs ?? []).some(o => o.assetId === captured.id));
    assert(delivered.length >= 2, 'Both deliveries must reference the same asset: ' + delivered.length);
    report.checks.push({ zeroDuplicate: true, total: after.total, deliveriesLinked: delivered.length });
    // Close the dialog, clean ending.
    [...document.querySelectorAll('button')].find(node => node.textContent.trim() === 'Fermer ×')?.click();
    search.value = '';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) { report.failure = String(error?.message ?? error); }
  window.__SMOKE_STAGE = report;
  await invoke('finish_smoke', { report }).catch(e => { window.__SMOKE_STAGE = { report, finishError: String(e) }; });
})();
