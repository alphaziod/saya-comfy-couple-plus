(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(f,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+25000;while(!await f()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,50))}};
 const root='__SMOKE_REPO_ROOT__';
 const button=t=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===t);
 const active=()=>document.querySelector('[data-temporary-workflow]')?.getAttribute('data-temporary-workflow');
 try{
 await nav('Génération');await wait(()=>document.querySelector('[data-generation-workspace]'),'workspace');
 const workflows=JSON.stringify(await api('workflows')),sessions=JSON.stringify(await api('graph_sessions'));
 for(const [path,kind] of [['/tmp/saya-normal-workflow.png','PNG UI only'],[root+'/scripts/fixtures/v15.2-subgraph-ui.json','normal JSON']]){
  await invoke('smoke_drop_image',{path});await wait(()=>active(),'open '+kind);
  const session=active(),view=await api('graph_view',{session});assert(view.sourceFormat==='comfy_ui','lost UI source');const source=JSON.parse(view.sourceDetails);assert(source.definitions.subgraphs.length>0,'lost subgraphs');const enabled=w=>(w.nodes??[]).every(n=>n.mode===0)&&(w.definitions?.subgraphs??[]).every(enabled);assert(enabled(source),'disabled nodes remain after import');assert(!document.querySelector('[data-image-restore]'),'extra lower panel');
  button('Nodes par passe').click();await wait(()=>document.querySelector('[data-scope-node="7"][data-input="seed"]')&&!button('Nodes par passe').disabled,'subgraph settings');
  if(kind==='PNG UI only'){const seed=document.querySelector('[data-scope-node="7"][data-input="seed"]');seed.value='9007199254740993';seed.dispatchEvent(new Event('input',{bubbles:true}));seed.dispatchEvent(new Event('change',{bubbles:true}));}
  assert(document.querySelectorAll('[data-saya-couple]').length===1,'must reuse existing renderer');assert(!document.querySelector('[data-image-confirm]')&&!document.querySelector('[data-image-submit]'),'separate launch interface remains');
  await wait(()=>!document.querySelector('[data-saya-start]').disabled,'existing launch ready');document.querySelector('[data-saya-start]').click();
  await wait(()=>document.querySelector('[data-temporary-job]'),'submit '+kind);assert(!document.querySelector('.error'),'conversion error');
  await wait(()=>document.querySelector('[data-saya-phase="1"]')?.getAttribute('data-state')==='succeeded'&&document.querySelector('.run-preview img'),'image rendered by existing generation component');assert(document.querySelector('[data-run-progress] progress').max===1,'wrong number of passes');
  const job=await api('job_details',{id:document.querySelector('[data-temporary-job]').getAttribute('data-temporary-job')});
  // The backend snapshot is text: seed validation never passes through a JS Number.
  assert(job.requestJson.includes(kind==='PNG UI only'?'9007199254740993':'18446744073709551615'),'seed precision lost');
  assert(JSON.stringify(await api('workflows'))===workflows,'saved workflow pollution');assert(JSON.stringify(await api('graph_sessions'))===sessions,'temporary saved');
  document.querySelector('[data-image-close]').click();await wait(()=>!active(),'close '+kind);report.checks.push(kind+' opened, subgraph converted and submitted with exact seed');
 }
 report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
