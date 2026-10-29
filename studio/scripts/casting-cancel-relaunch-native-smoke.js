(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+45000;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,100))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 try{
 await wait(()=>document.querySelector('nav'),'navigation');await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
 await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');await nav('Casting');await wait(()=>document.querySelector('[data-casting-source]')?.textContent.includes('Config consommée'),'casting source');
 set('[data-casting-n]','2');
 for(let i=0;i<2;i++){
  await press('[data-casting-lancer]');await wait(()=>document.querySelector('[data-casting-runbar]'),'active batch');
  await press('[data-casting-stop]');await wait(()=>!document.querySelector('[data-casting-runbar]')&&!document.querySelector('[data-casting-lancer]').disabled,'cancel releases launch');
  const rows=await api('casting_history',{sync:false});if(rows.some(r=>r.runState==='running'&&r.state==='waiting'))throw Error('queued local trial survived stop');
 }
 report.checks.push('launch, cancel, relaunch, cancel: no active controls or blocked launch; local waiting trials stopped');report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
