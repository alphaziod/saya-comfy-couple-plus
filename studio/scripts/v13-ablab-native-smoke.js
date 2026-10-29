(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  // Requires --mock-comfy --mock-accept (synthetic outputs, no GPU).
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
  const summary = text => [...document.querySelectorAll('summary')].find(s => s.textContent.trim() === text);
  const labeled = text => { const l = [...document.querySelectorAll('label')].find(x => x.textContent.trim().startsWith(text)); return l?.querySelector('input,textarea,select'); };
  const set = (el, value) => { el.value = value; el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); };
  const P1 = 'précis,  double espace\n(poids:1.3)';
  const graph = `{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},` +
    `"2":{"class_type":"LoraLoader","inputs":{"lora_name":"fixture-lora_name.safetensors","strength_model":0.8,"strength_clip":0.8,"model":["1",0],"clip":["1",1]}},` +
    `"3":{"class_type":"CLIPTextEncode","inputs":{"text":"old p1","clip":["2",1]}},"4":{"class_type":"CLIPTextEncode","inputs":{"text":"old neg","clip":["2",1]}},` +
    `"5":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},` +
    `"6":{"class_type":"KSampler","inputs":{"seed":1,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["2",0],"positive":["3",0],"negative":["4",0],"latent_image":["5",0]}},` +
    `"7":{"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}},"8":{"class_type":"SaveImage","inputs":{"filename_prefix":"ab","images":["7",0]}}}`;
  const fields = [['3', 'text', 'P1'], ['4', 'text', 'NEG'], ['6', 'seed', null], ['6', 'cfg', null], ['6', 'sampler_name', null], ['6', 'steps', null], ['2', 'strength_model', null], ['2', 'strength_clip', null]]
    .map(([nodeId, inputName, section]) => ({ nodeId, inputName, value: '', section }));
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    const workflow = await api('import_workflow', { name: 'AB ref', text: graph });
    await api('save_bindings', { id: workflow, fields });
    const job = await api('prepare', { id: workflow, characters: [], edits: [{ nodeId: '3', inputName: 'text', value: P1, section: 'P1' }, { nodeId: '6', inputName: 'seed', value: '18446744073709551615', section: null }] });
    await api('create_recipe', { job: job.jobId, name: 'Recette AB', description: '' });
    await nav('Labo A/B');
    await wait(() => document.querySelector('.subtabs button.on[data-nav="ablab"]'), 'A/B page missing');
    const recipeSelect = $('[data-ab-from-recipe]');
    await wait(() => recipeSelect.options.length > 1, 'Recipe options missing');
    set(recipeSelect, recipeSelect.options[1].value);
    await click('Préremplir');
    await wait(() => labeled('Nom de la campagne').value.startsWith('A/B'), 'Form not prefilled');
    assert(labeled('Seed de base').value === '18446744073709551615', 'Seed not prefilled exactly: ' + labeled('Seed de base').value);
    await click('Créer la campagne');
    await wait(() => $('[data-ab-run]') || [...document.querySelectorAll('h2')].some(h => h.textContent.trim().startsWith('A/B')), 'Campaign not created');
    for (const [slot, label, values] of [['cfg', 'Guidage', '[4.5, 7]'], ['sampler', 'Échantillonneur', '["euler","dpmpp_2m"]']]) {
      summary('Ajouter un paramètre (constant ou variante)').click();
      await wait(() => labeled('Réglage'), 'Parameter form missing');
      set(labeled('Réglage'), slot); set(labeled('Genre'), 'variant'); set(labeled('Libellé'), label); set(labeled('Valeurs (tableau JSON'), values);
      await click('Ajouter le paramètre');
      await wait(() => [...document.querySelectorAll('.ab-params li strong')].some(s => s.textContent.trim() === label), 'Parameter not added: ' + label);
    }
    await click('Préparer la grille (sans exécution)');
    await wait(() => $('[data-ab-plan-summary]')?.textContent.includes('4 cellule(s) préparable(s) sur 4'), 'Plan summary wrong: ' + ($('[data-ab-plan-summary]')?.textContent ?? ''));
    // Controlled comparison: one varying slot is attributed; two are refused.
    const diffA = labeled('Cellule A'), diffB = labeled('Cellule B');
    set(diffA, diffA.options[0].value); set(diffB, diffB.options[1].value);
    await click('Comparer champ par champ');
    await wait(() => $('.ab-diff'), 'Diff table missing');
    assert($('[data-attribution]')?.textContent.includes('admissible'), 'Single-variable attribution missing: ' + ($('[data-attribution]')?.textContent ?? ''));
    set(diffB, diffB.options[3].value);
    await click('Comparer champ par champ');
    await wait(() => $('[data-attribution]')?.textContent.includes('2 paramètres diffèrent'), 'Multi-variable diff accepted: ' + ($('[data-attribution]')?.textContent ?? ''));
    report.checks.push({ singleVariableDiff: true, multiVariableRefused: true, constants: 'prompt/seed/LoRA reference untouched' });
    $('[data-ab-choose="0"]').click(); $('[data-ab-choose="3"]').click();
    await wait(() => $('[data-ab-plan-summary]').textContent.includes('2 sélectionnée(s) → 2 job(s)'), 'Selection count wrong');
    await click('Préparer 2 brouillon(s)');
    await wait(() => document.querySelectorAll('[data-ab-draft]').length === 2, 'Two drafts expected');
    const jobsDraft = (await api('jobs')).filter(j => j.state === 'draft').length;
    // Submit only the first draft, after its own confirmation.
    const first = document.querySelector('[data-ab-draft]');
    first.querySelector('[data-draft-confirm]').click();
    first.querySelector('[data-draft-allow-removals]')?.click();
    const submitFirst = () => [...first.querySelectorAll('button')].find(b => b.textContent.trim() === 'Soumettre à ComfyUI');
    await wait(() => submitFirst() && !submitFirst().disabled, 'Submit not enabled after confirmations');
    submitFirst().click();
    await wait(() => first.querySelector('[data-draft-submitted]'), 'First draft not submitted');
    const second = document.querySelectorAll('[data-ab-draft]')[1];
    assert([...second.querySelectorAll('button')].find(b => b.textContent.trim() === 'Soumettre à ComfyUI').disabled, 'Second draft must stay unconfirmed');
    // Synthetic engine finishes; existing sync records outputs; lab collects them.
    let synced = null;
    await wait(async () => { synced = await api('sync_jobs'); return synced.some(j => j.state === 'succeeded'); }, 'Submitted job never succeeded on mock', 30000);
    await click('Rattacher les résultats');
    await wait(() => document.querySelector('.ab-cell img') || $('.notice')?.textContent.includes('rattachée'), 'Results not collected');
    const noteText = [...document.querySelectorAll('.notice')].map(n => n.textContent).join(' ');
    report.checks.push({ prefilledFromRecipe: true, planned: 4, drafts: 2, draftJobsBefore: jobsDraft, submitted: 1, collected: noteText.match(/(\d+) image/)?.[1] ?? null });
    assert(noteText.includes('1 image(s)'), 'Expected exactly one collected image: ' + noteText);
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
