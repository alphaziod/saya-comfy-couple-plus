(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 // V1 native smoke: character targeting: "Cibler des personnages" button of the
 // Casting menu → LARGE window (visual grid of the validated ones from the engine registry, portraits
 // of the app sheets joined by exact name, search by name, multi-select) → confirm
 // → back in casting (targeting chip, N = selection) → engine (SAYA_ENGINE_DIR stub)
 // receives --casting 2 --persos CH--0101,CH--0103 → 2 pass-1-ONLY runs → YES persisted.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const tile=id=>document.querySelector(`[data-cibler-perso="${id}"]`);
 try{
  await wait(()=>document.querySelector('[data-nav]'),'nav');
  await nav('Génération');
  await wait(()=>document.querySelector('[data-saya-source]'),'source');
  // The config is set in Generation, casting lives in its own menu (V0.1).
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);
  document.querySelector('[data-workflow-open]').click();
  await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
  await wait(()=>document.querySelector('[data-nav]'),'casting nav');
  await nav('Casting');
  await wait(()=>document.querySelector('[data-casting-cibler]'),'casting menu with cibler button');
  if(report.phase==='initial'){
   // 0. Sidecar fixture index (130 images + Alpha/Beta/Gamma Test sheets linked by
   // journal.jsonl): synchronous public command — no dependency on gallery timing.
   const prefs=await api('preferences');
   assert(prefs.libraryRoot,'fixture library root known');
   const indexation=await api('index',{root:prefs.libraryRoot});
   assert((indexation.characters??0)>=3,'sidecar characters imported, got '+(indexation.characters??0));
   // 1. The window opens: the grid lists the engine REGISTRY (3 validated from the
   // fake registry; in production, the 96 of negatifs_figes.json) joined to the sheets.
   document.querySelector('[data-casting-cibler]').click();
   await wait(()=>document.querySelector('[data-cibler-fenetre]'),'cibler window opens');
   await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===3,'three registry tiles, got '+document.querySelectorAll('[data-cibler-perso]').length);
   await wait(()=>tile('CH--9001')?.querySelector('img'),'portrait of Alpha Test joins by exact name');
   await wait(()=>tile('CH--9002')?.querySelector('img'),'portrait of Beta Test joins by exact name');
   await wait(()=>tile('CH--9003')?.querySelector('img'),'portrait of Gamma Test joins by exact name');
   report.checks.push('fenêtre ouverte : registre moteur listé, fiches jointes (portraits rendus)');
   // 2. Search by name: "Beta" filters the grid down to a single tile.
   set('[data-cibler-recherche]','Beta');await pause(150);
   await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===1,'search filters to one tile');
   assert(!!tile('CH--9002'),'the remaining tile is Beta Test');
   set('[data-cibler-recherche]','zombie-rien');await pause(150);
   await wait(()=>!!document.querySelector('[data-cibler-grille] .empty-prompts'),'empty search shows the empty line');
   set('[data-cibler-recherche]','');await pause(150);
   await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===3,'clearing search restores the full grid');
   report.checks.push('recherche par nom : filtre, cas vide, retour à la grille complète');
   // 3. Multi-select in the intended order: CH--9003 first, then CH--9001.
   tile('CH--9003').click();await pause(100);
   tile('CH--9001').click();
   await wait(()=>tile('CH--9001')?.getAttribute('data-selected')==='oui','first tile selected');
   await wait(()=>tile('CH--9003')?.getAttribute('data-selected')==='oui','second tile selected');
   await wait(()=>tile('CH--9002')?.getAttribute('data-selected')==='non','unselected tile stays off');
   await wait(()=>document.querySelector('[data-cibler-compteur]')?.textContent.includes('2 sélectionnés'),'counter shows 2 sélectionnés');
   const compteur=document.querySelector('[data-cibler-compteur]');
   assert(compteur.textContent.includes('CH--9001')&&compteur.textContent.includes('CH--9003'),'counter lists both ids');
   // 4. Confirm: the window closes, the targeting falls back into casting.
   document.querySelector('[data-cibler-valider]').click();
   await wait(()=>!document.querySelector('[data-cibler-fenetre]'),'window closes on validate');
   await wait(()=>document.querySelector('[data-casting-ciblage]'),'ciblage chip lands in the casting menu');
   assert(document.querySelector('[data-casting-ciblage]').textContent.includes('2'),'chip carries the count');
   assert(document.querySelector('[data-casting-n]').disabled===true,'N input is locked while ciblage is active');
   assert(document.querySelector('[data-casting-n]').value==='2','N mirrors the selection size');
   report.checks.push('multi-sélection puis validation : fenêtre fermée, chip « 2 persos ciblés », N verrouillé à 2');
   // 5. Launch: the (stub) engine receives EXACTLY the selection, in order.
   document.querySelector('[data-casting-lancer]').click();
   await wait(()=>document.querySelectorAll('[data-casting-shot]').length===2,'two targeted shots');
   await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'targeted phase 1 succeeded per perso');
   const cartes=[...document.querySelectorAll('[data-casting-shot]')];
   // Grid in "most recent first" order (createdAt chrono): CH--9001 (launched
   // last) at the top. The SELECTION ORDER is proven on the manifests:
   // isolatedRoot casting-tests/<batch>/1 = first requested id (CH--9003).
   assert(cartes[0].querySelector('h2').textContent==='Stub Ciblé CH--9001','first card is the newest essai CH--9001, got '+cartes[0].querySelector('h2').textContent);
   assert(cartes[1].querySelector('h2').textContent==='Stub Ciblé CH--9003','second card is CH--9003');
   const runs=await api('saya_runs');
   const cibleRuns=runs.filter(r=>r.isolatedRoot&&String(r.isolatedRoot).startsWith('casting-tests/'));
   assert(cibleRuns.length===2,'two targeted run manifests, got '+cibleRuns.length);
   const parIndex=Object.fromEntries(cibleRuns.map(r=>[String(r.isolatedRoot).split('/').pop(),r.casting?.shot?.stem]));
   assert(parIndex['1']==='hum--9003-stub-cible-noobai','selection order kept: index 1 = CH--9003, got '+parIndex['1']);
   assert(parIndex['2']==='hum--9001-stub-cible-noobai','selection order kept: index 2 = CH--9001, got '+parIndex['2']);
   for(const r of cibleRuns){
     assert(Object.keys(r.phases??{}).length===1&&r.phases['1'],'run '+r.id.slice(0,8)+' : phase 1 seule');
     assert(!r.phases['2']&&!r.phases['6'],'run '+r.id.slice(0,8)+' : jamais les 6 passes');
     assert(r.autoAdvance!==true,'run '+r.id.slice(0,8)+' : aucune avance automatique');
     assert(r.phases['1'].state==='succeeded'&&r.phases['1'].asset,'run '+r.id.slice(0,8)+' : image de passe 1');
   }
   report.checks.push('retombée moteur : --casting 2 --persos CH--9001,CH--9003 → 2 runs passe 1 seule, ordre conservé');
   // 6. Pass 3: no more YES button — click on the image → detail → "Lancer le
   //    workflow complet" (casting_decide keep:true, 6 passes). Stem = the targeted validated one.
   cartes[0].querySelector('[data-casting-preview]').click();
   await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'detail actions');
   document.querySelector('[data-casting-complet]').click();
   await wait(()=>cartes[0].getAttribute('data-decision')==='oui','workflow complet lancé');
   await wait(()=>{const p=cartes[0].querySelector('[data-casting-progress]');return p&&p.textContent.includes('Workflow complet terminé')},'6 passes terminées sur la validée ciblée');
   const choix=await api('casting_choices');
   assert(choix.length===1,'exactly one persisted OUI, got '+choix.length);
   assert(choix[0].stem==='hum--9001-stub-cible-noobai','persisted stem is the completed essai: '+choix[0].stem);
   assert(choix[0].name==='Stub Ciblé CH--9001','persisted row carries the targeted name');
   report.expected={graine:choix[0].graine,stem:choix[0].stem,runs:cibleRuns.map(r=>r.id)};
   report.capture=await invoke('smoke_capture',{name:'casting-v1-persos'});
  }else{
   // Process restart: the targeting's YES persists in the database.
   const choix=await api('casting_choices');
   assert(choix.length===1&&choix[0].stem===window.__V12_SMOKE_PREVIOUS__.expected.stem,'targeted OUI survives restart');
   report.checks.push('persisted targeted OUI survives app restart');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
