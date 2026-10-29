(async () => {
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [],
    url: location.href, origin: location.origin };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const wait = async (predicate, message, timeout = 20000) => {
    const end = Date.now() + timeout;
    while (!await predicate()) { if (Date.now() >= end) throw Error(message); await sleep(100); }
  };
  const button = text => [...document.querySelectorAll('button')].find(node => node.textContent.trim() === text);
  const click = async text => { await wait(() => button(text) && !button(text).disabled, 'Button missing or disabled: ' + text); button(text).click(); };
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const status = () => document.querySelector('[data-stream-status]');
  const loaded = () => Number(status()?.dataset.loaded ?? -1);
  const requests = () => Number(status()?.dataset.requests ?? -1);
  const complete = () => status()?.dataset.complete === 'true';
  const viewport = () => document.querySelector('.stream-viewport');
  const typeSearch = value => { const input = document.querySelector('input[aria-label="Rechercher une image"]'); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); };
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    await wait(() => innerWidth === 1920 && innerHeight === 1080, 'Expected Niri fullscreen viewport 1920x1080');
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Initial gallery missing');
    assert((await api('gallery', { filters: {} })).total === 0, 'Fixture database must start empty');
    await click('Indexer / actualiser');
    await wait(async () => (await api('gallery', { filters: {} })).total === 130 && !(await api('index_status')).running, 'Expected 130 indexed fixture images', 90000);

    // 1. Stream is the default view: it loads every entry in windows and virtualizes rendering.
    await wait(() => document.querySelector('input[data-stream-toggle]')?.checked, 'Stream should be on by default');
    await wait(() => viewport() && complete() && loaded() === 130, 'Stream did not load 130/130');
    await wait(() => [...document.querySelectorAll('.stream-card img')].some(img => img.complete && img.naturalWidth > 0), 'No stream thumbnail decoded');
    const rendered = document.querySelectorAll('.stream-card').length;
    assert(rendered > 0 && rendered < 130, 'Virtualization inactive: ' + rendered + ' cards rendered');
    assert(viewport().scrollHeight > viewport().clientHeight, 'Stream should need scrolling');
    report.checks.push({ streamLoaded: 130, renderedCards: rendered });

    // 2. Annotation from a scrolled position: stream refreshes in place (V12 defect: no refresh).
    viewport().scrollTop = Math.floor(viewport().scrollHeight / 2);
    viewport().dispatchEvent(new Event('scroll'));
    await sleep(300);
    const scrolled = viewport().scrollTop;
    assert(scrolled > 0, 'Stream did not scroll');
    const card = document.querySelector('.stream-card');
    const cardId = card.dataset.streamAsset;
    card.click();
    await wait(() => document.querySelector('dialog[open] img')?.naturalWidth > 0, 'Image dialog not decoded');
    const favorite = document.querySelector('dialog input[type=checkbox]');
    if (!favorite.checked) favorite.click();
    const callsBefore = requests();
    await click('Enregistrer les annotations');
    await wait(() => requests() > callsBefore && complete() && loaded() === 130, 'Stream did not refresh after annotation');
    await click('Fermer ×');
    await wait(() => !document.querySelector('dialog'), 'Dialog did not close');
    await wait(() => document.querySelector(`[data-stream-asset="${cardId}"]`)?.textContent.includes('★'), 'Refreshed card lacks favorite star');
    const afterRefresh = viewport().scrollTop;
    assert(Math.abs(afterRefresh - scrolled) < 600, 'Position lost on refresh: ' + afterRefresh + ' vs ' + scrolled);
    report.checks.push({ refreshAfterAnnotation: true, scroll: [scrolled, afterRefresh] });

    // 3. Empty result: complete, no request loop (V12 defect: empty loop).
    typeSearch('zz-no-such-image');
    await wait(() => status()?.dataset.total === '0' && complete(), 'Empty search not reported complete');
    await wait(() => document.querySelector('.stream-viewport .empty-state'), 'Empty state missing');
    const emptyCalls = requests();
    await sleep(2000);
    const emptyCallsLater = requests();
    assert(emptyCallsLater === emptyCalls, 'Empty result refetched: ' + emptyCalls + ' -> ' + emptyCallsLater);
    report.checks.push({ emptyResultExtraRequests: emptyCallsLater - emptyCalls });

    // 4. Rapid filter changes: final set is the last filter only, ordered, no stale window (V12 defect: stale queue).
    typeSearch('alpha');
    await sleep(30);
    typeSearch('beta');
    await wait(() => complete() && status()?.dataset.total === '50' && loaded() === 50, 'Beta filter did not settle at 50');
    await sleep(800);
    const ids = [...document.querySelectorAll('.stream-card')].map(node => node.querySelector('span').textContent);
    assert(ids.length > 0 && ids.every(text => text.includes('beta')), 'Stale entries after filter change: ' + ids.slice(0, 5).join(','));
    report.checks.push({ filterSettled: 50, staleEntries: 0 });

    // 5. Position restored after leaving the tab.
    typeSearch('');
    await wait(() => complete() && loaded() === 130, 'Unfiltered stream not reloaded');
    viewport().scrollTop = Math.floor(viewport().scrollHeight * 0.6);
    viewport().dispatchEvent(new Event('scroll'));
    await sleep(300);
    const saved = viewport().scrollTop;
    await nav('Galerie personnages');
    await wait(() => document.querySelector('.subtabs button.on[data-nav="people"]'), 'Character tab missing');
    await nav('Galerie');
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Gallery tab missing');
    await wait(() => document.querySelector('input[data-stream-toggle]')?.checked, 'Stream not active after remount');
    await wait(() => viewport() && viewport().scrollTop > 0 && complete(), 'Stream position not restored', 15000);
    const restored = viewport().scrollTop;
    assert(Math.abs(restored - saved) < 600, 'Scroll not restored: ' + restored + ' vs ' + saved);
    report.checks.push({ scrollRestored: [saved, restored] });

    // 6. The view choice persists: pages, then back to stream.
    document.querySelector('input[data-stream-toggle]').click();
    await wait(async () => (await api('preferences')).galleryView === 'pages' && document.querySelector('.pagination'), 'Pages view not persisted');
    document.querySelector('input[data-stream-toggle]').click();
    await wait(async () => (await api('preferences')).galleryView === 'stream' && viewport(), 'Stream view not persisted');
    report.checks.push({ viewPreferencePersisted: true });

    assert(document.documentElement.scrollWidth <= innerWidth, 'Horizontal overflow');
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
    report.requests = requests(); report.status = status()?.outerHTML.slice(0, 400);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(error => {
    window.__SMOKE_STAGE = { ...report, finishError: String(error) };
  });
})();
