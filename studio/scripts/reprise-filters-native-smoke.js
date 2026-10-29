(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],errors:[]},invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),pause=ms=>new Promise(r=>setTimeout(r,ms));
 window.__SMOKE_STAGE=report;
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(fn,m)=>{const end=Date.now()+30000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(100)}};
 const button=t=>[...document.querySelectorAll('button')].find(n=>n.textContent.trim()===t);
 const count=()=>Number(document.querySelector('[data-gallery-count] strong')?.textContent);
 const checkpoint=t=>[...document.querySelectorAll('[data-checkpoint-filter] label')].find(n=>n.textContent.trim()===t)?.querySelector('input');
 const lora=async v=>{const s=[...document.querySelectorAll('.filter-drawer label')].find(n=>n.textContent.startsWith('Ajouter une LoRA'))?.querySelector('select');assert(s,'LoRA selector');s.value=v;s.dispatchEvent(new Event('change',{bubbles:true}));await pause(150)};
 try{
 await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled,'index');button('Indexer / actualiser').click();await wait(async()=>(await api('gallery',{filters:{}})).total===134&&!(await api('index_status')).running,'index complete');
 await wait(()=>checkpoint('Fixture-A'),'facets');document.querySelector('.filter-drawer').open=true;
 checkpoint('Fixture-A').click();await wait(()=>count()===2,'checkpoint A');checkpoint('Fixture-B').click();await wait(()=>count()===4,'union checkpoints');
 assert(document.querySelector('[data-checkpoint-chips]').textContent.includes('(2)'),'chips');
 await lora('A');await wait(()=>count()===4,'A in union');await lora('B');await wait(()=>count()===2,'A+B inclusive');document.querySelector('[data-lora-exact]').click();await wait(()=>count()===1,'AB exact');
 const prefs=(await api('preferences')).galleryFilters;assert(prefs.checkpoints.length===2&&prefs.loras.length===2&&prefs.loraExact,'filter persistence');
 button('B ×').click();await lora('C');await wait(()=>count()===1,'AC exact remains distinct');
 const filtered=await api('gallery',{filters:(await api('preferences')).galleryFilters});assert(filtered.items[0].path.endsWith('filter-ac.png'),'AC result');
 await nav('Personnages');await pause(400);await nav('Galerie');await wait(()=>count()===1,'navigation filtered count');await wait(()=>document.querySelector('[data-lora-exact]')?.checked,'exact restored');
 report.checks.push('checkpoint union 2+2=4','LoRA intersection 4 to 2','exact AB vs ABC','exact AC distinct','chips and navigation persistence');report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
