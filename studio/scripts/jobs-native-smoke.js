(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const report={ok:false,checks:[],errors:[]};window.addEventListener('error',e=>report.errors.push(e.message));
 const wait=async(fn)=>{const end=Date.now()+12000;while(!fn()){if(Date.now()>end)throw new Error('Native condition timeout');await new Promise(r=>setTimeout(r,100));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 try {
  await nav('Workflows');await wait(()=>button('Retirer ce job de la file'));button('Retirer ce job de la file').click();await wait(()=>button('Confirmer l’action ciblée'));
  if(!button('Confirmer l’action ciblée').disabled)throw new Error('Control enabled without confirmation');
  const label=[...document.querySelectorAll('label')].find(x=>x.textContent.includes('Je confirme cette action'));label.querySelector('input').click();await wait(()=>!button('Confirmer l’action ciblée').disabled);
  button('Fermer').click();await wait(()=>!button('Confirmer l’action ciblée'));
  let refused=false;try{await api('control_job',{id:'native-job',operation:'cancel_queue',confirmed:false});}catch{refused=true;}if(!refused)throw new Error('IPC accepted unconfirmed action');
  button('Requête et journal').click();await wait(()=>document.querySelector('pre')?.textContent.includes('18446744073709551615'));
  report.checks.push({confirmationUi:true,ipcUnconfirmedRefused:true,exactSeedDisplayed:true,submitted:false,controlSent:false});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
