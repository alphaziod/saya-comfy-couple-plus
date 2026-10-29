(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 const wait=async fn=>{const end=Date.now()+20000;while(!fn()){if(Date.now()>end)throw new Error('Condition not reached '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const input=text=>[...document.querySelectorAll('label')].find(l=>l.textContent.trim().startsWith(text))?.querySelector('input');
 const set=(text,value)=>{const el=input(text);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));};
 const status=()=>document.querySelector('[data-backup-status]')?.textContent??'';
 try{
  await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled);
  const prefs=await api('preferences'),root=prefs.libraryRoot.replace(/\/images$/,'');if(!root.endsWith('/test-results/backups-native'))throw new Error('Unexpected fixture root');
  report.characterId=await api('save_character',{id:null,name:'Backup fixture '+Date.now(),identity:{identity_prompt:'(exact:1.2),  tags\nline'}});
  const before=JSON.stringify(await api('characters'));report.backupDirectory=root+'/backup-'+Date.now();report.restoredDirectory=report.backupDirectory+'-restored';
  await nav('Données');await wait(()=>input('Nouveau dossier de sauvegarde')?.value);set('Nouveau dossier de sauvegarde',report.backupDirectory);button('Créer la sauvegarde vérifiée').click();await wait(()=>status()==='Sauvegarde créée et vérifiée.');
  let manifest=JSON.parse(document.querySelector('[data-backup-report]').textContent);if(manifest.schemaVersion!==9||manifest.originalsIncluded!==false||manifest.sha256.length!==64)throw new Error('Invalid manifest');
  await wait(()=>!button('Vérifier la sauvegarde').disabled);button('Vérifier la sauvegarde').click();await wait(()=>status()==='Empreinte et intégrité de la sauvegarde vérifiées.');
  await wait(()=>!button('Créer la sauvegarde vérifiée').disabled);button('Créer la sauvegarde vérifiée').click();await wait(()=>document.querySelector('.error')&&!button('Créer la sauvegarde vérifiée').disabled);if(document.querySelector('[data-backup-report]'))throw new Error('Failure retained old success report');
  set('Nouveau dossier de restauration',report.restoredDirectory);await wait(()=>!button('Restaurer dans le nouveau dossier').disabled);button('Restaurer dans le nouveau dossier').click();await wait(()=>status().startsWith('Copie restaurée et vérifiée'));await wait(()=>!button('Restaurer dans le nouveau dossier').disabled);
  if(JSON.stringify(await api('characters'))!==before)throw new Error('Active database changed');
  const verify=await api('verify_backup',{directory:report.restoredDirectory});if(verify.sha256!==manifest.sha256)throw new Error('Restored bytes differ');
  button('Restaurer dans le nouveau dossier').click();await wait(()=>document.querySelector('.error')&&!button('Restaurer dans le nouveau dossier').disabled);
  report.checks.push({backupCreated:true,manifestVerified:true,restoreVerified:true,overwriteRefused:true,activeDatabaseUnchanged:true,originalsExcluded:true});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);report.uiErrors=[...document.querySelectorAll('.error')].map(e=>e.textContent);}
 await invoke('finish_smoke',{report});
})();
