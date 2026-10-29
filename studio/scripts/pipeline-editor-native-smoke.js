(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  // Slice G4 — brief point 13: visual pipeline editor.
  // Isolated fixture + --mock-comfy WITHOUT --mock-accept: object_info only,
  // no /prompt submission possible (the harness fails otherwise). The pipeline
  // is created via the canvas, saved (core preflight without an engine) and a
  // run is prepared; no pass is launched, no GPU.
  const report = { ok: false, checks: [], errors: [] };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const wait = async (predicate, message, timeout = 25000) => {
    window.__SMOKE_STEP = message + '@' + Date.now();
    const end = Date.now() + timeout;
    while (!await predicate()) { if (Date.now() >= end) throw Error(message); await sleep(50); }
  };
  const $ = s => document.querySelector(s);
  const click = async text => {
    const button = () => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text);
    await wait(() => button() && !button().disabled, 'Button unavailable: ' + text);
    button().click();
  };
  const set = (el, value, event = 'input') => { assert(el, 'Field missing'); el.value = value; el.dispatchEvent(new Event(event, { bubbles: true })); };
  const choose = async (el, value) => { await wait(() => [...el.options].some(o => o.value === value), 'Option missing ' + value); set(el, value, 'change'); };
  const center = el => { const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  const pointer = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, isPrimary: true }));
  const drag = async (from, to) => {
    const a = center(from), b = center(to);
    pointer(from, 'pointerdown', a.x, a.y);
    pointer(window, 'pointermove', (a.x + b.x) / 2, (a.y + b.y) / 2);
    pointer(window, 'pointermove', b.x, b.y);
    pointer(window, 'pointerup', b.x, b.y);
    await sleep(30);
  };
  window.addEventListener('error', e => report.errors.push(e.message));
  window.addEventListener('unhandledrejection', e => report.errors.push(String(e.reason)));
  try {
    const baseGraph = '{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},"2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"old P1"}},"3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"old neg"}},"4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},"5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":18446744073709551615,"steps":20,"cfg":6,"sampler_name":"euler","scheduler":"normal","denoise":1}},"6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},"7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"base"}}}';
    const base = await api('import_workflow', { name: 'G4 base', text: baseGraph });
    const upscale = await api('import_workflow', { name: 'G4 upscale', text: JSON.stringify({
      '1': { class_type: 'LoadImage', inputs: { image: 'fixture-image.png' } },
      '2': { class_type: 'ImageScaleBy', inputs: { image: ['1', 0], upscale_method: 'bicubic', scale_by: 2 } },
      '3': { class_type: 'SaveImage', inputs: { images: ['2', 0], filename_prefix: 'upscale' } },
    }) });
    // Real semantic bindings (as Saya configures them once): the
    // core preflight will resolve P1/NEG per section at save time.
    const fields = (await api('workflow', { id: base })).fields;
    await api('save_bindings', { id: base, fields: [
      { section: 'P1', nodeId: '2', inputName: 'text', value: 'old P1' },
      { section: 'NEG', nodeId: '3', inputName: 'text', value: 'old neg' },
    ] });
    assert((await api('workflow', { id: base })).fields.find(f => f.section === 'P1'), 'P1 section not bound');

    await nav('Pipelines');
    await wait(() => $('[data-pipelines]'), 'Pipelines page missing');
    await wait(() => $('[data-pipeline-editor]'), 'Visual pipeline editor missing');
    // Canvas: the draft starts with an empty pass (simple form
    // behaviour): it becomes the base pass; we add the upscale.
    set($('[data-pipeline-builder] label input'), 'G4 config globale');
    $('[data-pe-phase="passe1"]').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: center($('[data-pe-phase="passe1"]')).x, clientY: center($('[data-pe-phase="passe1"]')).y, pointerId: 1, isPrimary: true }));
    pointer(window, 'pointerup', center($('[data-pe-phase="passe1"]')).x, center($('[data-pe-phase="passe1"]')).y);
    await wait(() => $('[data-pe-step-workflow]'), 'Phase 1 workflow selector missing');
    await choose($('[data-pe-step-workflow]'), base);
    await choose($('[data-pe-phase-workflow]'), upscale);
    await click('+ Passe');
    await wait(() => $('[data-pe-phase="passe2"]'), 'Phase block 2 missing');
    await sleep(200); // the canvas re-framing applies after the addition
    // Image link pass1 → pass2 by dragging from port to port (ComfyUI style).
    await drag($('[data-pe-phase="passe1"] [data-pe-port="out"][data-pe-name="image"]'),
               $('[data-pe-phase="passe2"] [data-pe-port="in"][data-pe-name="image"]'));
    let edgeOk = true;
    try { await wait(() => $('[data-pe-edge="img:passe2"]'), 'Image edge missing', 6000); }
    catch { edgeOk = false; }
    if (!edgeOk) {
      const target = $('[data-pe-phase="passe2"] [data-pe-port="in"][data-pe-name="image"]');
      const at = center(target);
      const hit = document.elementFromPoint(at.x, at.y);
      report.debug = {
        note: document.querySelector('.pe-note')?.textContent ?? null,
        blocks: [...document.querySelectorAll('[data-pe-block]')].map(b => ({
          key: b.dataset.peBlock, rect: b.getBoundingClientRect().toJSON(),
          ports: [...b.querySelectorAll('[data-pe-port]')].map(p => ({ port: p.dataset.pePort, name: p.dataset.peName, rect: p.getBoundingClientRect().toJSON() })),
        })),
        edges: [...document.querySelectorAll('[data-pe-edge]')].map(e => e.dataset.peEdge),
        dropHit: { x: at.x, y: at.y, tag: hit?.tagName, port: hit?.dataset?.pePort, block: hit?.dataset?.peBlock, name: hit?.dataset?.peName },
        canvasRect: $('[data-pe-canvas]')?.getBoundingClientRect().toJSON() ?? null,
      };
      throw Error('Image edge missing — see report.debug');
    }
    // LoadImage node target of the link, via the pass 2 panel.
    $('[data-pe-phase="passe2"]').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: center($('[data-pe-phase="passe2"]')).x, clientY: center($('[data-pe-phase="passe2"]')).y, pointerId: 1, isPrimary: true }));
    pointer(window, 'pointerup', center($('[data-pe-phase="passe2"]')).x, center($('[data-pe-phase="passe2"]')).y);
    await wait(() => $('[data-pe-step-node]'), 'LoadImage target selector missing');
    await choose($('[data-pe-step-node]'), '1');
    // Drag a block: the move is local to the canvas.
    const before = $('[data-pe-phase="passe2"]').style.left;
    const c = center($('[data-pe-phase="passe2"]'));
    pointer($('[data-pe-phase="passe2"]'), 'pointerdown', c.x, c.y);
    pointer(window, 'pointermove', c.x + 80, c.y + 30);
    pointer(window, 'pointerup', c.x + 80, c.y + 30);
    await wait(() => $('[data-pe-phase="passe2"]').style.left !== before, 'Block did not move');
    // Global config no. 1: multiline P1 + NEG, linked to pass 1.
    $('[data-pe-config="config1"]').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: center($('[data-pe-config="config1"]')).x, clientY: center($('[data-pe-config="config1"]')).y, pointerId: 1, isPrimary: true }));
    pointer(window, 'pointerup', center($('[data-pe-config="config1"]')).x, center($('[data-pe-config="config1"]')).y);
    await click('Ajouter un champ');
    await click('Ajouter un champ');
    await wait(() => $('[data-pe-field-key="1"]'), 'Second field missing');
    set($('[data-pe-field-key="0"]'), 'P1');
    set($('[data-pe-field-value="0"]'), 'elf, solo\nligne 2 🌸');
    set($('[data-pe-field-key="1"]'), 'NEG');
    set($('[data-pe-field-value="1"]'), '');
    await drag($('[data-pe-config="config1"] [data-pe-port="out"][data-pe-name="P1"]'),
               $('[data-pe-phase="passe1"] [data-pe-port="in"][data-pe-name="config"]'));
    await drag($('[data-pe-config="config1"] [data-pe-port="out"][data-pe-name="NEG"]'),
               $('[data-pe-phase="passe1"] [data-pe-port="in"][data-pe-name="config"]'));
    await wait(() => $('[data-pe-edge="cfg:config1:passe1:P1"]') && $('[data-pe-edge="cfg:config1:passe1:NEG"]'), 'Config1 edges missing');
    // Config globale n°2 : seed exacte → passe 1, champ explicite → passe 2.
    await click('+ Config globale');
    await wait(() => $('[data-pe-config="config2"]'), 'Second config block missing');
    await sleep(200); // re-framing after the block is added
    $('[data-pe-config="config2"]').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: center($('[data-pe-config="config2"]')).x, clientY: center($('[data-pe-config="config2"]')).y, pointerId: 1, isPrimary: true }));
    pointer(window, 'pointerup', center($('[data-pe-config="config2"]')).x, center($('[data-pe-config="config2"]')).y);
    set($('[data-pe-config-label]'), 'Rendu amont', 'input');
    await click('Ajouter un champ');
    await click('Ajouter un champ');
    // A single properties panel is rendered: when config2 is
    // selected, all the [data-pe-field-*] fields belong to it.
    const cfg2 = () => $('[data-pe-config="config2"]');
    const field = (attr, key) => $(`[data-pe-field-${attr}="${key}"]`);
    await wait(() => field('key', '1'), 'Config2 fields missing');
    set(field('key', '0'), 'seed');
    set(field('value', '0'), '9007199254740993');
    set(field('key', '1'), '2.upscale_method');
    set(field('value', '1'), 'nearest-exact');
    await sleep(80);
    const seedPort = cfg2().querySelector('[data-pe-port="out"][data-pe-name="seed"]');
    if (!seedPort) {
      report.debug2 = {
        cfg2Ports: [...cfg2().querySelectorAll('[data-pe-port]')].map(p => ({ port: p.dataset.pePort, name: p.dataset.peName })),
        fieldInputs: [...document.querySelectorAll('[data-pe-field-key]')].map(el => ({ idx: el.dataset.peFieldKey, value: el.value })),
        label: $('[data-pe-config-label]')?.value ?? null,
        blocks: [...document.querySelectorAll('[data-pe-block]')].map(b => b.dataset.peBlock),
      };
      throw Error('seed port missing — see report.debug2');
    }
    await drag(seedPort,
               $('[data-pe-phase="passe1"] [data-pe-port="in"][data-pe-name="config"]'));
    await drag(cfg2().querySelector('[data-pe-port="out"][data-pe-name="2.upscale_method"]'),
               $('[data-pe-phase="passe2"] [data-pe-port="in"][data-pe-name="config"]'));
    await wait(() => $('[data-pe-edge="cfg:config2:passe1:seed"]') && $('[data-pe-edge="cfg:config2:passe2:2.upscale_method"]'), 'Config2 edges missing');
    // Front-side proof: the v2 manifest carries the parameters and the seed.
    const manifest = JSON.parse($('[data-pipeline-manifest]').textContent);
    const wanted = {
      v2: manifest.version === 2,
      p1neg: JSON.stringify(manifest.steps[0]?.parameters) === JSON.stringify({ P1: 'elf, solo\nligne 2 🌸', NEG: '' }),
      seed: manifest.steps[0]?.seed === '9007199254740993',
      explicit: JSON.stringify(manifest.steps[1]?.parameters) === JSON.stringify({ '2.upscale_method': 'nearest-exact' }),
      binding: manifest.steps[1]?.bindings?.[0]?.nodeId === '1',
    };
    if (Object.values(wanted).some(v => !v)) report.manifestDump = manifest;
    assert(wanted.v2, 'Manifest not v2');
    assert(wanted.p1neg, 'P1/NEG not in manifest');
    assert(wanted.seed, 'Seed not in manifest');
    assert(wanted.explicit, 'Explicit field not in manifest');
    assert(wanted.binding, 'LoadImage binding missing');
    // Visible core preflight: an unresolvable key is refused at save time.
    $('[data-pe-config="config1"]').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: center($('[data-pe-config="config1"]')).x, clientY: center($('[data-pe-config="config1"]')).y, pointerId: 1, isPrimary: true }));
    pointer(window, 'pointerup', center($('[data-pe-config="config1"]')).x, center($('[data-pe-config="config1"]')).y);
    await click('Ajouter un champ');
    await wait(() => field('key', '2'), 'Third field missing');
    set(field('key', '2'), '2.inexistant');
    await sleep(80);
    await drag($('[data-pe-config="config1"] [data-pe-port="out"][data-pe-name="2.inexistant"]'),
               $('[data-pe-phase="passe1"] [data-pe-port="in"][data-pe-name="config"]'));
    await click('Enregistrer le pipeline');
    await wait(() => document.querySelector('.error'), 'Kernel accepted an unresolvable field');
    assert(document.querySelector('.error').textContent.includes('2.inexistant'), 'Kernel refusal not shown: ' + document.querySelector('.error').textContent);
    assert((await api('pipelines')).length === 0, 'Pipeline saved despite kernel refusal');
    // Removal of the faulty field (the link goes with it), green save.
    document.querySelector('[data-pe-field-remove="2"]').click();
    await sleep(60);
    await click('Enregistrer le pipeline');
    await wait(async () => (await api('pipelines')).length === 1, 'Pipeline not saved');
    const pipeline = (await api('pipelines'))[0].id;
    // Prepared run without GPU: run creation, no pass launched.
    $(`[data-pipeline-item="${pipeline}"]`).click();
    await click('Nouvelle exécution');
    await wait(() => $('[data-run-next]'), 'Run not opened');
    const run = (await api('pipeline_runs', { pipeline }))[0].run;
    const detail = await api('pipeline_run', { run });
    assert(detail.steps.length === 2 && detail.steps.every(s => s.state === 'pending'), 'Run not fully pending');
    assert(detail.steps[0].step === 'passe1' && detail.steps[1].step === 'passe2', 'Execution order wrong');
    assert($('[data-run-next]').disabled, 'Run must stay confirmation-gated');
    assert(report.errors.length === 0, report.errors.join('; '));
    report.checks.push({
      phases: 2, configBlocks: 2, configLinks: 4,
      manifestV2: true, kernelPreflightRefusalShown: true, kernelPreflightGreenAfterFix: true,
      runPreparedWithoutSubmission: true, dragAndPorts: true,
    });
    Object.assign(report, { ok: true, pipeline, run, base, upscale });
  } catch (e) { report.failure = String(e.stack || e); report.uiErrors = [...document.querySelectorAll('.error')].map(n => n.textContent); }
  await invoke('finish_smoke', { report });
})();
