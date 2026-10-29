(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,pause=ms=>new Promise(r=>setTimeout(r,ms));
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+15000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(100)}};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
 const pages={casting:'Casting',scenes:'Scènes',assistant:'Prompts',workflows:'Workflows',people:'Galerie personnages',ablab:'Labo A/B',modules:'Modules',pipelines:'Pipelines',graph:'Éditeur graphique',engine:'ComfyUI',data:'Données'};
 await wait(()=>document.querySelectorAll('[data-nav]').length>=9,'navigation ready');
 assert(![...document.querySelectorAll('[data-nav]')].some(b=>b.textContent.trim()==='Recettes'),'removed recipes menu');
 assert(![...document.querySelectorAll('[data-nav]')].some(b=>b.textContent.trim()==='Générations'),'removed generations browser menu');
 assert(![...document.querySelectorAll('[data-nav]')].some(b=>b.textContent.trim()==='Arbre'),'removed generation tree menu');
 assert(!document.body.textContent.includes('Arbre des générations'),'generation tree page must be gone from navigation');
 for(const [id,label] of Object.entries(pages)){
   await nav(label);const button=await(async()=>{await wait(()=>document.querySelector(`[data-nav="${id}"][aria-current="page"],.subtabs button.on[data-nav="${id}"],[data-nav="${id}"].active`),'active menu '+label);return document.querySelector(`[data-nav="${id}"][aria-current="page"],.subtabs button.on[data-nav="${id}"],[data-nav="${id}"].active`)})();
   await wait(()=>document.querySelector(`[data-page-guide="${id}"]`),'page '+label);await pause(250);
   const guide=document.querySelector(`[data-page-guide="${id}"]`);assert(guide.querySelector('strong')?.textContent&&guide.querySelector('p')?.textContent.length>40,'empty help '+label);
   assert(button.getAttribute('aria-current')==='page'||button.getAttribute('aria-selected')==='true'||button.classList.contains('active'),'active menu '+label);
   if(id==='ablab')assert(!document.querySelector('.page-heading').textContent.includes('Aucune soumission'),'stale AB claim');
   if(id==='pipelines')assert(![...document.querySelectorAll('.error')].some(n=>n.textContent.includes('Le nom du pipeline est vide')),'empty draft must not show runtime error');
   if(id==='workflows')assert(document.querySelector('[data-workflow-library]')||document.body.textContent.includes('Aucun workflow importé'),'workflow library or its empty state missing');
   report.checks.push(label+' : menu actif, aide et page visibles');
 }
 assert(!report.errors.length,'browser errors');report.ok=true;
 }catch(e){report.failure=String(e.message||e)}await invoke('finish_smoke',{report});
})();
