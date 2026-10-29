(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  // Requires --mock-comfy --mock-accept. The pass stays 120 s in the mock queue:
  // the UI waits (60 s IPC bound), then cancels; nothing completes later.
  const report = { ok: false, checks: [], errors: [] };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const wait = async (predicate, message, timeout = 20000) => {
    window.__SMOKE_STEP = message + '@' + Date.now();
    const end = Date.now() + timeout;
    while (!await predicate()) { if (Date.now() >= end) throw Error(message); await sleep(100); }
  };
  const $ = s => document.querySelector(s);
  const graph = JSON.stringify({
    1: { class_type: 'CheckpointLoaderSimple', inputs: { ckpt_name: 'fixture-ckpt_name.safetensors' } },
    2: { class_type: 'CLIPTextEncode', inputs: { clip: ['1', 1], text: 'slow' } }, 3: { class_type: 'CLIPTextEncode', inputs: { clip: ['1', 1], text: 'neg' } },
    4: { class_type: 'EmptyLatentImage', inputs: { width: 512, height: 512, batch_size: 1 } },
    5: { class_type: 'KSampler', inputs: { model: ['1', 0], positive: ['2', 0], negative: ['3', 0], latent_image: ['4', 0], seed: 1, steps: 20, cfg: 6, sampler_name: 'euler', scheduler: 'normal', denoise: 1 } },
    6: { class_type: 'VAEDecode', inputs: { samples: ['5', 0], vae: ['1', 2] } }, 7: { class_type: 'SaveImage', inputs: { images: ['6', 0], filename_prefix: 'mock-slow-120' } } });
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    const wf = await api('import_workflow', { name: 'Lent', text: graph });
    const pipeline = await api('save_pipeline', { text: JSON.stringify({ format: 'saya-workflow-pipeline', version: 1, id: crypto.randomUUID(), name: 'Annulable', workflows: [{ id: wf }], steps: [{ id: 'base', workflowId: wf, kind: 'image', outputs: [{ name: 'image', portType: 'IMAGE' }] }] }) });
    const run = await api('start_pipeline_run', { pipeline });
    await nav('Pipelines');
    await wait(() => $(`[data-pipeline-item="${pipeline}"]`), 'Pipeline missing');
    $(`[data-pipeline-item="${pipeline}"]`).click();
    await wait(() => $(`[data-pipeline-run="${run}"]`), 'Run missing');
    $(`[data-pipeline-run="${run}"]`).click();
    await wait(() => $('[data-run-confirm]'), 'Run detail missing');
    assert($('[data-run-next]').disabled, 'Next enabled without confirmation');
    $('[data-run-confirm]').click();
    await wait(() => !$('[data-run-next]').disabled, 'Next not enabled');
    $('[data-run-next]').click();
    await wait(() => $('[data-run-summary]')?.textContent.includes('soumises 1'), 'Pass not shown as submitted', 90000);
    report.checks.push({ waiting: $('[data-run-summary]').textContent.slice(0, 80) });
    $('[data-run-cancel-confirm]').click();
    await wait(() => !$('[data-run-cancel]').disabled, 'Cancel not enabled');
    $('[data-run-cancel]').click();
    await wait(() => [...document.querySelectorAll('.notice')].some(n => n.textContent.includes('annulée')), 'Cancellation not confirmed in UI', 20000);
    await wait(() => $('[data-run-summary]')?.textContent.includes('en échec 1'), 'Failed count missing');
    const detail = await api('pipeline_run', { run });
    assert(detail.state === 'paused' && detail.steps[0].state === 'failed', 'Run/pass states wrong: ' + JSON.stringify(detail.steps[0]));
    assert(detail.events.some(e => e.kind === 'cancelled'), 'Cancelled event missing');
    report.checks.push({ cancelled: true, state: detail.state });
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
