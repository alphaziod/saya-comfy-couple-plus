(async () => {
  window.__SMOKE_STAGE = "starting";
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, text) => { if (!ok) throw new Error(text); };
  const wait = async (predicate, timeout = 30000) => { const start = Date.now(); while (!predicate()) { if (Date.now() - start > timeout) throw new Error('Timed out: ' + predicate); await new Promise(r => setTimeout(r, 100)); } };
  const click = text => { const node = [...document.querySelectorAll('button')].find(n => n.textContent.includes(text)); assert(node, 'Button missing: '+text); node.click(); };
  const report = { ok: false, checks: [], errors: [] };
  window.addEventListener('error', e => report.errors.push(e.message));
  window.addEventListener('unhandledrejection', e => report.errors.push(String(e.reason)));
  try {
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'));
    await new Promise(r => setTimeout(r, 300));
    window.__SMOKE_STAGE = 'gallery IPC';
    const before = await api('gallery', { filters: {} });
    const start = Date.now();
    window.__SMOKE_STAGE = 'index';
    click('Indexer / actualiser');
    await wait(() => document.querySelector('[data-gallery-index]')?.textContent.includes('en cours'));
    let pending = true, maxReadMs = 0, polls = 0;
    while (pending) {
      const t = Date.now();
      await api('gallery', { filters: {} });
      maxReadMs = Math.max(maxReadMs, Date.now()-t); polls++;
      await new Promise(r => setTimeout(r, 100));
      const state = await api('index_status'); pending = state.running;
      if (Date.now()-start > 240000) throw new Error('Index timeout');
    }
    const after = await api('gallery', { filters: {} });
    assert(after.total > 0, 'Real library is empty');
    await wait(() => { const img=document.querySelector('.image-card img'); return img?.complete && img.naturalWidth > 0; });
    const first = document.querySelector('.image-card img');
    assert(first.complete && first.naturalWidth > 0, 'Thumbnail failed to decode');
    report.checks.push({indexMs:Date.now()-start, before:before.total, images:after.total, maxReadMs, polls, rendered:document.querySelectorAll('.image-card').length});
    document.querySelector('.image-card').click();
    await wait(() => document.querySelector('[data-inspector-open]'));
    document.querySelector('[data-inspector-open]').click();
    await wait(() => document.querySelector('dialog[open] img')?.naturalWidth > 0);
    assert(document.activeElement.closest('dialog'), 'Modal did not take focus');
    click('Fermer');
    await wait(() => !document.querySelector('dialog'));
    const favorite = document.querySelector('[data-annotation-favorite]'); if (!favorite.checked) favorite.click();
    const note = document.querySelector('[data-annotation-note]'); note.value = 'Native smoke annotation'; note.dispatchEvent(new Event('input',{bubbles:true}));
    click('Enregistrer les annotations');
    await wait(() => ![...document.querySelectorAll('button')].some(b => b.textContent === 'Enregistrement…'));
    const saved = (await api('gallery',{filters:{favorite:true}})).items.find(a => a.note === 'Native smoke annotation');
    assert(saved,'Annotation not persisted');
    report.checks.push({preview:true,annotation:true,assetId:saved.id});
    const pathBefore = after.items[0].path;
    const search = document.querySelector('input[placeholder="Rechercher…"]'); search.value = pathBefore; search.dispatchEvent(new Event('input',{bubbles:true}));
    await wait(() => document.querySelectorAll('.image-card').length === 1);
    assert(document.documentElement.scrollWidth <= innerWidth,'Horizontal overflow');
    report.checks.push({search:true,viewport:[innerWidth,innerHeight]});
    assert(report.errors.length === 0, 'WebView errors');
    report.ok = true;
  } catch (error) { report.failure = String(error) + '\n' + String(error?.stack || ''); }
  window.__SMOKE_STAGE = report;
  await invoke('finish_smoke', { report }).catch(e => { window.__SMOKE_STAGE = {report,finishError:String(e)}; });
})();
