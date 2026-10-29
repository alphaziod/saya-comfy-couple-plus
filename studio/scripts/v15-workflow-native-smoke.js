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


 const fill=(el,value)=>{assert(el,'Missing field');el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));};
 const openPath=async(path,count)=>{fill(document.querySelector('[aria-label="Chemin workflow JSON"]'),path);await wait(()=>!document.querySelector('[data-graph-open-path]').disabled,'Open button ready');document.querySelector('[data-graph-open-path]').click();await wait(()=>document.querySelectorAll('[data-graph-node]').length===count && !document.querySelector('[data-graph-open-path]').disabled,'File opened');};
 try{
  await nav('Éditeur graphique');
  await wait(()=>document.querySelector('[data-graph-editor]'),'Graph ready');
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   const original=await api('import_workflow',{name:'UI roundtrip',text:graph(false)});
   const opened=await api('graph_open',{workflow:original});
   const file=await api('graph_save_ui',{session:opened.session,name:'Native source'});
   await api('graph_close',{session:opened.session});
   await openPath(file.path,12);
   assert(!document.querySelector('[data-graph-export]').disabled,'Convertible UI refused');
   const sampler=[...document.querySelectorAll('[data-graph-node]')].find(n=>n.textContent.includes('KSampler'));
   sampler.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,clientX:10,clientY:10}));
   window.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,button:0}));
   await wait(()=>document.querySelector('[data-graph-param="seed"]'),'Seed field');
   assert(document.querySelector('[data-graph-param="seed"]').value==='18446744073709551615','Seed rounded');
   const textNode=[...document.querySelectorAll('[data-graph-node]')].find(n=>n.textContent.includes('CLIPTextEncode'));
   textNode.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,clientX:10,clientY:10}));
   window.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,button:0}));
   await wait(()=>document.querySelector('[data-graph-param="text"]'),'Prompt field');
   fill(document.querySelector('[data-graph-param="text"]'),'  exact,\nété 🌸  ');
   const apply=[...document.querySelectorAll('button')].find(b=>b.dataset.graphApply==='text');
   assert(apply,'Apply prompt');apply.click();
   await new Promise(r=>setTimeout(r,300));
   await wait(()=>!document.querySelector('[data-graph-save-ui]').disabled,'Edit finished');
   document.querySelector('[data-graph-save-ui]').click();
   await wait(()=>[...document.querySelectorAll('.notice')].some(n=>n.textContent.startsWith('Copie UI enregistrée : ')),'Saved UI');
   const path=[...document.querySelectorAll('.notice')].find(n=>n.textContent.startsWith('Copie UI enregistrée : ')).textContent.slice('Copie UI enregistrée : '.length);
   report.expected={path,original};
   document.querySelector('[data-graph-export]').click();
   await wait(()=>[...document.querySelectorAll('.notice')].some(n=>n.textContent.includes('Nouveau workflow')),'Converted API saved');
   assert((await api('workflows')).length===2,'API copy missing');
   report.checks.push('native file opening, full graph, exact max seed, multiline prompt edit, UI copy and API revision');
   await openPath('__SMOKE_COMFYUI_DIR__/user/default/workflows/ILUSTCOUPLECLEAN.json',109);
   assert(document.querySelector('[data-graph-export]').disabled,'Unqualified real graph exported');
   assert(document.querySelectorAll('[data-graph-edge]').length===211,'Real links lost');
   assert(document.querySelectorAll('[data-graph-frame="subgraph"]').length>0,'Subgraphs invisible');
   assert(document.querySelector('[data-conversion-diagnostics]').textContent.includes('subgraph'),'Missing diagnostics');
   report.checks.push('real Saya Couple UI: 109 nodes, 211 links, subgraphs and diagnostics visible; incomplete API refused');
  }else{
   report.expected=window.__V12_SMOKE_PREVIOUS__.expected;
   await openPath(report.expected.path,12);
   const details=await api('graph_open_file',{path:report.expected.path});
   assert(Object.values(details.view.paramText).some(p=>p.text==='  exact,\nété 🌸  '),'Saved prompt lost');
   assert(Object.values(details.view.paramText).some(p=>p.seed==='18446744073709551615'),'Saved seed lost');
   assert(details.view.exportable,'Saved UI not convertible');
   await api('graph_close',{session:details.session});
   report.checks.push('saved UI reopens after restart with exact prompt and seed');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
