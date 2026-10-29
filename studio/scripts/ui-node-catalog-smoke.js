(async()=>{const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;const invoke=window.__TAURI_INTERNALS__.invoke,assert=(v,m)=>{if(!v)throw Error(m)};const wait=async(p,m,t=30000)=>{const e=Date.now()+t;while(!await p()){if(Date.now()>e)throw Error(m);await new Promise(r=>setTimeout(r,200))}};window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));try{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 await wait(()=>document.querySelector('[data-nav]'),'nav');await nav('Éditeur graphique');
 await wait(()=>document.querySelector('[data-graph-new]'),'graph new');document.querySelector('[data-graph-new]').click();
 await wait(()=>document.querySelector('[data-graph-catalog-open]'),'catalog button');document.querySelector('[data-graph-catalog-open]').click();
 await wait(()=>document.querySelector('[data-node-catalog]'),'catalog');report.checks.push('catalog open');
 const search=document.querySelector('[data-node-search]');search.value='KSampler';search.dispatchEvent(new Event('input',{bubbles:true}));
 await wait(()=>document.querySelector('.node-catalog-item'),'results');report.checks.push('search');
 document.querySelector('.node-catalog-item').click();
 await wait(()=>document.querySelectorAll('.graph-node').length>=1,'node added');report.checks.push('node added');
 assert(document.querySelector('[data-graph-new]'),'new workflow control');report.ok=true;
 }catch(e){report.failure=String(e?.stack||e)}window.__SMOKE_STAGE=report;if(typeof invoke==='function')await invoke('finish_smoke',{report}).catch(()=>{});})();
