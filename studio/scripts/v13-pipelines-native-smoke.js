(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  // Isolated Niri fixture + --mock-comfy --mock-accept --restart. No real GPU.
  const report = { ok: false, checks: [], errors: [] };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const wait = async (predicate, message) => {
    window.__SMOKE_STEP = message;
    const end = Date.now() + 25000;
    while (!await predicate()) { if (Date.now() > end) throw Error(message); await new Promise(r => setTimeout(r, 50)); }
  };
  const $ = selector => document.querySelector(selector);
  const click = async text => {
    const button = () => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text);
    await wait(() => button() && !button().disabled, 'Button unavailable: ' + text);
    button().click();
  };
  const set = (el, value, event = 'input') => { assert(el, 'Field missing'); el.value = value; el.dispatchEvent(new Event(event, { bubbles: true })); };
  const choose = async (el, value) => { await wait(() => [...el.options].some(o => o.value === value), 'Option missing ' + value); set(el, value, 'change'); };
  window.addEventListener('error', e => report.errors.push(e.message));
  window.addEventListener('unhandledrejection', e => report.errors.push(String(e.reason)));
  try {
    const restart = window.__V12_SMOKE_PHASE__ === 'restart';
    let pipeline, run, base, upscale;
    if (!restart) {
      const source = '{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},"2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"exact,  double\\nligne"}},"3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"neg"}},"4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},"5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":18446744073709551615,"steps":20,"cfg":6,"sampler_name":"euler","scheduler":"normal","denoise":1}},"6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},"7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"base"}}}';
      base = await api('import_workflow', { name: 'Pipeline base', text: source });
      upscale = await api('import_workflow', { name: 'Pipeline upscale', text: JSON.stringify({
        '1': { class_type: 'LoadImage', inputs: { image: 'fixture-image.png' } },
        '2': { class_type: 'ImageScaleBy', inputs: { image: ['1', 0], upscale_method: 'nearest-exact', scale_by: 2 } },
        '3': { class_type: 'SaveImage', inputs: { images: ['2', 0], filename_prefix: 'upscale' } }
      }) });
    } else ({ pipeline, run, base, upscale } = window.__V12_SMOKE_PREVIOUS__);
    await nav('Pipelines');
    await wait(() => $('[data-pipelines]'), 'Pipelines page missing');
    if (!restart) {
      assert($('[data-pipeline-save]').disabled, 'Invalid pipeline can be saved');
      set($('[data-pipeline-builder] label input'), 'Smoke two passes');
      await choose($('[data-pipeline-step="0"] select'), base);
      await click('Ajouter une passe');
      await choose($('[data-pipeline-step="1"] select'), upscale);
      await choose($('[data-pipeline-step="1"]').querySelectorAll('select')[1], 'passe1');
      await wait(() => $('[data-pipeline-step="1"]').querySelectorAll('select').length === 3, 'LoadImage selector missing');
      await choose($('[data-pipeline-step="1"]').querySelectorAll('select')[2], '1');
      await click('Enregistrer le pipeline');
      await wait(async () => (await api('pipelines')).length === 1, 'Pipeline not saved');
      pipeline = (await api('pipelines'))[0].id;
      await click('Nouvelle exécution');
      await wait(() => $('[data-run-next]'), 'Run not opened');
      run = (await api('pipeline_runs', { pipeline }))[0].run;
    } else {
      await wait(() => $(`[data-pipeline-item="${pipeline}"]`), 'Saved pipeline missing after restart');
      $(`[data-pipeline-item="${pipeline}"]`).click();
      await wait(() => $(`[data-pipeline-run="${run}"]`), 'Saved run missing');
      $(`[data-pipeline-run="${run}"]`).click();
      await wait(() => $('[data-run-next]'), 'Saved run not opened');
      const saved = await api('pipeline_run', { run });
      assert(saved.steps[0].state === 'completed' && saved.steps[1].state === 'pending', 'Restart lost the pass boundary');
      assert(saved.steps[0].job === window.__V12_SMOKE_PREVIOUS__.firstJob, 'First pass was replaced');
    }
    assert($('[data-run-next]').disabled, 'Execution enabled without confirmation');
    $('[data-run-confirm]').click();
    await wait(() => !$('[data-run-next]').disabled, 'Confirmation not applied');
    $('[data-run-next]').click();
    await wait(async () => (await api('pipeline_run', { run })).steps[restart ? 1 : 0].state === 'completed', 'Pass did not complete');
    const detail = await api('pipeline_run', { run });
    if (!restart) {
      assert(detail.steps[1].state === 'pending', 'Second pass executed automatically');
      await wait(() => $('[data-run-next]')?.disabled && !$('[data-run-confirm]').checked, 'Confirmation was reused');
    } else {
      assert(detail.state === 'completed', 'Pipeline incomplete');
      assert(detail.steps[0].job === window.__V12_SMOKE_PREVIOUS__.firstJob, 'Restart resubmitted the first pass');
      assert(detail.steps[1].outputSha256 && detail.steps[1].outputBytes > 0, 'Output not persisted');
    }
    let refused = false;
    try { await api('release_pipeline_memory', { run, confirmed: false }); } catch { refused = true; }
    assert(refused, 'Memory release accepted without confirmation');
    $('[data-memory-panel]').open = true;
    $('[data-memory-measure]').click();
    await wait(() => $('[data-memory-snapshot]'), 'Memory measurements missing');
    assert($('[data-memory-release]').disabled, 'Memory release enabled without confirmation');
    $('[data-memory-confirm]').click();
    await wait(() => !$('[data-memory-release]').disabled, 'Memory confirmation not applied');
    $('[data-memory-release]').click();
    await wait(() => $('[data-memory-outcome]'), 'Memory release report missing');
    assert($('[data-memory-outcome]').textContent.includes('aucune libération mesurée'), 'CPU mock claimed VRAM release');
    assert((await api('pipeline_run', { run })).events.some(e => e.kind === 'memory_release_observed'), 'Memory report not persisted');
    const original = await api('workflow', { id: base });
    assert(original.fields.find(f => f.inputName === 'seed').value === '18446744073709551615', 'Original seed altered');
    assert(original.fields.find(f => f.nodeId === '2' && f.inputName === 'text').value === 'exact,  double\nligne', 'Original prompt altered');
    if (restart) {
      const before = JSON.stringify(await api('pipeline_run', { run }));
      const paths = await api('data_paths');
      const defaultBackup = paths.suggestedBackup + '-sqlite';
      const fullBackup = paths.suggestedBackup + '-states';
      await nav('Données');
      const field = text => [...document.querySelectorAll('label')].find(l => l.textContent.startsWith(text))?.querySelector('input');
      await wait(() => field('Nouveau dossier de sauvegarde')?.value, 'Backup form missing');
      assert(!$('[data-backup-states]').checked, 'Pass-state backup is enabled by default');
      set(field('Nouveau dossier de sauvegarde'), defaultBackup);
      await click('Estimer la taille');
      await wait(() => $('[data-backup-estimation]'), 'Size estimate missing');
      await click('Créer la sauvegarde vérifiée');
      await wait(() => $('[data-backup-report]'), 'Default backup missing');
      const minimal = JSON.parse($('[data-backup-report]').textContent);
      assert(minimal.version === 3 && !minimal.passStatesIncluded && minimal.passStateFiles.length === 0, 'Default backup contains states');
      await wait(() => !$('[data-backup-states]').disabled && !field('Nouveau dossier de sauvegarde').matches(':disabled'), 'Backup still busy');
      $('[data-backup-states]').click();
      set(field('Nouveau dossier de sauvegarde'), fullBackup);
      await click('Estimer la taille');
      await wait(() => $('[data-backup-estimation]')?.textContent.includes('pass-states/'), 'State inventory missing from estimate');
      await click('Créer la sauvegarde vérifiée');
      await wait(() => $('[data-backup-report]') && JSON.parse($('[data-backup-report]').textContent).passStatesIncluded, 'Full backup missing');
      const full = JSON.parse($('[data-backup-report]').textContent);
      assert(full.passStateFiles.length === 3, 'Two states and their manifest expected');
      set(field('Nouveau dossier de restauration'), fullBackup + '-restored');
      await click('Restaurer dans le nouveau dossier');
      await wait(() => $('[data-backup-status]')?.textContent.startsWith('Copie restaurée'), 'Restore missing');
      await wait(() => $('[data-backup-dependencies]'), 'Dependency report missing after restore');
      const depsText = $('[data-backup-dependencies]').textContent;
      assert(depsText.includes('États de pipeline terminés non inclus : 0'), 'Restored full backup lacks states: ' + depsText);
      assert(depsText.includes('jamais sauvegardés'), 'External models not flagged');
      report.checks.push({ dependencyReport: depsText.replace(/\s+/g, ' ').slice(0, 160) });
      const verified = await api('verify_backup', { directory: fullBackup + '-restored' });
      assert(verified.sha256 === full.sha256, 'Restored SQLite differs');
      assert(JSON.stringify(verified.passStateFiles) === JSON.stringify(full.passStateFiles), 'Restored states differ');
      assert(JSON.stringify(await api('pipeline_run', { run })) === before, 'Active pipeline changed during restore');
      await click('Créer la sauvegarde vérifiée');
      await wait(() => document.querySelector('.error') && !$('[data-backup-report]'), 'Overwrite was not refused');
      report.checks.push({ sqliteDefault:true, optionalStates:full.passStateFiles.length, isolatedRestore:true, activePipelineUnchanged:true, overwriteRefused:true });
    }
    assert(report.errors.length === 0, report.errors.join('; '));
    Object.assign(report, { ok: true, pipeline, run, base, upscale, firstJob: detail.steps[0].job });
    report.checks.push({ phase: restart ? 'restart' : 'initial', steps: detail.steps, sourcePreserved: true });
  } catch (e) { report.failure = String(e.stack || e); report.uiErrors = [...document.querySelectorAll('.error')].map(e => e.textContent); }
  await invoke('finish_smoke', { report });
})();
