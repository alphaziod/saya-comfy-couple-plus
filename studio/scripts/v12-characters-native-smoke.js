(async () => {
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [],
    url: location.href, origin: location.origin, invokeType: typeof window.__TAURI_INTERNALS__?.invoke };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const wait = async (predicate, message, timeout = 20000) => {
    const end = Date.now() + timeout;
    while (!await predicate()) {
      if (Date.now() >= end) throw Error(message);
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  };
  const button = (text, root = document) => [...root.querySelectorAll('button')].find(node => node.textContent.trim() === text);
  const click = async text => {
    await wait(() => button(text) && !button(text).disabled, 'Button missing or disabled: ' + text);
    button(text).click();
  };
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const decoded = selector => { const image = document.querySelector(selector); return image?.complete && image.naturalWidth > 0; };
  const counters = (node, linked, visible) => {
    const text = node?.textContent || '';
    assert(new RegExp(`${linked}\\s+(?:images?\\s+)?associées?`).test(text), 'Linked count missing: ' + text);
    assert(new RegExp(`${visible}\\s+(?:images?\\s+)?visibles?`).test(text), 'Visible count missing: ' + text);
  };
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    report.bootstrap = await invoke('get_bootstrap');
    await wait(() => innerWidth === 1920 && innerHeight === 1080, 'Expected Niri fullscreen viewport 1920x1080');
    report.viewport = { width: innerWidth, height: innerHeight, dpr: devicePixelRatio };
    let expected, ids;
    if (report.phase === 'initial') {
      await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Initial gallery missing');
      assert((await api('gallery', { filters: {} })).total === 0, 'Fixture database must start empty');
      await click('Indexer / actualiser');
      await wait(async () => (await api('gallery', { filters: {} })).total === 130 && !(await api('index_status')).running,
        'Expected 130 indexed fixture images', 90000);
      ids = Object.fromEntries((await api('character_cards')).map(card => [card.name.split(' ')[0].toLowerCase(), card.id]));
      assert(ids.alpha && ids.beta && ids.gamma, 'Catalogue characters not imported: ' + JSON.stringify(ids));
      const assets = (await api('gallery', { filters: { characters: [ids.alpha], sort: 'name_asc' } })).items;
      assert(assets.length >= 2, 'Alpha assets missing');
      const variant = await api('save_variant', { character: ids.alpha, id: null, name: 'V12 Garden',
        blocks: { P1: 'alpha variant identity exact', BG: 'garden exact,  double space\nline' }, constraints: {} });
      const secondVariant = await api('save_variant', { character: ids.alpha, id: null, name: 'V12 Roof',
        blocks: { P1: 'alpha second identity exact', BG: 'roof exact' }, constraints: {} });
      await api('reference_edit', { character: ids.alpha, variant: null, asset: assets[0].id, operation: 'add' });
      await api('reference_edit', { character: ids.alpha, variant: null, asset: assets[0].id, operation: 'primary' });
      await api('reference_edit', { character: ids.alpha, variant, asset: assets[1].id, operation: 'add' });
      await api('reference_edit', { character: ids.alpha, variant, asset: assets[1].id, operation: 'primary' });
      await api('annotate', { id: assets[1].id, favorite: true, note: 'V12 isolated favorite', review: 'accepted' });
      expected = { variant, secondVariant, portrait: assets[0].id, favorite: assets[1].id, ids };
      await nav('Galerie personnages');
    } else {
      expected = window.__V12_SMOKE_PREVIOUS__?.expected;
      assert(expected, 'Restart expectations missing');
      ids = expected.ids;
      const preferences = await api('preferences');
      assert(preferences.selectedTab === 'people', 'Selected tab not persisted');
      await wait(() => document.querySelector('.subtabs button.on[data-nav="people"]'), 'Character tab not restored on restart');
      assert((await api('gallery', { filters: {} })).total === 130, 'Index not persisted');
      report.checks.push({ restart: true, selectedTabRestored: true, indexedImagesPreserved: true });
    }
    report.expected = expected;
    await wait(() => document.querySelectorAll('.person-card').length === 3, 'Expected three active character cards');
    for (const [id, count] of [[ids.alpha, 60], [ids.beta, 50], [ids.gamma, 20]]) {
      counters(document.querySelector(`[data-character="${id}"]`), count, count);
    }
    await wait(() => decoded(`[data-character="${ids.alpha}"] img`), 'Alpha portrait not decoded');
    const cards = await api('character_cards');
    assert(cards.length === 3, 'IPC active card count changed');
    const alpha = cards.find(card => card.id === ids.alpha);
    assert(alpha.imageCount === 60 && alpha.visibleImageCount === 60, 'IPC card counts changed');
    assert(alpha.portrait === expected.portrait && alpha.variantCount === 2, 'Portrait or variants changed');
    assert(alpha.mainVariant === null, 'Multiple variants must not invent a main variant');
    document.querySelector(`[data-character="${ids.alpha}"]`).click();
    await wait(() => document.querySelector('h1')?.textContent === 'Alpha Test', 'Alpha sheet missing');
    const sheet = await api('character_sheet', { id: ids.alpha });
    assert(sheet.identityPrompt === 'alpha full P1 identity' && sheet.anatomyPrompt === 'alpha full P1 anatomy', 'IPC identity changed');
    assert(document.querySelector('[data-p1]')?.textContent === sheet.identityPrompt, 'P1 UI changed');
    assert(document.querySelector('[data-anatomy]')?.textContent === sheet.anatomyPrompt, 'Anatomy UI changed');
    assert(sheet.imageCount === 60 && sheet.visibleImageCount === 60, 'IPC sheet counts changed');
    // Sheet header: the class is `.sheet-titlebar` since dc1fe2e
    // (19 Oct, cosmetic rename of `.page-heading` — this stale probe
    // had been breaking the smoke for a week, not the gallery itself).
    counters(document.querySelector('.sheet-titlebar'), 60, 60);
    for (const [id, name, p1, bg] of [[expected.variant, 'V12 Garden', 'alpha variant identity exact', 'garden exact,  double space\nline'],
      [expected.secondVariant, 'V12 Roof', 'alpha second identity exact', 'roof exact']]) {
      const variant = sheet.variants.find(item => item.id === id);
      assert(variant?.name === name && variant.promptBlocks.P1 === p1 && variant.promptBlocks.BG === bg, 'Variant IPC changed: ' + name);
      assert(sheet.backgrounds.some(item => item.variantId === id && item.bg === bg), 'BG IPC changed: ' + name);
      const detail = document.querySelector(`[data-variant="${id}"]`);
      assert(detail, 'Variant UI missing: ' + name); detail.open = true;
      assert(detail.textContent.includes(p1) && detail.textContent.includes(bg), 'Variant UI changed: ' + name);
    }
    assert(sheet.references.length === 2 && sheet.references.some(item => item.assetId === expected.portrait && !item.variantId && item.primary)
      && sheet.references.some(item => item.assetId === expected.favorite && item.variantId === expected.variant && item.primary), 'Reference scopes changed');
    assert(document.querySelectorAll('.reference-grid').length === 3, 'Expected top images, sheet references and variant references grids');
    await wait(() => [...document.querySelectorAll('.reference-grid img')].every(image => image.complete && image.naturalWidth > 0), 'Reference/top images not decoded');
    assert(document.querySelector('.sheet-body')?.textContent.includes('Principale · V12 Garden'), 'Variant reference scope UI missing');
    const top = [...document.querySelectorAll('[data-top-image]')];
    assert(top.length === 10 && sheet.topImages.length === 10, 'Top ten count changed');
    assert(top[0].dataset.topImage === expected.favorite && sheet.topImages[0].id === expected.favorite, 'Favorite not ranked first');
    assert(top.every((node, index) => node.dataset.topImage === sheet.topImages[index].id), 'UI top order differs from IPC');
    top[0].click();
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]') && decoded('dialog[open] img'), 'Top image did not open gallery dialog');
    await click('Fermer ×');
    await wait(() => !document.querySelector('dialog'), 'Image dialog did not close');
    await nav('Galerie personnages');
    await wait(() => document.querySelector('h1')?.textContent === 'Alpha Test', 'Selected sheet not restored after gallery');
    await click('← Toutes les fiches');
    await wait(() => document.querySelectorAll('.person-card').length === 3, 'Return to card list failed');
    await wait(async () => (await api('preferences')).selectedTab === 'people', 'Selected tab preference missing');
    assert(document.documentElement.scrollWidth <= innerWidth, 'Horizontal overflow');
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.checks.push({ cards: 3, portraitsDecoded: true, exactPromptsAndBackgrounds: true, variants: 2,
      referenceScopes: 2, topImages: 10, favoriteFirst: true, linkedAndVisibleCounts: true, galleryRoundtrip: true });
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
