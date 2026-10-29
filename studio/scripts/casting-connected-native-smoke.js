(async()=>{
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(f,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+100000;while(!await f()){if(Date.now()>end)throw Error(m+': '+[...document.querySelectorAll('.error')].map(n=>n.textContent));await pause(150)}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
 if(report.phase==='initial'){
  await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'opened');
  await nav('Casting');await wait(()=>!document.querySelector('[data-casting-lancer]').disabled,'linked config');
  set('[data-casting-n]','2');set('[data-casting-famille]','bny');await press('[data-casting-lancer]');await pause(250);const readStart=Date.now();await api('casting_history',{sync:false});assert(Date.now()-readStart<1400,'RNG blocked history');
  await wait(()=>document.querySelectorAll('[data-casting-shot][data-state="succeeded"]').length===2,'two trial images');
  const history=await api('casting_history',{sync:false});assert(history.length===2,'history lost');
  assert(history.every(r=>r.catalogue&&!r.catalogue.includes('/engine-stub/')),'mutable engine catalogue referenced');
  const first=document.querySelector('[data-casting-shot]');const stem=first.dataset.castingShot;
  // Pass 3: no more YES/NO buttons — click on the image → detail → "Lancer le
  // workflow complet" (casting_decide keep:true). NO remains possible in the backend
  // (data kept) but the interface no longer offers it: direct API call.
  first.querySelector('[data-casting-preview]').click();
  await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'detail actions');
  document.querySelector('[data-casting-complet]').click();
  await wait(()=>document.querySelector(`[data-casting-shot="${stem}"]`)?.dataset.decision==='oui','yes saved');
  // Grid in "most recent first" order: the undecided NO trial is the one whose stem
  // differs from the completed card (the naive find fell back on the same trial).
  const stemComplete=first.dataset.castingShot;
  const undecided=history.find(r=>r.decision===''&&r.stem!==stemComplete);
  assert(undecided,'un second essai sans décision disponible');
  await api('casting_decide',{run:undecided.run,keep:false});
  await wait(async()=>{const rows=await api('casting_history',{sync:false});return rows.some(r=>r.decision==='oui'&&r.runState==='completed')&&rows.some(r=>r.decision==='non')},'yes completes six passes and no stays rejected');
  const cartesDec=[...document.querySelectorAll('[data-casting-shot]')].map(c=>c.getAttribute('data-decision'));
  assert(cartesDec.filter(d=>d==='oui').length===1&&cartesDec.filter(d=>d==='non').length===1,'decision states shown on cards without keep/discard tabs');
  assert(!document.querySelector('[data-casting-gardes]')&&!document.querySelector('[data-casting-non]'),'kept/discard tabs and buttons removed from UI');
  await press('[data-casting-preview]');await wait(()=>document.querySelector('[data-casting-preview-dialog] img'),'large final preview');document.querySelector('[data-casting-preview-fermer]').click();
  const kept=(await api('casting_history',{sync:false})).find(r=>r.decision==='oui');const jobs=(await api('jobs')).length;
  await api('casting_decide',{run:kept.run,keep:true});assert((await api('jobs')).length===jobs,'repeated yes submitted twice');
  assert(kept.phases['6'].state==='succeeded'&&kept.finalAsset,'final result missing');
  report.frozen=history.map(r=>({catalogue:r.catalogue,shot:r.shot,run:r.run}));
  set('[data-casting-n]','1');await press('[data-casting-lancer]');await wait(()=>document.querySelectorAll('[data-casting-shot]').length===3,'second batch');
  const after=await api('casting_history',{sync:false});for(const old of history){assert(JSON.stringify(after.find(r=>r.run===old.run).shot)===JSON.stringify(old.shot),'old shot mutated by next batch')}
  await nav('Personnages');await nav('Casting');await wait(()=>document.querySelectorAll('[data-casting-shot]').length===3,'history after navigation');
  report.capture=await invoke('smoke_capture',{name:'casting-connected'});
  report.checks.push('Generation config reused; stable catalogue per batch; history restored on navigation','detail-image actions: workflow complet starts passes 2–6 once; NON via API persists (command kept, UI removed); final preview');
 }else{
  await nav('Casting');await wait(()=>document.querySelectorAll('[data-casting-shot]').length===3,'history after restart');
  const dec=[...document.querySelectorAll('[data-casting-shot]')].map(c=>c.getAttribute('data-decision'));
  assert(dec.filter(d=>d==='oui').length===1,'yes lost');assert(dec.filter(d=>d==='non').length===1,'no lost');
  assert(document.querySelector('[data-casting-n]').value==='1','casting inputs lost');report.checks.push('runs, decisions and casting inputs restored after native restart');
 }
 assert(![...document.querySelectorAll('[data-nav]')].some(b=>b.textContent.trim()==='Générations'),'Générations tab removed');report.checks.push('Générations tab removed (generations browser UI retired 2026-10-28)');assert(!report.errors.length,'unhandled errors');report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.rawError=typeof e==='object'?JSON.stringify(e):String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent);try{report.capture=await invoke('smoke_capture',{name:'casting-connected-failure'})}catch{}}
 await invoke('finish_smoke',{report});
})();
