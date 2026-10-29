(async () => {
  // Requires --mock-comfy (scrubbed object_info subset). Optional --restart.
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [] };
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
  const $ = selector => document.querySelector(selector);
  const button = text => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text);
  const click = async text => { await wait(() => button(text) && !button(text).disabled, 'Button missing or disabled: ' + text); button(text).click(); };
  const set = (el, value) => { el.value = value; el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); };
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const workflowId = async name => (await api('workflows')).find(w => w.name === name)?.id;
  const GRAPH = {
    1: { class_type: 'CheckpointLoaderSimple', inputs: { ckpt_name: 'fixture-ckpt_name.safetensors' } },
    2: { class_type: 'CLIPTextEncode', inputs: { clip: ['1', 1], text: 'P1 exact,  double espace\nligne' } },
    3: { class_type: 'CLIPTextEncode', inputs: { clip: ['1', 1], text: 'blurry' } },
    4: { class_type: 'EmptyLatentImage', inputs: { width: 1024, height: 1024, batch_size: 1 } },
    5: { class_type: 'KSampler', inputs: { model: ['1', 0], positive: ['2', 0], negative: ['3', 0], latent_image: ['4', 0], seed: 18446744073709551615n, steps: 20, cfg: 6, sampler_name: 'euler', scheduler: 'normal', denoise: 1 } },
    6: { class_type: 'VAEDecode', inputs: { samples: ['5', 0], vae: ['1', 2] } },
    7: { class_type: 'SaveImage', inputs: { images: ['6', 0], filename_prefix: 'smoke' } },
  };
  // u64 max must survive as an exact JSON integer: serialize BigInt by hand.
  const text = graph => JSON.stringify(graph, (k, v) => typeof v === 'bigint' ? `__u64_${v}__` : v).replace(/"__u64_(\d+)__"/g, '$1');
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    if (report.phase === 'initial') {
      await api('import_workflow', { name: 'Smoke source', text: text(GRAPH) });
      await api('import_workflow', { name: 'Smoke cible', text: text({ ...GRAPH, 8: { class_type: 'PreviewImage', inputs: {} } }) });
      const target = await workflowId('Smoke cible');
      const targetNodesBefore = await api('workflow_nodes', { id: target });
      // VISUAL extraction: selection in the graph editor, "Créer un module"
      // button — no more checkbox list.
      const pointer = (el, type, x, y, shift = false) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, isPrimary: true, shiftKey: shift }));
      const center = el => { const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
      const pick = (id, shift = false) => { const node = document.querySelector(`[data-graph-node="${id}"]`); assert(node, 'Node missing in editor: ' + id); const c = center(node); pointer(node, 'pointerdown', c.x, c.y, shift); pointer(window, 'pointerup', c.x, c.y, shift); };
      const source = await workflowId('Smoke source');
      const extractInEditor = async (name, ids) => {
        await nav('Éditeur graphique');
        await wait(() => document.querySelector('[data-graph-editor]'), 'Graph editor page missing');
        const select = [...document.querySelectorAll('[data-graph-editor] select, select')].find(s => [...s.options].some(o => o.value === source));
        assert(select, 'Workflow selector missing');
        if (select.value !== source) { select.value = source; select.dispatchEvent(new Event('change', { bubbles: true })); }
        await click('Ouvrir');
        await wait(() => document.querySelectorAll('[data-graph-node]').length === 7, 'Seven nodes expected in editor');
        for (const [index, id] of ids.entries()) pick(id, index > 0);
        await wait(() => $('[data-graph-status]')?.textContent.includes(`${ids.length} sélectionné(s)`), 'Selection not reflected in status');
        await click('Créer un module');
        await wait(() => $('[data-graph-module-name]'), 'Module form missing');
        set($('[data-graph-module-name]'), name);
        await click('Extraire et enregistrer');
        await wait(() => $('[data-module-created]'), 'Module not created');
        return $('[data-module-created-ports]').textContent;
      };
      const samplerPorts = await extractInEditor('Sampler+Decode', ['5', '6']);
      assert(['5.model', '5.positive', '5.negative', '5.latent_image', '6.vae'].every(p => samplerPorts.includes(p)), 'Sampler+Decode input ports wrong: ' + samplerPorts);
      assert(samplerPorts.includes('6.0') && samplerPorts.includes('IMAGE'), 'Sampler+Decode output missing: ' + samplerPorts);
      const encoderPorts = await extractInEditor('Encodeurs', ['2', '3']);
      assert(encoderPorts.includes('2.clip') && encoderPorts.includes('3.clip') && encoderPorts.includes('CONDITIONING'), 'Encodeurs ports wrong: ' + encoderPorts);
      await nav('Modules');
      const modules = await api('modules');
      assert(modules.length === 2, 'Expected two modules');
      report.checks.push({ extracted: modules.map(m => m.name), visualSelection: true });
      // Detail: engine check, ports, provenance.
      document.querySelector(`[data-module-item="${modules.find(m => m.name === 'Sampler+Decode').id}"]`).click();
      await wait(() => $('[data-module-detail] h2')?.textContent.startsWith('Sampler+Decode'), 'Detail missing');
      await wait(() => $('[data-module-engine]')?.dataset.moduleEngine === 'ok', 'Engine dependency check not ok');
      const inputs = [...document.querySelectorAll('[data-module-inputs] tbody tr')].map(r => r.cells[0].textContent);
      assert(['5.model', '5.positive', '5.negative', '5.latent_image', '6.vae'].every(p => inputs.includes(p)), 'Input ports: ' + inputs);
      assert($('[data-module-outputs]').textContent.includes('IMAGE'), 'Output port missing');
      assert($('[data-module-provenance]').textContent.includes('Smoke source'), 'Provenance missing');
      // Composition with the encoders module: CONDITIONING feeds positive/negative.
      set($('[data-module-partner]'), modules.find(m => m.name === 'Encodeurs').id);
      await wait(() => document.querySelectorAll('[data-composable]').length === 2, 'Composition missing');
      const [forward, backward] = [...document.querySelectorAll('[data-composable]')];
      assert(forward.dataset.composable === 'false' && backward.dataset.composable === 'true', 'Composition verdicts wrong');
      assert(backward.textContent.includes('5.positive') && backward.textContent.includes('CONDITIONING'), 'Composition links missing');
      report.checks.push({ engineChecked: true, composition: 'Encodeurs → Sampler+Decode' });
      // Reuse inside the target workflow.
      await wait(() => [...$('[data-module-target]').options].some(o => o.value === target), 'Target option missing');
      set($('[data-module-target]'), target);
      await wait(() => $('[data-module-candidates]'), 'Candidates missing');
      for (const [port, value] of [['5.model', '1:0'], ['5.positive', '2:0'], ['5.negative', '3:0'], ['5.latent_image', '4:0'], ['6.vae', '1:2']]) set($(`[data-module-input="${port}"]`), value);
      const output = $('[data-module-output]');
      assert([...output.options].some(o => o.value === '7:images' && o.disabled), 'Linked input not disabled');
      set(output, '8:images');
      await click('Prévisualiser l’insertion');
      await wait(() => $('[data-module-preview]'), 'Preview missing');
      assert($('[data-module-preview]').textContent.includes('2 nodes ajoutés') && $('[data-module-preview]').textContent.includes('1 entrée(s) existante(s) recâblée(s)'), 'Preview summary wrong: ' + $('[data-module-preview]').textContent.slice(0, 200));
      await click('Enregistrer comme nouveau workflow');
      await wait(async () => (await api('workflows')).length === 3, 'Copy workflow not saved');
      const targetNodesAfter = await api('workflow_nodes', { id: target });
      assert(JSON.stringify(targetNodesAfter) === JSON.stringify(targetNodesBefore), 'Target workflow modified');
      report.checks.push({ insertPreview: true, copySaved: true, targetUnchanged: true });
      report.expected = { modules: modules.length };
    } else {
      assert((await api('preferences')).selectedTab === 'modules', 'Modules tab not restored');
      await wait(() => document.querySelector('.subtabs button.on[data-nav="modules"]'), 'Modules page not restored');
      await wait(() => document.querySelectorAll('[data-module-item]').length === 2, 'Modules not persisted');
      document.querySelector('[data-module-item]').click();
      await wait(() => $('[data-module-detail]') && $('[data-module-version="1"]'), 'Detail after restart missing');
      report.checks.push({ restart: true, modulesPersisted: 2, workflows: (await api('workflows')).length });
    }
    assert(document.documentElement.scrollWidth <= innerWidth, 'Horizontal overflow');
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
    report.picker = [...document.querySelectorAll('[data-pick-node]')].map(n => n.outerHTML.slice(0, 120));
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
