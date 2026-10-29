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
    `"10":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["8",0],"negative":["6",0],"latent_image":["9",0],"seed":18446744073709551615,"steps":20,"cfg":6,"sampler_name":"euler","scheduler":"normal","denoise":1}},` +
    `"11":{"class_type":"VAEDecode","inputs":{"samples":["10",0],"vae":["1",2]}},"12":{"class_type":"SaveImage","inputs":{"images":["11",0],"filename_prefix":"scene"}}}`;


 const choose=async(selector,value)=>{await wait(()=>document.querySelector(selector),'Field '+selector);await select(document.querySelector(selector),value);};
 const tick=selector=>{const el=document.querySelector(selector);assert(el,'Checkbox '+selector);el.checked=true;el.dispatchEvent(new Event('change',{bubbles:true}));};
 const press=async selector=>{await wait(()=>document.querySelector(selector)&&!document.querySelector(selector).disabled,'Ready '+selector);document.querySelector(selector).click();};
 try{
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   const source=await api('import_workflow',{name:'Source à découper',text:graph(false)});
   await nav('Pipelines');await wait(()=>document.querySelector('[data-workflow-split]'),'Split tool');document.querySelector('[data-workflow-split] summary').click();
   await choose('[data-split-workflow]',source);await press('[data-split-analyze]');
   await wait(()=>document.querySelector('[data-split-boundary]'),'Analysis');
   assert([...document.querySelector('[data-split-boundary]').options].some(o=>o.textContent.includes('LATENT')&&!o.disabled),'LATENT should be supported');
   await choose('[data-split-boundary]','10:0');await press('[data-split-add]');
   await choose('[data-split-output]','12');await press('[data-split-preview]');
   await wait(()=>document.querySelector('[data-split-plan]'),'Preview');
   assert(document.querySelector('[data-split-plan]').textContent.includes('pass2'),'Second pass absent');
   assert(document.querySelector('[data-split-save]').disabled,'Unconfirmed split allowed');
   tick('[data-split-confirm]');await press('[data-split-save]');
   await wait(async()=>(await api('pipelines')).length===1,'Pipeline saved');
   const pipeline=(await api('pipelines'))[0].id;
   await wait(()=>document.querySelector('[data-pipeline-item="'+pipeline+'"]')?.classList.contains('on'),'Created pipeline selected');
   await click('Nouvelle exécution');await wait(()=>document.querySelector('[data-run-next]'),'Run ready');
   const run=(await api('pipeline_runs',{pipeline}))[0].run;report.expected={pipeline,run,source};
   assert(document.querySelector('[data-run-next]').disabled,'Run without confirmation');
   tick('[data-run-confirm]');await press('[data-run-next]');
   await wait(()=>document.querySelector('[data-run-step="pass1"]')?.textContent.includes('completed'),'First mock pass completed');
   const r=await api('pipeline_run',{run});assert(r.steps[0].state==='completed'&&r.steps[1].state==='pending','First pass state');
   assert(r.steps[0].outputKind==='latent','First pass is not LATENT');
   report.checks.push('explicit LATENT boundary, typed refusals, preview, new workflow copies and v2 pipeline, confirmation, first synthetic LATENT pass');
  }else{
   report.expected=window.__V12_SMOKE_PREVIOUS__.expected;
   const {pipeline,run}=report.expected;
   await nav('Pipelines');await press('[data-pipeline-item="'+pipeline+'"]');await press('[data-pipeline-run="'+run+'"]');
   await wait(()=>document.querySelector('[data-run-next]'),'Resumed run');
   assert(document.querySelector('[data-run-step="pass1"]').textContent.includes('completed'),'First pass not restored');
   tick('[data-run-confirm]');await press('[data-run-next]');
   await wait(()=>document.querySelector('[data-run-step="pass2"]')?.textContent.includes('completed'),'Second mock pass completed');
   const done=await api('pipeline_run',{run});assert(done.state==='completed','Pipeline incomplete');assert(done.steps[1].outputKind==='image','Second pass is not IMAGE');
   report.checks.push('restart, verified intermediate LATENT reloaded by LoadLatent, second synthetic IMAGE pass completed');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
