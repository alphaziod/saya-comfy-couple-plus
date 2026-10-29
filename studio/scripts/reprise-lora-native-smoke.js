(async()=>{
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),pause=ms=>new Promise(r=>setTimeout(r,ms));
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(fn,m)=>{const end=Date.now()+25000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(100)}};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click();await pause(300)};
 const button=(text,root=document)=>[...root.querySelectorAll('button')].find(n=>n.textContent.trim()===text);
 const set=(n,v)=>{assert(n,'input');n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}))};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
 await nav('LoRA');await wait(()=>document.querySelectorAll('[data-lora-item]').length>0,'inventory');
 if(report.phase==='initial'){
 assert(document.querySelectorAll('[data-lora-item]').length===6,'six fixture models (isolated folder only)');[...document.querySelectorAll('.lora-tree .folder')].find(b=>b.textContent.trim().startsWith('Portraits')).click();await wait(()=>document.querySelectorAll('[data-lora-item]').length===3,'category');
 const item=document.querySelector('[data-lora-item]');button('☆',item).click();await wait(()=>button('★',item),'favorite shown');button('Ajouter',item).click();await pause(100);
 set(document.querySelector('.lora-sel input[type=number]'),'0.65');set(document.querySelectorAll('.lora-sel input[type=number]')[1],'0.35');set(document.querySelector('[placeholder="Nom du stack"]'),'Reprise fixture');
 await pause(100);button('Enregistrer',document.querySelector('.lora-stack')).click();await wait(async()=>(await api('preferences')).loraStacks?.length===1,'stack saved');
 button('Envoyer à Créer').click();await wait(async()=>(await api('preferences')).loraActiveStack?.[0]?.modelWeight===0.65,'active stack');
 await nav('Galerie');await nav('LoRA');await wait(()=>document.querySelector('.lora-sel'),'navigation stack');
 }
 await wait(()=>document.querySelector('.lora-sel'),'persisted stack');assert(document.querySelector('.lora-sel input[type=number]').value==='0.65','MODEL persistence');assert(document.querySelectorAll('.lora-sel input[type=number]')[1].value==='0.35','CLIP persistence');
 assert(document.querySelectorAll('[data-lora-item]').length===3,'category persistence');assert((await api('preferences')).loraFavorites.length===1,'favorite persistence');assert((await api('model_library',{kind:'loras'})).items.filter(m=>m.favorite).length===1,'favorite written to the LoraManager sidecar');
 const search=document.querySelector('.lora-top input[aria-label="Rechercher un modèle"]');set(search,'-1');await wait(()=>document.querySelectorAll('[data-lora-item]').length===1,'search');set(search,'');await wait(()=>document.querySelectorAll('[data-lora-item]').length===3,'search cleared');
 await wait(()=>[...document.querySelectorAll('.lora-item img')].every(n=>n.complete&&n.naturalWidth>0),'preview decoded');await pause(1000);
 report.capture=await invoke('smoke_capture',{name:'lora-'+report.phase});
 report.checks.push('folder tree and search','sidecar favorite','favorites','stack MODEL/CLIP and generation handoff','navigation and restart persistence','local previews');report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message||e)+' | '+String(e?.stack||'');report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
