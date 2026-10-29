(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const api = (action, args = {}) => invoke('studio', {request: {action, ...args}});
  const report = {ok: false, checks: [], errors: []};
  window.addEventListener('error', e => report.errors.push(e.message));
  window.addEventListener('unhandledrejection', e => report.errors.push(String(e.reason)));
  const wait = async fn => {const end = Date.now() + 25000; while (!await fn()) {if (Date.now() > end) throw Error('Timeout: ' + fn); await new Promise(r => setTimeout(r, 100));}};
  const button = (text, root = document) => [...root.querySelectorAll('button')].find(b => b.textContent.trim() === text);
  const set = (el, value, type = 'input') => {if (!el) throw Error('Missing field'); el.value = value; el.dispatchEvent(new Event(type, {bubbles: true}));};
  const label = (text, root = document) => [...root.querySelectorAll('label')].find(l => l.textContent.startsWith(text));
  const assert = (condition, message) => {if (!condition) throw Error(message);};
  const row = name => [...document.querySelectorAll('article')].find(a => a.querySelector('strong')?.textContent.startsWith(name + ' · '));
  const input = (id, name) => document.querySelector(`[data-workflow-node="${id}"][data-workflow-input="${name}"]`).querySelector('textarea,input:not([type=checkbox])');
  const node = (class_type, inputs) => ({class_type, inputs});
  const texts = {p1: '(silver hair:1.2),  exact\nP1', p2: '(black hair:1.1),  exact\nP2', quality: 'quality,  exact\n', action: 'standing together,  exact\n', bg: 'garden,  exact\n', neg: 'blur,  exact\n'};
  const canonical = value => JSON.stringify(value, (_, v) => v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => a.localeCompare(b))) : v);
  const saved = [];
  try {
    await wait(()=>innerWidth===1920 && innerHeight===1080);
    report.viewport = {width:innerWidth,height:innerHeight};
    const status = await api('connection');
    assert(status.connected, 'Isolated engine unavailable');
    assert((await api('models')).checkpoints[0] === 'synthetic.safetensors', 'Not the synthetic engine; refusing submission');
    await wait(async () => (await api('live_state')).connected);
    const people = [], variants = [];
    for (let i = 0; i < 2; i++) {
      people.push(await api('save_character', {id: null, name: 'Journey identity ' + i, identity: {identity_prompt: 'base ' + i, studioTraits: 'Permanent identity ' + i}}));
      variants.push(await api('save_variant', {character: people[i], id: null, name: 'Journey variant ' + i, blocks: {P1: texts[i ? 'p2' : 'p1']}, constraints: {}}));
    }
    for (const scenario of ['SOLO', 'COUPLE', 'ERROR', 'CANCEL', 'LOST']) {
      const coupled = scenario === 'COUPLE', specialized = coupled || scenario === 'SOLO', name = 'Isolated ' + scenario;
      const graph = {
        model: node('CheckpointLoaderSimple', {ckpt_name: 'synthetic.safetensors'}),
        loras: node('Lora Loader (LoraManager)', {model: ['model', 0], clip: ['model', 1], text: '', __lm_autocomplete_meta_text: '', loras: {__value__: [{name: 'synthetic-lora', strength: '0.85', clipStrength: '0.25', active: true, expanded: false, locked: true}]}}),
        latent: node('EmptyLatentImage', {width: 64, height: 64, batch_size: 1}),
        sample: node('KSampler', {model: ['loras', 0], positive: ['combined', 0], negative: ['neg', 0], latent_image: ['latent', 0], seed: 'EXACT_U64', steps: 3, cfg: 5, sampler_name: 'euler', scheduler: 'normal', denoise: 1}),
        decode: node('VAEDecode', {samples: ['sample', 0], vae: ['model', 2]}),
        out: node('SaveImage', {images: ['decode', 0], filename_prefix: scenario})
      };
      for (const key of Object.keys(texts).filter(k => coupled || k !== 'p2')) graph[key] = node('CLIPTextEncode', {text: key.startsWith('p') ? 'original ' + key : texts[key], clip: ['loras', 1]});
      graph.scene = node('ConditioningCombine', {conditioning_1: ['action', 0], conditioning_2: ['bg', 0]});
      graph.qualityScene = node('ConditioningCombine', {conditioning_1: ['quality', 0], conditioning_2: ['scene', 0]});
      if (coupled) graph.people = node('ConditioningCombine', {conditioning_1: ['p1', 0], conditioning_2: ['p2', 0]});
      graph.combined = node('ConditioningCombine', {conditioning_1: [coupled ? 'people' : 'p1', 0], conditioning_2: ['qualityScene', 0]});
      const original = JSON.stringify(graph).replace('"EXACT_U64"', '18446744073709551615');
      const workflow = await api('import_workflow', {name, text: original});
      const roles = {quality: 'QUALITY', p1: 'P1', p2: 'P2', action: 'ACTION', bg: 'BG', neg: 'NEG'};
      const fields = (await api('workflow', {id: workflow})).fields.filter(f => (roles[f.nodeId] && f.inputName === 'text') || (f.nodeId === 'sample' && ['seed', 'steps', 'cfg', 'sampler_name'].includes(f.inputName))).map(f => ({...f, section: roles[f.nodeId] ?? null}));
      await api('save_bindings', {id: workflow, fields});
      await wait(() => document.querySelector('[data-nav="characters"]')); await nav('Personnages');
      await wait(() => document.querySelector('[data-nav="characters"][aria-current="page"]')); await nav('Workflows');
      await wait(() => button(name));
      const profile = document.querySelector('[data-workflow-profile]');
      const actionsBeforeFilter = JSON.stringify(await invoke('smoke_actions'));
      set(profile, coupled ? 'solo' : 'couple', 'change'); await new Promise(r => setTimeout(r, 0));
      assert(!button(name), 'Workflow appears under wrong profile');
      set(profile, coupled ? 'couple' : 'solo', 'change'); await wait(() => button(name));
      assert(JSON.stringify(await invoke('smoke_actions')) === actionsBeforeFilter, 'Profile filter writes data');
      if(coupled){
        const previous=(await api('jobs')).find(j=>j.id===saved[0].job);
        for(let i=0;i<2;i++)await api('reference_edit',{character:people[i],variant:variants[i],asset:previous.outputs[0].assetId,operation:'add'});
      }
      button(name).click();
      await wait(() => document.querySelector('[data-workflow-node="p1"]'));
      if (specialized) {
        const editor = document.querySelector('[data-couple-editor]'); editor.open = true;
        await wait(() => button('Réinitialiser depuis les textes du workflow'));
        assert(editor.querySelectorAll('[data-couple-person]').length === (coupled ? 2 : 1), 'Wrong specialized identity panels');
        assert(editor.querySelector('[data-couple-person="0"] textarea').value === 'original p1', 'Initial mapped prompt not loaded');
        const p1text = editor.querySelector('[data-couple-person="0"] textarea');
        set(p1text, 'temporary edit'); await new Promise(r=>setTimeout(r,0));
        button('Annuler les éditions Couple').click();
        await wait(()=>p1text.value === 'original p1');
        button('Réinitialiser depuis les textes du workflow').click();
        for (let i = 0; i < (coupled ? 2 : 1); i++) {
          const person = editor.querySelector(`[data-couple-person="${i}"]`), selects = person.querySelectorAll('select');
          set(selects[0], people[i], 'change'); await wait(() => [...selects[1].options].some(o => o.value === variants[i])); set(selects[1], variants[i], 'change');
          await wait(()=>person.querySelector('[data-character-context]'));
          const context=person.querySelector('[data-character-context]');
          assert(context.dataset.characterContext===people[i], 'Mixed character context');
          assert(context.textContent.includes('Permanent identity '+i), 'Missing permanent traits');
          context.querySelector('details').open=true;
          if(coupled){
            await wait(()=>context.querySelector('[data-context-reference]'));
            context.querySelector('[data-context-reference]').scrollIntoView();
            await wait(()=>context.querySelector('[data-context-reference] img')?.naturalWidth===320);
            set(selects[1],'','change');await new Promise(r=>setTimeout(r,0));
            assert(!context.querySelector('[data-context-reference]'),'Variant reference leaked into base scope');
            set(selects[1],variants[i],'change');
          }
          context.querySelector('[data-character-history]').open=true;
          if(coupled && i===0)await wait(()=>context.querySelector(`[data-context-job="${saved[0].job}"]`));
          else await wait(()=>context.textContent.includes('Aucune préparation associée.'));
          await new Promise(r => setTimeout(r, 0)); button('Comparer ce preset pour P' + (i + 1)).click();
          await wait(() => editor.querySelector('[data-couple-preview]')); button('Appliquer les sections affichées').click();
          await wait(() => person.querySelector('textarea').value === texts[i ? 'p2' : 'p1']);
        }
        button(coupled ? 'Valider et préparer Saya Couple' : 'Valider et préparer SOLO').click();
      } else {
        const person = document.querySelector('[data-general-person="0"]'), selects = person.querySelectorAll('select');
        set(selects[0], people[0], 'change'); await wait(() => [...selects[1].options].some(o => o.value === variants[0])); set(selects[1], variants[0], 'change');
        await new Promise(r => setTimeout(r, 0)); button('Charger le preset dans P1').click();
        await wait(() => document.querySelector('[data-preset-preview]')); button('Appliquer ce preset à P1').click();
        await wait(() => input('p1', 'text').value === texts.p1);
        // Explicitly editable parameters; no structural change or u64 JS round trip.
        set(input('sample', 'steps'), '4'); set(input('sample', 'cfg'), '6.5');
        button('Valider le mapping et préparer la requête').click();
      }
      await wait(() => document.querySelector('[data-prepared-request]') && !button('Annuler les éditions non appliquées').disabled);
      const requestJson = document.querySelector('[data-prepared-request] pre').textContent, request = JSON.parse(requestJson);
      const job = (await api('jobs')).find(j => j.workflow === name);
      set(profile, 'unconfigured', 'change'); await new Promise(r => setTimeout(r, 0));
      assert(document.querySelector('[data-prepared-request] pre').textContent === requestJson, 'Filtering changed prepared snapshot');
      set(profile, coupled ? 'couple' : 'solo', 'change');
      assert(requestJson.includes('18446744073709551615'), 'Seed changed');
      assert(canonical(request.prompt.loras) === canonical(graph.loras), 'Composite LoRA data changed');
      assert(request.prompt.sample.inputs.steps === (specialized ? 3 : 4) && request.prompt.sample.inputs.cfg === (specialized ? 5 : 6.5), 'Mapped parameters not applied');
      assert(request.characters.length === (coupled ? 2 : 1), 'Wrong identity count');
      for (let i = 0; i < request.characters.length; i++) assert(request.characters[i].variant.id === variants[i] && request.prompt['p' + (i + 1)].inputs.text === texts['p' + (i + 1)], 'Variant mismatch');
      for (const key of ['quality', 'action', 'bg', 'neg']) assert(request.prompt[key].inputs.text === texts[key], 'Shared text changed');
      for (const [id, n] of Object.entries(graph)) for (const [key, value] of Object.entries(n.inputs)) if (Array.isArray(value)) assert(JSON.stringify(request.prompt[id].inputs[key]) === JSON.stringify(value), 'Link changed');
      assert(coupled || !request.prompt.p2, 'SOLO invented P2');
      assert(button('Lancer la génération confirmée').disabled, 'Missing confirmation gate');
      let refused = false;
      try {await api('submit', {job: job.id, hash: '', confirmed: false, allow_removals: false});} catch {refused = true;}
      assert(refused, 'IPC accepted unconfirmed request');
      const prepared = document.querySelector('[data-prepared-request]');
      for (const checkbox of prepared.querySelectorAll('input[type=checkbox]')) checkbox.click();
      await wait(() => !button('Lancer la génération confirmée').disabled);
      const launch = button('Lancer la génération confirmée'); launch.click(); launch.click();
      await wait(() => !document.querySelector('[data-prepared-request]'));
      if (scenario === 'LOST') await wait(() => [...document.querySelectorAll('.error')].some(e => e.textContent.includes('/prompt')));
      if (scenario === 'SOLO' || coupled) {
        await wait(() => row(name)?.querySelector('progress')?.value === 2);
        report.checks.push({scenario, progressVisible: true});
      }
      if (scenario === 'CANCEL') {
        await wait(() => button('Retirer ce job de la file', row(name) ?? document));
        button('Retirer ce job de la file', row(name)).click();
        await wait(() => button('Confirmer l’action ciblée', row(name)));
        assert(button('Confirmer l’action ciblée', row(name)).disabled, 'Cancellation unconfirmed');
        label('Je confirme cette action', row(name)).querySelector('input').click();
        await wait(() => !button('Confirmer l’action ciblée', row(name)).disabled);
        button('Confirmer l’action ciblée', row(name)).click();
      }
      const expected = scenario === 'ERROR' ? 'failed' : scenario === 'CANCEL' ? 'cancelled' : 'succeeded';
      await wait(async () => (await api('jobs')).find(j => j.id === job.id)?.state === expected);
      await wait(() => row(name)?.querySelector('strong')?.textContent.endsWith(expected));
      const final = (await api('jobs')).find(j => j.id === job.id);
      assert((await api('job_details', {id: job.id})).requestJson === requestJson, 'Snapshot mutated after submit');
      if (expected === 'succeeded') {
        row(name).scrollIntoView();
        await wait(() => [...row(name).querySelectorAll('img')].some(i => i.complete && i.naturalWidth === 320));
        assert(final.outputs.length === 1, 'Missing output');
        if(scenario==='SOLO'){
          const panel=row(name).querySelector('[data-job-checkpoints]');panel.open=true;
          await wait(()=>panel.querySelector('[data-capture-checkpoint]'));
          panel.querySelector('[data-capture-checkpoint]').click();
          await wait(()=>panel.querySelector('[data-checkpoint]')&&!panel.querySelector('[data-capture-checkpoint]').disabled);
          const checkpoint=panel.querySelector('[data-checkpoint]');
          button('Vérifier et prévisualiser la copie',checkpoint).click();
          await wait(()=>checkpoint.querySelector('img')?.naturalWidth===1600);
          await wait(()=>!button('Valider cette image',checkpoint).disabled);
          button('Valider cette image',checkpoint).click();
          await wait(()=>checkpoint.querySelector('h4').textContent.includes('validated'));
          const records=await api('checkpoints',{job:job.id});
          assert(records.length===1&&records[0].resumeCapability==='unverified','Image falsely claims resume compatibility');
          assert(JSON.parse(records[0].manifestJson).jobId===job.id&&records[0].manifestJson.includes('18446744073709551615'),'Checkpoint manifest changed exact parameters');
          report.checkpoint={job:job.id,id:records[0].id,manifestJson:records[0].manifestJson};
        }
        const asset = await api('detail', {id: final.outputs[0].assetId});
        assert(asset.generations.some(g => g.jobId === job.id && g.nodeId === 'out' && g.kind === 'saved'), 'Output provenance missing');
        assert((await api('gallery', {filters: {characters: coupled ? people : [people[0]], variant: variants[0], job: job.id}})).total === 1, 'Output identity association missing');
      }
      if (coupled) assert(!(await api('live_state')).connected, 'WebSocket outage not observed');
      if (scenario === 'ERROR') assert(row(name).querySelector('.error')?.textContent.includes('Synthetic node failure'), 'Engine error not visible');
      if (scenario === 'LOST') {
        const events = (await api('job_details', {id: job.id})).events;
        assert(events.some(e => e.type === 'submission_recovered') && events.some(e => e.type === 'submission_uncertain'), 'Submission history incomplete');
        assert(![...document.querySelectorAll('.error')].some(e => e.textContent.includes('/prompt')), 'Recovered submission still displays obsolete transport error');
      }
      assert((await api('workflow', {id: workflow})).fields.find(f => f.nodeId === 'p1').value === 'original p1', 'Imported workflow modified');
      saved.push({workflow, original, job: job.id, state: expected, requestJson, people: coupled ? people : [people[0]], variant: variants[0]});
      report.checks.push({scenario, state: expected, explicitProfile: true, filterPreservesDraft: true, exactSnapshot: true, linksPreserved: true, outputs: final.outputs.length});
    }
    await api('save_variant', {character: people[0], id: variants[0], name: 'Later edit', blocks: {P1: 'later variant edit'}, constraints: {}});
    await nav('Données');
    const backupInput=()=>[...document.querySelectorAll('label')].find(l=>l.textContent.startsWith('Nouveau dossier de sauvegarde'))?.querySelector('input');
    await wait(()=>backupInput()?.value&&!button('Créer la sauvegarde vérifiée').disabled);
    const backupPath=backupInput().value;
    button('Créer la sauvegarde vérifiée').click();
    await wait(()=>document.querySelector('[data-backup-status]')?.textContent==='Sauvegarde créée et vérifiée.');
    const backup=JSON.parse(document.querySelector('[data-backup-report]').textContent);
    assert(backup.version===2&&backup.checkpointFiles.length===1,'Checkpoint missing from backup');
    await wait(()=>!button('Restaurer dans le nouveau dossier').closest('fieldset').disabled);
    const restoreInput=[...document.querySelectorAll('label')].find(l=>l.textContent.startsWith('Nouveau dossier de restauration')).querySelector('input');
    restoreInput.value=backupPath+'-restored';restoreInput.dispatchEvent(new Event('input',{bubbles:true}));
    await wait(()=>!button('Restaurer dans le nouveau dossier').disabled);
    button('Restaurer dans le nouveau dossier').click();
    await wait(()=>document.querySelector('[data-backup-status]')?.textContent.startsWith('Copie restaurée et vérifiée'));
    const restored=await api('verify_backup',{directory:restoreInput.value});
    assert(JSON.stringify(restored)===JSON.stringify(backup),'Checkpoint backup restoration mismatch');
    report.checkpointBackup={source:backupPath,restored:restoreInput.value,files:restored.checkpointFiles.length};
    report.saved = saved;
    if(report.checkpoint)saved[0].checkpoint=report.checkpoint;
    report.ok = !report.errors.length;
  } catch (e) {report.failure = String(e?.message ?? e); report.uiErrors = [...document.querySelectorAll('.error')].map(e => e.textContent); report.live = await api('live_state'); report.articles = [...document.querySelectorAll('article')].map(e=>e.textContent);}
  await invoke('finish_smoke', {report});
})();
