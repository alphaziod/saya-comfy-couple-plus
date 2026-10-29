(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 const wait=async fn=>{const end=Date.now()+20000;while(!fn()){if(Date.now()>end)throw new Error('Condition not reached '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const label=text=>[...document.querySelectorAll('label')].find(l=>l.textContent.trim().startsWith(text));
 const select=async(text,value)=>{const el=label(text).querySelector('select');el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await new Promise(r=>setTimeout(r,0));};
 const count=()=>document.querySelectorAll('.image-card').length;
 const ready=()=>document.querySelector('.image-grid')?.getAttribute('aria-busy')==='false';
 try{
  const prior=(await api('preferences')).galleryFilters;await wait(()=>button('Réinitialiser les filtres')&&ready()&&!button('Indexer / actualiser')?.disabled);await nav('Personnages');await wait(()=>document.querySelector('[data-nav="characters"][aria-current="page"]'));const actionsBefore=await invoke('smoke_actions');await nav('Galerie');await wait(()=>button('Indexer / actualiser'));if(!button('Indexer / actualiser').disabled)throw new Error('Index enabled before preferences');button('Indexer / actualiser').click();if(JSON.stringify(await invoke('smoke_actions'))!==JSON.stringify(actionsBefore.concat('set_preference')))throw new Error('Unexpected initialization write');report.checks.push({initializationBlocksIndex:true});await wait(()=>button('Réinitialiser les filtres')&&ready()&&!button('Indexer / actualiser')?.disabled);if(!document.querySelector('.root-field input').value.endsWith('/test-results/lora-native-v2/images'))throw new Error('Unexpected index root');
  if(prior?.loraExact&&prior?.loras?.length===2){await wait(()=>count()===1);if(!document.querySelector('[data-lora-exact]').checked)throw new Error('Exact mode not restored');report.checks.push({restartPersistence:true});}
  button('Réinitialiser les filtres').click();await wait(()=>ready());button('Indexer / actualiser').click();await wait(()=>count()===5&&ready());
  await select('Ajouter une LoRA (ET)','A');await wait(()=>count()===3&&ready());await select('Ajouter une LoRA (ET)','B');await wait(()=>count()===2&&ready());
  document.querySelector('[data-lora-exact]').click();await wait(()=>count()===1&&ready());if(!document.querySelector('.image-card').textContent.includes('ab.png'))throw new Error('Wrong exact set');
  await select('Checkpoint','model');await wait(()=>count()===1&&ready());
  button('A ×').click();await wait(()=>count()===0&&ready());button('B ×').click();await wait(()=>count()===1&&ready());await select('Checkpoint','');await wait(()=>count()===2&&ready());
  if(!document.body.textContent.includes('ne prouvent pas'))throw new Error('Unknown metadata caveat absent');
  await select('Ajouter une LoRA (ET)','A');await wait(()=>count()===1&&ready());await select('Ajouter une LoRA (ET)','B');await wait(()=>count()===1&&ready());
  const prefs=(await api('preferences')).galleryFilters;if(!prefs.loraExact||prefs.loras.join(',')!=='A,B')throw new Error('Preferences missing');
  const all=await api('gallery',{filters:{}});if(all.total!==5)throw new Error('Filter mutated library');
  report.checks.push({inclusiveVersusExact:true,combinedCheckpoint:true,emptyIndexedGroup:true,missingMetadataExplicit:true,persisted:true,viewport:[innerWidth,innerHeight]});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);report.uiErrors=[...document.querySelectorAll('.error')].map(e=>e.textContent);}
 await invoke('finish_smoke',{report});
})();
