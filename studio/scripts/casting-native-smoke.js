(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 // U1 native smoke: 1-to-1 casting: Casting panel → engine (SAYA_ENGINE_DIR stub) →
 // N pass-1-ONLY runs → one image per OC → click on the image → "Lancer le workflow
 // complet" (casting_decide keep:true, 6 passes on THIS result). Pass 3: the YES/NO
 // sort is gone from the interface — the casting_choices data stays in the backend.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 // New flow: click image → detail window → "Lancer le workflow complet".
 const completSur=async carte=>{
  carte.querySelector('[data-casting-preview]').click();
  await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'détail ouvert avec les deux actions');
  document.querySelector('[data-casting-complet]').click();
  await wait(()=>!document.querySelector('[data-casting-preview-dialog]'),'détail fermé après le lancement');
 };
 try{
  await wait(()=>document.querySelector('[data-nav]'),'nav');
  await nav('Génération');
  await wait(()=>document.querySelector('[data-saya-source]'),'source');
  // V0.1: the config is set in Generation, casting lives in its own menu.
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);
  document.querySelector('[data-workflow-open]').click();
  await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
  await wait(()=>document.querySelector('[data-nav]'),'casting nav');
  await nav('Casting');
  await wait(()=>document.querySelector('[data-casting-lancer]'),'casting menu');
  if(report.phase==='initial'){
   set('[data-casting-n]','2');
   set('[data-casting-famille]','bny');
   await pause(100);
   document.querySelector('[data-casting-lancer]').click();
   // The (stub) engine answers fast; the two pass-1 runs sync from the database.
   await wait(()=>document.querySelectorAll('[data-casting-shot]').length===2,'two casting shots');
   await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'phase 1 succeeded per OC');
   // PASS 1 ONLY: each manifest has a single phase, no auto-advance (before a decision).
   const rows=[...document.querySelectorAll('[data-casting-shot]')];
   const runs=await api('saya_runs');
   const castingRuns=runs.filter(r=>r.isolatedRoot&&String(r.isolatedRoot).startsWith('casting-tests/'));
   assert(castingRuns.length===2,'two casting run manifests, got '+castingRuns.length);
   for(const r of castingRuns){
     assert(Object.keys(r.phases??{}).length===1&&r.phases['1'],'run '+r.id.slice(0,8)+' : phase 1 seule');
     assert(!r.phases['2']&&!r.phases['6'],'run '+r.id.slice(0,8)+' : jamais les 6 passes');
     assert(r.autoAdvance!==true,'run '+r.id.slice(0,8)+' : aucune avance automatique');
     assert(r.phases['1'].state==='succeeded','run '+r.id.slice(0,8)+' : passe 1 réussie');
     assert(r.phases['1'].asset,'run '+r.id.slice(0,8)+' : image de passe 1');
   }
   report.checks.push('casting panel → engine → 2 runs, phase 1 only (no phase 2..6, no autoAdvance), images shown');
   // No more YES/NO buttons in the grid: both trials stay undecided.
   assert(!rows[0].querySelector('[data-casting-oui]')&&!rows[0].querySelector('[data-casting-non]'),'les boutons OUI/NON ont disparu de la carte');
   assert(rows.every(c=>c.getAttribute('data-decision')===''),'aucune décision tant qu\'aucune action du détail');
   // Pass 3: click on the image → two separate actions → full workflow on trial 1.
   rows[0].querySelector('[data-casting-preview]').click();
   await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-enregistrer]'),'« Enregistrer ce personnage » présent dans le détail');
   await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'« Lancer le workflow complet » présent dans le détail');
   document.querySelector('[data-casting-preview-dialog] [data-casting-complet]').click();
   await wait(()=>rows[0].getAttribute('data-decision')==='oui','workflow complet lancé (decision oui)');
   await wait(()=>{const p=rows[0].querySelector('[data-casting-progress]');return p&&p.textContent.includes('Workflow complet terminé')},'les 6 passes se sont enchaînées');
   const historique=await api('casting_history',{sync:false});
   const fini=historique.find(r=>r.decision==='oui');
   assert(fini&&fini.runState==='completed'&&fini.phases?.['6']?.state==='succeeded'&&fini.finalAsset,'finition 6 passes terminée avec image finale');
   const choix=await api('casting_choices');
   assert(choix.length===1,'exactly one persisted OUI, got '+choix.length);
   assert(String(choix[0].stem).startsWith('bny-'),'persisted stem is a bny OC');
   assert(choix[0].famille==='bny'&&choix[0].graine&&choix[0].p1,'persisted row carries famille/graine/p1');
   // The second trial stays undecided: no action was requested for it.
   assert(rows[1].getAttribute('data-decision')==='','second essai untouched');
   report.checks.push('détail image : deux actions séparées ; workflow complet → 6 passes terminées, OUI persisté (Database) ; essai 2 intact');
   report.expected={graine:choix[0].graine,stem:choix[0].stem,runs:castingRuns.map(r=>r.id)};
   report.capture=await invoke('smoke_capture',{name:'casting-u1'});
  }else{
   // Process restart: the YES answers persist in the database, the list comes back complete.
   const choix=await api('casting_choices');
   assert(choix.length===1&&choix[0].stem===window.__V12_SMOKE_PREVIOUS__.expected.stem,'OUI survives restart');
   const historique=await api('casting_history',{sync:false});
   const fini=historique.find(r=>r.decision==='oui');
   assert(fini&&fini.runState==='completed'&&fini.phases?.['6']?.state==='succeeded','finition terminée persiste au redémarrage');
   report.checks.push('persisted OUI + completed finition survive app restart');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
