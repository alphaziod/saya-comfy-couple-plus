(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  // Requires --mock-comfy (refusing): SOLO and COUPLE scenes prepared, never sent.
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
  const select = async (el, value, what) => { await wait(() => el && [...el.options].some(o => o.value === value), 'Option missing: ' + what); el.value = value; el.dispatchEvent(new Event('change', { bubbles: true })); };
  const text = n => `{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":${JSON.stringify(n)}}}`;
  const graph = couple => `{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},` +
    `"2":${text('old p1')},"3":${text('old p2')},"4":${text('old action')},"5":${text('old bg')},"6":${text('old neg')},` +
    `"7":{"class_type":"ConditioningCombine","inputs":{"conditioning_1":["2",0],"conditioning_2":["${couple ? 3 : 4}",0]}},` +
    `"8":{"class_type":"ConditioningCombine","inputs":{"conditioning_1":["7",0],"conditioning_2":["5",0]}},` +
    `"9":{"class_type":"EmptyLatentImage","inputs":{"width":1024,"height":1024,"batch_size":1}},` +
    `"10":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["8",0],"negative":["6",0],"latent_image":["9",0],"seed":5,"steps":20,"cfg":6,"sampler_name":"euler","scheduler":"normal","denoise":1}},` +
    `"11":{"class_type":"VAEDecode","inputs":{"samples":["10",0],"vae":["1",2]}},"12":{"class_type":"SaveImage","inputs":{"images":["11",0],"filename_prefix":"scene"}}}`;
  const map = sections => sections.map(([node, section]) => ({ nodeId: node, inputName: 'text', value: '', section }));
  const P1 = '(silver hair:1.2),  exact\nsecond line 🌸';
  const ACTION = 'holds (a rose:1.1)\n  gently';
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    const solo = await api('import_workflow', { name: 'Scène solo', text: graph(false) });
    await api('save_bindings', { id: solo, fields: map([['2', 'P1'], ['4', 'ACTION'], ['5', 'BG'], ['6', 'NEG']]) });
    const couple = await api('import_workflow', { name: 'Scène couple', text: graph(true) });
    await api('save_bindings', { id: couple, fields: map([['2', 'P1'], ['3', 'P2'], ['4', 'ACTION'], ['5', 'BG'], ['6', 'NEG']]) });
    const mira = await api('save_character', { id: null, name: 'Mira', identity: { identity_prompt: 'mira identity' } });
    const sport = await api('save_variant', { character: mira, id: null, name: 'Sport', blocks: { P1, ACTION }, constraints: {} });
    const noa = await api('save_character', { id: null, name: 'Noa', identity: { identity_prompt: 'noa,  exact' } });
    await nav('Scènes');
    await wait(() => document.querySelector('.subtabs button.on[data-nav="scenes"]'), 'Scenes page missing');

    for (const [name, workflow, people] of [['Smoke SOLO', solo, [mira]], ['Smoke COUPLE', couple, [mira, noa]]]) {
      await click('Nouvelle scène');
      await wait(() => $('[data-scene-editor]'), 'Editor missing');
      const nameInput = [...document.querySelectorAll('[data-scene-editor] label')].find(l => l.textContent.startsWith('Nom de la scène')).querySelector('input');
      nameInput.value = name; nameInput.dispatchEvent(new Event('input', { bubbles: true }));
      await select($('[data-scene-workflow]'), workflow, 'workflow');
      for (const [index, id] of people.entries()) {
        await wait(() => $(`[data-scene-person-select="${index}"]`), 'Person selector missing ' + index);
        await select($(`[data-scene-person-select="${index}"]`), id, 'person ' + index);
      }
      const variant = $('[data-scene-person="0"]').querySelectorAll('select')[1];
      await select(variant, sport, 'variant');
      const action = $('[data-scene-section="ACTION"] select');
      await select(action, 'preset', 'ACTION preset');
      await click("Assembler l'aperçu complet");
      await wait(() => $('[data-scene-preview-result]'), 'Preview missing for ' + name);
      const p1 = $('[data-scene-prompt="P1"]').querySelectorAll('pre');
      assert(p1[p1.length - 1].textContent === P1, 'P1 not exact: ' + JSON.stringify(p1[p1.length - 1].textContent));
      assert($('[data-scene-prompt="ACTION"]').querySelector('pre:last-of-type').textContent === ACTION, 'ACTION not exact');
      if (people.length === 2) assert([...$('[data-scene-prompt="P2"]').querySelectorAll('pre')].pop().textContent === 'noa,  exact', 'P2 not exact');
      else assert(!$('[data-scene-prompt="P2"]'), 'SOLO preview must not include P2');
      await click(name === 'Smoke SOLO' ? 'Enregistrer la nouvelle scène' : 'Enregistrer la nouvelle scène');
      await wait(async () => (await api('scenes')).some(s => s.name === name), 'Scene not saved: ' + name);
      await click('Préparer la requête du runner (sans envoi)');
      await wait(() => $('[data-prepared-draft]'), 'Draft missing for ' + name);
      const count = Number($('[data-draft-change-count]').dataset.draftChangeCount);
      const draft = await api('job_details', { id: $('[data-prepared-draft]').dataset.preparedDraft });
      const prompt = JSON.parse(draft.requestJson).prompt;
      assert(prompt['2'].inputs.text === P1 && prompt['4'].inputs.text === ACTION, 'Prepared texts not exact');
      assert(people.length === 2 ? prompt['3'].inputs.text === 'noa,  exact' : prompt['3'].inputs.text === 'old p2', 'P2 handling wrong');
      assert(prompt['10'].inputs.positive[0] === '8' && prompt['7'].inputs.conditioning_1[0] === '2', 'Connections changed');
      assert(button('Soumettre à ComfyUI').disabled, 'Submit enabled without confirmation');
      if (people.length === 1) {
        const input = $('[data-draft-recipe-name]');
        input.value = 'Recette de scène'; input.dispatchEvent(new Event('input', { bubbles: true }));
        await wait(() => !$('[data-draft-save-recipe]').disabled, 'Save recipe disabled');
        $('[data-draft-save-recipe]').click();
        await wait(() => $('[data-draft-recipe-saved]'), 'Recipe not saved from draft');
        const recipe = await api('recipe', { id: $('[data-draft-recipe-saved]').dataset.draftRecipeSaved });
        assert(JSON.stringify(recipe.content).includes(JSON.stringify(P1).slice(1, -1)), 'Recipe from scene lost the exact P1');
        assert((recipe.content.characters ?? []).length === 1, 'Recipe lost the scene character');
        report.checks.push({ recipeFromSceneDraft: recipe.id });
      }
      report.checks.push({ scene: name, changes: count, exact: true, roles: draft.characters?.length ?? null });
    }
    assert((await api('scenes')).length === 2, 'Expected two saved scenes');
    const original = await api('workflow', { id: couple });
    assert(original.fields.find(f => f.nodeId === '2').value === 'old p1', 'Stored workflow modified');
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
