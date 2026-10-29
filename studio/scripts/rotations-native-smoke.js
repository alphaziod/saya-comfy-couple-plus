(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 // U2 native smoke: casting rotations: Casting panel → engine (SAYA_ENGINE_DIR stub) →
 // Mode A "1 character × N times the same pose" and Mode B "1 pose × N random characters",
 // N pass-1-ONLY runs → image per shot → click on the image → "Lancer le workflow
 // complet" (casting_decide keep:true). Pass 3: no more YES/NO sort in the UI.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const completSur=async carte=>{
  carte.querySelector('[data-casting-preview]').click();
  await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'détail ouvert');
  document.querySelector('[data-casting-complet]').click();
  await wait(()=>!document.querySelector('[data-casting-preview-dialog]'),'détail fermé');
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
  await wait(()=>document.querySelector('[data-casting-rot-lancer]'),'rotations menu');
  if(report.phase==='initial'){
   // MODE A: 1 character × N times the same pose.
   // Rotations have no text field any more: pick the character in the picker.
   document.querySelector('[data-casting-rot-choisir]').click();
   await wait(()=>document.querySelector('[data-cibler-perso="CH--9201"]'),'perso tile in picker');
   document.querySelector('[data-cibler-perso="CH--9201"]').click();
   await wait(()=>document.querySelector('[data-casting-rot-perso="CH--9201"]'),'perso chosen');
   set('[data-casting-rot-n]','2');
   await pause(100);
   document.querySelector('[data-casting-rot-lancer]').click();
   await wait(()=>document.querySelectorAll('[data-casting-shot]').length===2,'two rotation shots');
   await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'mode A phase 1 succeeded');
   const rot=[...document.querySelectorAll('[data-casting-shot]')];
   assert(rot[0].querySelector('h2').textContent.startsWith('Stub Prêtresse'),'mode A cards show the perso');
   assert(rot[0].querySelector('h2').textContent!==rot[1].querySelector('h2').textContent,'the two rotations are distinguishable');
   // The card follows the job; the run manifest is synced just after: wait for it.
   let rotRuns=[];
   await wait(async()=>{rotRuns=(await api('saya_runs')).filter(r=>String(r.isolatedRoot||'').startsWith('casting-tests/'));return rotRuns.length===2&&rotRuns.every(r=>r.phases?.['1']?.state==='succeeded'&&r.phases['1'].asset)},'two mode A run manifests synced');
   for(const r of rotRuns){
     assert(Object.keys(r.phases??{}).length===1&&r.phases['1'],'run '+r.id.slice(0,8)+' : phase 1 seule');
     assert(!r.phases['2']&&!r.phases['6'],'run '+r.id.slice(0,8)+' : jamais les 6 passes');
     assert(r.autoAdvance!==true,'run '+r.id.slice(0,8)+' : aucune avance automatique');
     assert(r.phases['1'].state==='succeeded'&&r.phases['1'].asset,'run '+r.id.slice(0,8)+' : image de passe 1');
   }
   report.checks.push('mode A (1 perso × N même pose) : 2 runs passe 1 seule, même perso affiché, images montrées');
   // Pass 3: no more YES buttons — click on the image of the first rotation →
   // full workflow (6 passes on THIS result). The other rotation stays untouched.
   assert(!rot[0].querySelector('[data-casting-oui]')&&!rot[0].querySelector('[data-casting-non]'),'OUI/NON retirés des cartes');
   await completSur(rot[0]);
   await wait(()=>rot[0].getAttribute('data-decision')==='oui','mode A workflow complet lancé');
   await wait(()=>{const p=rot[0].querySelector('[data-casting-progress]');return p&&p.textContent.includes('Workflow complet terminé')},'mode A : 6 passes terminées');
   await wait(()=>rot[1].getAttribute('data-decision')===''&&rot[1].getAttribute('data-state')==='succeeded','la seconde rotation reste sans décision');
   const choix1=await api('casting_choices');
   assert(choix1.length===1,'one persisted OUI after mode A, got '+choix1.length);
   assert(String(choix1[0].stem).includes('-rot-'),'mode A OUI carries a rot stem: '+choix1[0].stem);
   // MODE B: 1 pose × N random characters — no sort to do, the trials stay
   // in the grid undecided (keep/discard sorting will live in the gallery).
   document.querySelector('[data-casting-pose-choisir]').click();
   await wait(()=>document.querySelector('[data-action-item="x_pinces_cheveux"]'),'action tile in picker');
   document.querySelector('[data-action-item="x_pinces_cheveux"]').click();
   await wait(()=>document.querySelector('[data-casting-pose-fixe="x_pinces_cheveux"]'),'action chosen');
   set('[data-casting-pose-n]','2');
   await pause(100);
   document.querySelector('[data-casting-pose-lancer]').click();
   // The grid shows the selected lot: the new mode B lot (2 trials) once launched.
   await wait(()=>{const c=[...document.querySelectorAll('[data-casting-shot]')];return c.length===2&&c.every(t=>t.querySelector('h2')?.textContent.startsWith('Stub OC'))},'mode B lot shown (2 shots)');
   await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].slice(0,2).every(c=>c.getAttribute('data-state')==='succeeded'),'mode B phase 1 succeeded');
   const pose=[...document.querySelectorAll('[data-casting-shot]')].slice(0,2);
   assert(pose.every(c=>c.querySelector('h2').textContent.startsWith('Stub OC')),'mode B cards show random OCs');
   assert(pose.every(c=>c.getAttribute('data-decision')===''),'mode B cards stay undecided');
   const choix=await api('casting_choices');
   assert(choix.length===1,'still exactly one persisted OUI, got '+choix.length);
   const lignePose=choix[0];
   assert(lignePose&&String(lignePose.stem).includes('-rot-'),'the mode A OUI is the only choice');
   report.checks.push('mode B (1 pose × N persos) : 2 essais passe 1 sans décision, aucun tri OUI/NON proposé');
   report.expected={rot:choix1[0].stem,runs:rotRuns.map(r=>r.id)};
   report.capture=await invoke('smoke_capture',{name:'rotations-u2'});
  }else{
   // Process restart: mode A's YES persists in the database, the grid is complete.
   const choix=await api('casting_choices');
   assert(choix.length===1&&choix[0].stem===window.__V12_SMOKE_PREVIOUS__.expected.rot,'mode A OUI survives restart, got '+choix.length);
   report.checks.push('persisted OUI from mode A survives app restart');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
