(async () => {
  // Requires --mock-comfy --mock-accept. Recipe job → output → gallery.
  // Re-anchored on 2026-10-25 (UI decision, PASSATION_ZCODE_2026-10-25.md §6):
  // the gallery detail panel no longer shows provenance or the
  // "Voir dans l'arbre" button ([data-asset-provenance] / [data-open-tree]
  // removed). Re-anchored on 2026-10-26 (brief point 10): the Tree page is
  // removed from the frontend navigation; the backend data/provenance
  // (generation_tree, generation_branches, plan_tree_branch, job_provenance)
  // are kept and verified here through direct API calls.
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
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const graph = `{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},` +
    `"2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"old"}},"3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"neg"}},` +
    `"4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},` +
    `"5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":3,"steps":20,"cfg":6,"sampler_name":"euler","scheduler":"normal","denoise":1}},` +
    `"6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},"7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"prov"}}}`;
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    const workflow = await api('import_workflow', { name: 'Prov', text: graph });
    await api('save_bindings', { id: workflow, fields: [{ nodeId: '2', inputName: 'text', value: '', section: 'P1' }] });
    const base = await api('prepare', { id: workflow, edits: [{ nodeId: '2', inputName: 'text', value: 'base', section: 'P1' }], characters: [] });
    const recipe = await api('create_recipe', { job: base.jobId, name: 'Recette provenance', description: '' });
    const job = await api('prepare', { id: workflow, edits: [{ nodeId: '2', inputName: 'text', value: 'depuis recette', section: 'P1' }], characters: [], origin: { kind: 'recipe', id: recipe.id } });
    await api('submit', { job: job.jobId, hash: job.snapshotHash, confirmed: true, allow_removals: true });
    await wait(async () => (await api('sync_jobs')).some(j => j.id === job.jobId && j.state === 'succeeded'), 'Job not succeeded', 30000);
    const provenance = await api('job_provenance', { id: job.jobId });
    assert(provenance.origin?.kind === 'recipe' && provenance.outputs.length === 1, 'Job provenance incomplete: ' + JSON.stringify(provenance).slice(0, 300));
    const asset = provenance.outputs[0].asset;
    const checkpoint = await api('capture_checkpoint', { job: job.jobId, asset, max_bytes: 1024 * 1024 * 1024 });
    await api('review_checkpoint', { id: checkpoint.id, accept: true, note: 'smoke' });
    await api('verify_checkpoint', { id: checkpoint.id });
    report.checks.push({ checkpoint: checkpoint.id, reviewed: true });
    // Gallery: the produced image lands in the catalog stream and opens in the
    // viewer; the compact metadata line ties the image back to its job.
    await nav('Workflows');
    await wait(() => !document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Left gallery');
    await nav('Galerie');
    await wait(() => document.querySelector(`[data-stream-asset="${asset}"]`), 'Output not in gallery', 20000);
    document.querySelector(`[data-stream-asset="${asset}"]`).click();
    await wait(() => $('[data-generation-metadata] [data-generation-mode]'), 'Gallery detail metadata missing');
    const modeLine = $('[data-generation-mode]').textContent.replace(/\s+/g, ' ');
    assert(modeLine.includes('job Saya Studio'), 'Detail line does not tie the image to its job: ' + modeLine);
    report.checks.push({ galleryDetail: modeLine.slice(0, 160) });
    // V1: the detail is in the inspector, no viewer to close.
    // Tree removed from the navigation (brief point 10): no more "Arbre" button
    // nor "Arbre des générations" page. The backend data/provenance
    // remain: lineage, branches, branch prepared from a checkpoint,
    // all through the same Tauri commands, verified here without an interface.
    assert(![...document.querySelectorAll('[data-nav]')].some(n => n.textContent.trim() === 'Arbre'), 'Arbre nav must be gone');
    const tree = await api('generation_tree', { root: job.jobId });
    assert(tree?.root?.jobId === job.jobId, 'generation_tree backend missing');
    const beforeBranches = (await api('generation_branches', { id: job.jobId })).branches.length;
    await api('plan_tree_branch', { parent: job.jobId, relation: 'checkpoint_restore', label: 'reprise smoke', checkpoint: checkpoint.id });
    const afterBranches = (await api('generation_branches', { id: job.jobId })).branches.length;
    assert(afterBranches > beforeBranches, 'planned branch missing from backend');
    const planned = (await api('generation_tree', { root: job.jobId })).root.children.some(child => child.planned);
    assert(planned, 'planned branch not rendered in backend tree');
    report.checks.push({ treeBackendConserved: true, breadcrumbRoot: job.jobId.slice(0, 8), checkpointRestore: true });
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
    try { const g = await api('gallery', { filters: {} }); report.gallery = { total: g.total, paths: g.items.map(i => i.path + ' ' + i.id) }; report.stream = document.querySelector('[data-stream-status]')?.outerHTML.slice(0, 300) ?? null; report.h1 = document.querySelector('h1')?.textContent; } catch (e) { report.gallery = String(e); }
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
