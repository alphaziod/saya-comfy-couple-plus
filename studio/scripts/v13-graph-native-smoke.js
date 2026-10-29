(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  // Requires --mock-comfy (object_info only; nothing submitted).
  const report = { ok: false, checks: [], errors: [] };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const wait = async (predicate, message, timeout = 20000) => {
    window.__SMOKE_STEP = message + '@' + Date.now();
    const end = Date.now() + timeout;
    while (!await predicate()) { if (Date.now() >= end) throw Error(message); await sleep(50); }
  };
  const $ = s => document.querySelector(s);
  const button = text => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text);
  const click = async text => { await wait(() => button(text) && !button(text).disabled, 'Button missing or disabled: ' + text); button(text).click(); };
  const center = el => { const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  const pointer = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, isPrimary: true }));
  const SEED = '18446744073709551614';
  const graph = `{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},` +
    `"2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"exact,  double\\nligne"}},"3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"neg"}},` +
    `"4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},` +
    `"5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":18446744073709551615,"steps":20,"cfg":6.5,"sampler_name":"euler","scheduler":"normal","denoise":1}},` +
    `"6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},"7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"g"}}}`;
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    const source = await api('import_workflow', { name: 'Graphe source', text: graph });
    const before = JSON.stringify(await api('workflow_nodes', { id: source }));
    await nav('Éditeur graphique');
    await wait(() => $('[data-graph-editor]'), 'Graph editor page missing');
    const select = [...document.querySelectorAll('[data-graph-editor] select, select')].find(s => [...s.options].some(o => o.value === source));
    assert(select, 'Workflow selector missing');
    select.value = source; select.dispatchEvent(new Event('change', { bubbles: true }));
    await click('Ouvrir');
    await wait(() => document.querySelectorAll('[data-graph-node]').length === 7, 'Seven nodes expected');
    // Wires are hidden by default (local preference): we display them.
    const wires = [...document.querySelectorAll('label.checkbox input')].find(i => i.parentElement.textContent.includes('Afficher les fils'));
    assert(wires, 'Edges toggle missing');
    if (!wires.checked) wires.click();
    await wait(() => document.querySelectorAll('[data-graph-edge]').length === 9, 'Nine edges expected: ' + document.querySelectorAll('[data-graph-edge]').length);
    // Drag the KSampler: one move, then undo restores, redo reapplies.
    const ks = () => $('[data-graph-node="5"]');
    const left0 = ks().style.left;
    const c = center(ks().querySelector('header') ?? ks());
    pointer(ks(), 'pointerdown', c.x, c.y); pointer(window, 'pointermove', c.x + 60, c.y + 40); pointer(window, 'pointermove', c.x + 120, c.y + 80); pointer(window, 'pointerup', c.x + 120, c.y + 80);
    await wait(() => ks().style.left !== left0, 'Node did not move');
    const left1 = ks().style.left;
    await wait(() => !$('[data-graph-undo]').disabled, 'Undo not enabled');
    $('[data-graph-undo]').click();
    await wait(() => ks().style.left === left0, 'Undo did not restore position');
    $('[data-graph-redo]').click();
    await wait(() => ks().style.left === left1, 'Redo did not reapply');
    report.checks.push({ dragUndoRedo: [left0, left1] });
    // Add a PreviewImage and connect VAEDecode.IMAGE → PreviewImage.images by dragging port to port.
    const add = document.querySelector('input[aria-label="Ajouter une classe"]');
    add.value = 'PreviewImage'; add.dispatchEvent(new Event('input', { bubbles: true }));
    add.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await wait(() => document.querySelectorAll('[data-graph-node]').length === 8, 'PreviewImage not added');
    const preview = [...document.querySelectorAll('[data-graph-node]')].find(n => n.textContent.includes('PreviewImage'));
    const out = $('[data-graph-node="6"] [data-port="out"]');
    const into = preview.querySelector('[data-port="in"][data-name="images"]');
    const a = center(out), b = center(into);
    pointer(out, 'pointerdown', a.x, a.y); pointer(window, 'pointermove', (a.x + b.x) / 2, (a.y + b.y) / 2); pointer(window, 'pointermove', b.x, b.y); pointer(window, 'pointerup', b.x, b.y);
    await wait(() => document.querySelectorAll('[data-graph-edge]').length === 10, 'Connection not created');
    // Incompatible link (LATENT → IMAGE) is refused by Rust with a message, no edge added.
    const lat = $('[data-graph-node="5"] [data-port="out"]');
    const c1 = center(lat), c2 = center(preview.querySelector('[data-port="in"][data-name="images"]'));
    pointer(lat, 'pointerdown', c1.x, c1.y); pointer(window, 'pointermove', c2.x, c2.y); pointer(window, 'pointerup', c2.x, c2.y);
    await sleep(600);
    assert(document.querySelectorAll('[data-graph-edge]').length === 10, 'Refused link changed the graph');
    report.checks.push({ addNode: true, connect: true, refusedLink: [...document.querySelectorAll('.error')].map(e => e.textContent).join(' ').slice(0, 120) });
    // Edit the u64 seed in the properties panel (exact text), then export.
    pointer(ks(), 'pointerdown', center(ks()).x, center(ks()).y); pointer(window, 'pointerup', center(ks()).x, center(ks()).y);
    await wait(() => $('[data-graph-props] h3')?.textContent.includes('KSampler'), 'KSampler not selected');
    const seedLabel = [...$('[data-graph-props]').querySelectorAll('label')].find(l => l.firstChild?.textContent?.trim() === 'seed');
    const seedInput = seedLabel.querySelector('input');
    assert(seedInput.value === '18446744073709551615', 'Seed text not exact: ' + seedInput.value);
    seedInput.value = SEED; seedInput.dispatchEvent(new Event('input', { bubbles: true }));
    seedInput.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await wait(() => seedLabel.querySelector('input').value === SEED && !$('[data-graph-status]')?.textContent.includes('…'), 'Seed not applied');
    const titleInput = $('[data-graph-node-title]'), colorInput = $('[data-graph-node-color]');
    assert(titleInput && colorInput, 'Node appearance fields missing');
    const oldLabel = ks().textContent;
    titleInput.value = 'Sampler nommé'; titleInput.dispatchEvent(new Event('change', { bubbles: true }));
    await wait(() => ks().textContent.includes('Sampler nommé'), 'Node title not applied');
    colorInput.value = '#336699'; colorInput.dispatchEvent(new Event('change', { bubbles: true }));
    await wait(() => { const h=ks().querySelector('.graph-head'); return h && getComputedStyle(h).backgroundColor === 'rgb(51, 102, 153)'; }, 'Node color not applied');
    $('[data-graph-undo]').click(); $('[data-graph-undo]').click();
    await wait(() => !ks().textContent.includes('Sampler nommé'), 'Style undo failed');
    $('[data-graph-redo]').click(); $('[data-graph-redo]').click();
    await wait(() => { const h=ks().querySelector('.graph-head'); return ks().textContent.includes('Sampler nommé') && h && getComputedStyle(h).backgroundColor === 'rgb(51, 102, 153)'; }, 'Style redo failed');
    report.checks.push({ nodeTitle: true, nodeColor: true, styleUndoRedo: true });
    const name = [...document.querySelectorAll('label')].find(l => l.textContent.startsWith('Exporter comme nouveau workflow')).querySelector('input');
    name.value = 'Graphe édité'; name.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(() => !$('[data-graph-export]').disabled, 'Export disabled: ' + ($('[data-graph-status]')?.textContent ?? ''));
    $('[data-graph-export]').click();
    await wait(async () => (await api('workflows')).some(w => w.name === 'Graphe édité'), 'Exported workflow missing');
    const exported = (await api('workflows')).find(w => w.name === 'Graphe édité');
    const fields = (await api('workflow', { id: exported.id })).fields;
    assert(fields.find(f => f.nodeId === '5' && f.inputName === 'seed').value === SEED, 'Exported seed not exact');
    assert(fields.find(f => f.nodeId === '2' && f.inputName === 'text').value === 'exact,  double\nligne', 'Exported prompt not exact');
    assert((await api('workflow', { id: exported.id })).nodeCount === 8, 'Exported node count');
    assert(JSON.stringify(await api('workflow_nodes', { id: source })) === before, 'Source workflow modified');
    report.checks.push({ seedExact: SEED, exported: exported.id, sourceUnchanged: true });
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
