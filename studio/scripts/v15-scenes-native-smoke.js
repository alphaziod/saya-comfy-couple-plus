(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const click=async(text)=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click()};
 const select=async(el,value)=>{await wait(()=>el&&[...el.options].some(o=>o.value===String(value)),'Select option');el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
  const text = n => `{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":${JSON.stringify(n)}}}`;
  const graph = couple => `{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},` +
    `"2":${text('old p1')},"3":${text('old p2')},"4":${text('old action')},"5":${text('old bg')},"6":${text('old neg')},` +
    `"7":{"class_type":"ConditioningCombine","inputs":{"conditioning_1":["2",0],"conditioning_2":["${couple ? 3 : 4}",0]}},` +
    `"8":{"class_type":"ConditioningCombine","inputs":{"conditioning_1":["7",0],"conditioning_2":["5",0]}},` +
    `"9":{"class_type":"EmptyLatentImage","inputs":{"width":1024,"height":1024,"batch_size":1}},` +
    `"10":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["8",0],"negative":["6",0],"latent_image":["9",0],"seed":5,"steps":20,"cfg":6,"sampler_name":"euler","scheduler":"normal","denoise":1}},` +
    `"11":{"class_type":"VAEDecode","inputs":{"samples":["10",0],"vae":["1",2]}},"12":{"class_type":"SaveImage","inputs":{"images":["11",0],"filename_prefix":"scene"}}}`;

 try{
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   const workflow=await api('import_workflow',{name:'Grok target',text:graph(false)});
   await api('save_bindings',{id:workflow,fields:[['2','P1'],['4','ACTION'],['5','BG'],['6','NEG']].map(([nodeId,section])=>({nodeId,inputName:'text',value:'',section}))});
   const character=await api('save_character',{id:null,name:'Grok actor',identity:{identity_prompt:' actor exact\n'}});
   await nav('Scènes');await click('Importer des scènes…');
   const input=document.querySelector('[data-grok-scene-paths]');input.value=window.__V15_GROK_SCENE_FILE__;input.dispatchEvent(new Event('input',{bubbles:true}));
   await click('Analyser les scènes');await wait(()=>document.querySelectorAll('[data-grok-scene-entry]').length===1,'One scene');
   const box=document.querySelector('[data-grok-scene-entry]');box.checked=true;box.dispatchEvent(new Event('change',{bubbles:true}));
   await click('Confirmer les scènes (1)');await wait(()=>document.querySelector('[data-grok-scene-report]'),'Scene stored');
   const close=[...document.querySelector('.grok-scene-dialog').querySelectorAll('button')].find(b=>b.textContent==='Fermer');await wait(()=>!close.disabled,'Close ready');close.click();
   await wait(()=>document.querySelector('[data-grok-scene-variant]'),'Variant list');
   await select(document.querySelector('[data-grok-scene-variant]'),1);document.querySelector('[data-grok-scene-use]').click();
   await wait(()=>document.querySelector('[data-scene-editor]'),'Composer draft');
   await select(document.querySelector('[data-scene-workflow]'),workflow);
   await wait(()=>document.querySelector('[data-scene-person-select="0"]'),'P1 ready');await select(document.querySelector('[data-scene-person-select="0"]'),character);
   // F12: explicit zone, visual drag + resize, saved with the scene.
   document.querySelector('[data-scene-zone-enable="0"]').click();
   await wait(()=>document.querySelector('[data-scene-zone="0"]'),'P1 zone');
   const stage=document.querySelector('[data-scene-stage]'),zone=document.querySelector('[data-scene-zone="0"]'),sr=stage.getBoundingClientRect();
   const x0=Number(document.querySelector('[data-scene-zone-field="0-x"]').value);
   zone.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,clientX:sr.left+20,clientY:sr.top+20}));
   stage.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,button:0,clientX:sr.left+70,clientY:sr.top+50}));
   stage.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,button:0,clientX:sr.left+70,clientY:sr.top+50}));
   await wait(()=>Number(document.querySelector('[data-scene-zone-field="0-x"]').value)>x0,'zone moved');
   const width0=Number(document.querySelector('[data-scene-zone-field="0-width"]').value);
   document.querySelector('[data-scene-zone-resize="0"]').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,clientX:sr.left+300,clientY:sr.top+300}));
   stage.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,button:0,clientX:sr.left+340,clientY:sr.top+330}));
   stage.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,button:0,clientX:sr.left+340,clientY:sr.top+330}));
   await wait(()=>Number(document.querySelector('[data-scene-zone-field="0-width"]').value)>width0,'zone resized');
   await click("Assembler l'aperçu complet");await wait(()=>document.querySelector('[data-scene-preview-result]'),'Preview');
   assert(document.querySelector('[data-scene-preview-zone="P1"]'),'zone missing from preview');
   assert(document.querySelector('[data-scene-prompt="BG"] pre:last-of-type').textContent===' room  \nété 🌸','BG altered');
   await click('Enregistrer la nouvelle scène');await wait(async()=>(await api('scenes')).length===1,'Saved scene');
   const scene=(await api('scenes'))[0];report.expected={id:scene.id};
   const stored=await api('scene',{id:scene.id});assert(stored.manifest.grokSource.variantIndex===1,'Source variant lost');
   assert(stored.manifest.p1.zone&&stored.manifest.p1.zone.width>width0,'Saved zone lost');
   await click('Préparer la requête du runner (sans envoi)');await wait(()=>document.querySelector('[data-prepared-draft]'),'Prepared draft');
   const job=await api('job_details',{id:document.querySelector('[data-prepared-draft]').dataset.preparedDraft});assert(JSON.parse(job.requestJson).prompt['5'].inputs.text===' room  \nété 🌸','Runner prompt altered');
   assert(button('Soumettre à ComfyUI').disabled,'Unconfirmed submission enabled');
   report.checks.push('static Python data import, scene/intime grouping, explicit variant selection, exact BG in composer preview and runner draft, provenance retained, zero submission; explicit zone drag/resize saved and restored');
  }else{
   report.expected=window.__V12_SMOKE_PREVIOUS__.expected;
   const scene=await api('scene',{id:report.expected.id});assert(scene.manifest.bg.text===' room  \nété 🌸','Restart prompt lost');assert(scene.manifest.grokSource.variantIndex===1,'Restart provenance lost');assert(scene.manifest.p1.zone&&scene.manifest.p1.zone.enabled,'Restart zone lost');
   assert((await api('grok_scenes')).length===1,'Scene library lost');report.checks.push('source library and composed scene persist after restart');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
