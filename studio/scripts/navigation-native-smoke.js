(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,report={ok:false,checks:[],errors:[]};
 const wait=async fn=>{const end=Date.now()+10000;while(!fn()){if(Date.now()>end)throw new Error('Condition not reached '+fn);await new Promise(r=>setTimeout(r,25));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 window.addEventListener('error',e=>report.errors.push(e.message));
 try{
  await wait(()=>document.querySelector('[data-nav="workflows"]')&&button('Indexer / actualiser'));
  if(!button('Indexer / actualiser').disabled)throw new Error('Test missed pending initialization');
  await nav('Workflows');await wait(()=>document.querySelector('.subtabs button.on[data-nav="workflows"]'));
  await new Promise(r=>setTimeout(r,1400));
  if(!document.querySelector('.subtabs button.on[data-nav="workflows"]'))throw new Error('Late preferences overwrote user navigation');
  const prefs=await invoke('studio',{request:{action:'preferences'}});if(prefs.selectedTab!=='workflows')throw new Error('Selected tab not persisted');
  report.checks.push({latePreferenceIgnored:true,userNavigationPersisted:true});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
