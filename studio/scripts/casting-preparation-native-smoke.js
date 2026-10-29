(async()=>{
const report={ok:false,checks:[]};window.__SMOKE_STAGE=report;
const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,80))}};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
try{
await wait(()=>document.querySelector('[data-langue-setup]'),'language');document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language saved');
await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');set('[data-saya-source]',window.__V15_UI_WORKFLOW__);document.querySelector('[data-workflow-open]').click();await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened');
await nav('Casting');await wait(()=>document.querySelector('[data-casting-lancer]')&&!document.querySelector('[data-casting-lancer]').disabled,'ready');set('[data-casting-n]','20');document.querySelector('[data-casting-lancer]').click();
await wait(()=>document.querySelector('[data-casting-preparation]'),'preparation visible');
await nav('Galerie');await nav('Casting');await wait(()=>document.querySelector('[data-casting-preparation]'),'preparation survives navigation');
if(!document.querySelector('[data-casting-lancer]').disabled)throw Error('second launch allowed');
let counts=[],stages=new Set();
await wait(async()=>{const p=await api('casting_preparation_status');if(p.active){stages.add(p.stage);if(p.stage==='prepare')counts.push(p.completed);}return !p.active},'preparation completed');
await wait(()=>!document.querySelector('[data-casting-preparation]'),'preparation closes');
await wait(async()=>(await api('casting_history',{sync:false})).length===20,'twenty prepared');
await wait(()=>document.querySelector('[data-casting-runbar] [data-run-progress]'),'detailed progress visible');
await wait(async()=>{const rows=await api('casting_history',{sync:false});return rows.some(r=>r.promptId&&['submitted','queued','running'].includes(r.state))},'active prompt joined without jobs pagination');
await wait(()=>{const sampling=document.querySelector('[data-casting-runbar] [data-run-progress] label:last-of-type span');return sampling&&/\d+ \/ \d+/.test(sampling.textContent)},'real sampler progress shown');
const stop=document.querySelector('[data-casting-stop]');if(!stop||stop.disabled)throw Error('cancel locked during generation');stop.click();
await wait(()=>!document.querySelector('[data-casting-runbar]'),'cancel returns controls');
if(!counts.length||counts.some((v,i)=>i&&v<counts[i-1]))throw Error('missing or non-monotonic preparation count');
await wait(()=>document.querySelector('[data-casting-lot]'),'lot visible');if(document.querySelector('[data-casting-lot]').textContent.includes('Invalid Date'))throw Error('invalid date');
report.checks.push('live preparation visible after navigation; second launch disabled; monotonic prepared count; all 20 persisted; no Invalid Date');report.stages=[...stages];report.counts=counts;report.ok=true;
}catch(e){report.failure=String(e);report.errors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
await invoke('finish_smoke',{report});})();
