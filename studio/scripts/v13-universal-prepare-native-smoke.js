(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const report = { ok: false, checks: [], errors: [] };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    const created = await api('save_character', { id:null, name:'Alpha', identity:{identity_prompt:'full P1,  exact\nligne', anatomy_prompt:'anatomie exacte'} });
    await api('save_variant', { character:created, id:null, name:'BG Garden', blocks:{BG:'jardin exact'}, constraints:{} });
    const card = { id:created, name:'Alpha' };
    const sheet = await api('character_sheet', { id: card.id });
    assert(typeof sheet.identityPrompt === 'string' && sheet.identityPrompt.length > 0, 'Character P1 prompt missing');
    const graph = {
      '1':{class_type:'CheckpointLoaderSimple',inputs:{ckpt_name:'fixture-ckpt_name.safetensors'}},
      '2':{class_type:'CLIPTextEncode',inputs:{text:'old P1',clip:['1',1]}},
      '3':{class_type:'CLIPTextEncode',inputs:{text:'old anatomy',clip:['1',1]}},
      '4':{class_type:'CLIPTextEncode',inputs:{text:'old BG',clip:['1',1]}},
      '5':{class_type:'CLIPTextEncode',inputs:{text:'old action',clip:['1',1]}},
      '6':{class_type:'EmptyLatentImage',inputs:{width:512,height:512,batch_size:1}},
      '7':{class_type:'KSampler',inputs:{seed:42,steps:20,cfg:6.0,sampler_name:'euler',scheduler:'normal',denoise:1.0,model:['1',0],positive:['2',0],negative:['4',0],latent_image:['6',0]}},
      '8':{class_type:'VAEDecode',inputs:{samples:['7',0],vae:['1',2]}},
      '9':{class_type:'SaveImage',inputs:{images:['8',0],filename_prefix:'universal'}}
    };
    const workflow = await api('import_workflow', { name:'Universal fixture', text:JSON.stringify(graph) });
    const original = (await api('workflow', { id: workflow })).fields.map(f=>[f.nodeId,f.inputName,f.value]);
    for (const [key,node,input] of [['P1','2','text'],['ANATOMY_P1','3','text'],['BG','4','text'],['ACTION','5','text'],['QUICK_STEPS','7','steps']]) {
      await api('save_semantic_binding', { id:workflow, key, node, input });
    }
    const config = {
      p1:{characterId:card.id,variantId:null}, p2:{characterId:''},
      bg:{source:'fixed',text:'jardin exact',selected:'jardin exact'},
      action:{source:'fixed',text:'running',selected:'running'},
      mappings:{P1:{nodeId:'2',inputName:'text'},ANATOMY_P1:{nodeId:'3',inputName:'text'},BG:{nodeId:'4',inputName:'text'},ACTION:{nodeId:'5',inputName:'text'}},
      quick:[{nodeId:'7',inputName:'steps',value:'33',visible:true}],
      favorites:[card.id]
    };
    const profile = await api('save_prepare_profile', { id:null, name:'Native universal', workflow, mode:'solo', config });
    const preview = await api('prepare_universal', { id:profile.id });
    assert(preview.resolution.p1Name === sheet.name, 'P1 name mismatch');
    assert(preview.resolution.bg === 'jardin exact', 'BG not resolved');
    assert(preview.resolution.action === 'running', 'Action not resolved');
    const prepared = JSON.parse(preview.prepared.requestJson);
    assert(prepared.prompt['2'].inputs.text === sheet.identityPrompt, 'P1 prompt not injected');
    assert(prepared.prompt['3'].inputs.text === sheet.anatomyPrompt, 'Anatomy not injected');
    assert(prepared.prompt['4'].inputs.text === 'jardin exact', 'BG not injected');
    assert(prepared.prompt['5'].inputs.text === 'running', 'Action not injected');
    assert(prepared.prompt['7'].inputs.steps === 33, 'Quick steps not injected');
    const after = (await api('workflow', { id: workflow })).fields.map(f=>[f.nodeId,f.inputName,f.value]);
    assert(JSON.stringify(after) === JSON.stringify(original), 'Original workflow values changed');
    assert((await api('prepare_bindings', { id:workflow })).length === 5, 'Bindings missing');
    await nav('Génération');
    await new Promise(r => setTimeout(r, 350));
    await wait(() => document.querySelector('[data-generation-prepare]'), 'Universal preparation page missing');
    report.checks.push({ workflow, profile: profile.id, p1: sheet.name, bg: 'jardin exact', action: 'running', steps: 33, originalUnchanged: true, page: true });
    assert(report.errors.length === 0, report.errors.join(';'));
    report.ok = true;
  } catch (error) { report.failure = String(error?.message || error); report.uiErrors = [...document.querySelectorAll('.error')].map(n => n.textContent); }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
