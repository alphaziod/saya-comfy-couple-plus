(async()=>{
 // V4 native smoke: anti-oversight finalisation: COMPLETE FLOW in one window.
 // 1. Empty case "no config": Casting opened BEFORE Generation → welcome note,
 //    Launch disabled, empty grid, no crash.
 // 2. Empty case "no result": generations browser before any run →
 //    "Aucune image trouvée" (fake engine roots / empty ComfyUI copies /
 //    app results absent), API listing at 0.
 // 3. Clean error: family refused by the engine (stub only accepts bny) →
 //    the engine error bubbles up in role=alert, no shot created.
 // 4. Config in Generation → "Config consommée" in Casting.
 // 5. Character targeting (V1): 1 validated → Launch → pass 1 only → YES.
 // 6. Rotations Mode B (U2): 1 pose × 2 characters → pass 1 only → YES + NO.
 // 7. Empty case "grid filters with no result" + persisted YES (casting_choices).
 // 8. Browser: the casting pass-1 images land in the
 //    "Résultats de l'app" root, chronological order, no invented attribution.
 // 9. Restart: YES and the browser images survive the process.
 // SAYA_ENGINE_DIR stub engine (scripts/casting-stub-engine.py), fake ComfyUI
 // accepting /prompt: no GPU, never the installed app, never the real engine.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const texte=s=>{const n=document.querySelector(s);return n?n.textContent.trim():''};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const tile=id=>document.querySelector(`[data-cibler-perso="${id}"]`);
 try{
  if(report.phase==='initial'){
   // 1. EMPTY CASE "no config": the app seeds the base preset on first
   //    start (generationWorkspace prefilled) — we erase the preference to
   //    exercise the real "no config" path of the Casting menu.
   await api('set_preference',{key:'generationWorkspace',value:{}});
   await wait(()=>document.querySelector('[data-nav]'),'nav casting');
   await nav('Casting');
   await wait(()=>document.querySelector('[data-casting-panel]'),'casting menu');
   // The welcome note comes from the preferences (async): wait for it to resolve.
   await wait(()=>texte('[data-casting-source]').length>0,'note d\'accueil résolue');
   assert(document.querySelector('[data-casting-lancer]').disabled===true,'Lancer désactivé sans config');
   assert(texte('[data-casting-source]').includes("Règle ta config dans Generation"),'note d\'accueil sans config : '+texte('[data-casting-source]'));
   assert(texte('.casting-grid .empty-prompts').includes('Ce menu ne règle pas le workflow'),'grille vide explique le chemin Generation');
   assert(!document.querySelector('[data-casting-filtres]'),'aucune barre de filtres sans résultats');
   report.checks.push('cas vide « aucune config » : note d\'accueil, Lancer désactivé, grille vide, pas de filtres');
   // 2. EMPTY CASE "no result": browser before any run.
   const vide=await api('generation_browser');
   assert(vide.totalImages===0,'listing à 0 image, got '+vide.totalImages);
   // engine output, delivered images (capture root), ComfyUI copies, app buffer
   assert(vide.roots.length===4,'quatre racines listées même vides, got '+vide.roots.length);
   report.checks.push('cas vide « aucun résultat » : listing API à 0, 4 racines');
   // 4. Config in Generation (the settings live THERE, not in Casting).
   await nav('Génération');
   await wait(()=>document.querySelector('[data-saya-source]'),'source');
   set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);
   document.querySelector('[data-workflow-open]').click();
   await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
   // GenerationWorkspace writes the preference at the moment one LEAVES the page
   // (onDestroy): in real use Saya configures then goes to casting afterwards.
   // The smoke waits for the write to actually land before navigating (never an IPC race).
   await wait(async()=>{const p=await api('preferences');return !!p.generationWorkspace?.session},'préférence generationWorkspace écrite à la sortie de Generation');
   // 4bis. Casting consumes the config.
   await nav('Casting');
   await wait(()=>document.querySelector('[data-casting-lancer]'),'casting ready');
   await wait(()=>texte('[data-casting-source]').includes('Config consommée'),'config consommée : '+texte('[data-casting-source]'));
   assert(document.querySelector('[data-casting-lancer]').disabled===false,'Lancer actif avec la config de Generation');
   report.checks.push('config réglée dans Generation → « Config consommée » dans Casting, Lancer actif');
   // 3. CLEAN ERROR: family the engine refuses (stub = bny only).
   set('[data-casting-famille]','kmn');await pause(100);
   document.querySelector('[data-casting-lancer]').click();
   await wait(()=>!!document.querySelector('.error[role="alert"]'),'erreur moteur affichée');
   const err=texte('.error[role="alert"]');
   assert(err.includes('moteur')&&err.includes('famille de casting inconnue : kmn'),'erreur porte la cause moteur : '+err);
   assert(document.querySelectorAll('[data-casting-shot]').length===0,'aucun shot créé par un refus');
   // The error stays displayed until the next action (task() resets it) —
   // intended behaviour: Saya reads the cause, the next launch clears it.
   set('[data-casting-famille]','');await pause(100);
   assert(!!document.querySelector('.error[role="alert"]'),'l\'erreur reste lisible jusqu\'à l\'action suivante');
   report.checks.push('erreur propre : refus moteur (famille inconnue) en role=alert, zéro shot, erreur persistante jusqu\'à l\'action suivante');
   // 5. CHARACTER TARGETING (V1): one validated from the fixture registry → Launch → pass 1
   //    only → click on the image → full workflow (6 passes, pass 3).
   document.querySelector('[data-casting-cibler]').click();
   await wait(()=>document.querySelector('[data-cibler-fenetre]'),'fenêtre cibler');
   await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===2,'deux tuiles du registre fixture');
   assert(!!tile('CH--9201')&&!!tile('CH--9202'),'tuiles CH--9201 et CH--9202');
   tile('CH--9201').click();
   await wait(()=>tile('CH--9201')?.getAttribute('data-selected')==='oui','tuile ciblée');
   document.querySelector('[data-cibler-valider]').click();
   await wait(()=>document.querySelector('[data-casting-ciblage]'),'chip ciblage');
   assert(document.querySelector('[data-casting-n]').value==='1','N verrouillé à la sélection');
   document.querySelector('[data-casting-lancer]').click();
   await wait(()=>document.querySelectorAll('[data-casting-shot]').length===1,'un shot ciblé');
   await wait(()=>document.querySelector('[data-casting-shot]')?.getAttribute('data-state')==='succeeded','passe 1 ciblée réussie');
   const cible=document.querySelector('[data-casting-shot]');
   assert(cible.getAttribute('data-casting-shot')==='hum--9201-stub-cible-noobai','stem = la validée ciblée');
   assert(!cible.querySelector('[data-casting-oui]')&&!cible.querySelector('[data-casting-non]'),'OUI/NON retirés de la carte');
   cible.querySelector('[data-casting-preview]').click();
   await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'détail : workflow complet proposé');
   document.querySelector('[data-casting-complet]').click();
   await wait(()=>cible.getAttribute('data-decision')==='oui','workflow complet lancé sur le ciblage');
   await wait(()=>{const p=cible.querySelector('[data-casting-progress]');return p&&p.textContent.includes('Workflow complet terminé')},'les 6 passes du ciblage se sont enchaînées');
   const choix1=await api('casting_choices');
   assert(choix1.length===1,'un OUI persisté, got '+choix1.length);
   report.checks.push('ciblage V1 : CH--9201 seul au casting, passe 1 réussie, workflow complet 6 passes, OUI persisté');
   // 6. ROTATIONS MODE B (U2): 1 pose × 2 characters — NO sort requested: the trials
   //    stay in the grid undecided (keep/discard sorting will live in the gallery).
   document.querySelector('[data-casting-ciblage-retirer]').click();
   await wait(()=>!document.querySelector('[data-casting-ciblage]'),'ciblage retiré');
   document.querySelector('[data-casting-pose-choisir]').click();
   await wait(()=>document.querySelector('[data-action-item="x_pinces_cheveux"]'),'action tile in picker');
   document.querySelector('[data-action-item="x_pinces_cheveux"]').click();
   await wait(()=>document.querySelector('[data-casting-pose-fixe="x_pinces_cheveux"]'),'action chosen');
   set('[data-casting-pose-n]','2');await pause(100);
   document.querySelector('[data-casting-pose-lancer]').click();
   // The grid shows the selected lot: the new Mode B lot (2 bny trials).
   await wait(()=>{const c=[...document.querySelectorAll('[data-casting-shot]')];return c.length===2&&c.every(t=>t.getAttribute('data-casting-shot').startsWith('bny-'))},'deux rotations ajoutées (nouveau lot)');
   await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'passe 1 réussie partout');
   const cartes=()=>[...document.querySelectorAll('[data-casting-shot]')];
   const rot=cartes().filter(c=>c.getAttribute('data-casting-shot').startsWith('bny-'));
   assert(rot.length===2,'deux shots Mode B bny');
   assert(rot.every(c=>c.getAttribute('data-decision')===''),'rotations sans décision (plus de boutons OUI/NON)');
   const choix=await api('casting_choices');
   assert(choix.length===1,'toujours un seul OUI persisté, got '+choix.length);
   const stemsTries=choix.map(c=>c.stem).sort();
   assert(JSON.stringify(stemsTries)===JSON.stringify(['hum--9201-stub-cible-noobai']),'stem persisté exact : '+JSON.stringify(stemsTries));
   report.checks.push('rotations Mode B : 2 shots passe 1 sans décision, aucun tri imposé, 1 OUI au total');
   // Runs: the targeting finish has the 6 passes; rotations stay pass 1 ONLY.
   const runs=await api('saya_runs');
   const mesRuns=runs.filter(r=>String(r.isolatedRoot||'').startsWith('casting-tests/'));
   assert(mesRuns.length===3,'trois runs casting-tests, got '+mesRuns.length);
   for(const r of mesRuns){
     const bny=String(r.casting?.shot?.stem||'').startsWith('bny-');
     if(bny){
       assert(Object.keys(r.phases??{}).length===1&&r.phases['1'],'rotation '+r.id.slice(0,8)+' : phase 1 seule');
       assert(!r.phases['2']&&!r.phases['6'],'rotation '+r.id.slice(0,8)+' : jamais les 6 passes');
       assert(r.autoAdvance!==true,'rotation '+r.id.slice(0,8)+' : aucune avance automatique');
     }else{
       assert(r.phases['6']?.state==='succeeded','finition '+r.id.slice(0,8)+' : 6 passes réussies');
     }
   }
   // 7. EMPTY CASE "grid filters with no result": hum ∧ illustrious = nothing.
   set('[data-casting-filtre-famille]','hum');
   set('[data-casting-filtre-model]','illustrious');await pause(150);
   assert(!cartes().length,'aucune carte sous hum ∧ illustrious');
   assert(texte('.casting-grid .empty-prompts').includes('Aucun shot ne correspond à ces filtres'),'ligne vide de filtres affichée');
   assert(texte('[data-casting-filtre-note]').includes('0 affiché'),'note de filtre à 0');
   set('[data-casting-filtre-famille]','');set('[data-casting-filtre-model]','');await pause(150);
   await wait(()=>cartes().length===2,'filtres levés : les deux cartes du lot affiché');
   report.checks.push('cas vide filtres grille : hum ∧ illustrious → ligne dédiée, retour complet');
   // 8. BROWSER: the casting pass-1 images land in the app root.
   const listing=await api('generation_browser');
   assert(listing.totalImages>=3,'au moins trois images dans le navigateur, got '+listing.totalImages);
   const appRoot=listing.roots.find(r=>r.kind==='app');
   assert(appRoot&&appRoot.exists===true,'racine app existante');
   // Deliveries are organized by day and family: count every app folder.
   const imagesApp=listing.folders.filter(f=>f.root==='app').reduce((n,f)=>n+f.imageCount,0);
   assert(imagesApp>=3,'racine app porte les images de passe 1, got '+imagesApp);
   report.checks.push('listing API : images passe 1 du casting dans la racine app');
   report.expected={ouis:stemsTries,total:listing.totalImages,fini:'hum--9201-stub-cible-noobai'};
   report.capture=await invoke('smoke_capture',{name:'casting-parcours-v4'});
  }else{
   // 9. RESTART: the YES (full workflow) and the browser images survive.
   const choix=await api('casting_choices');
   assert(choix.length===1,'un OUI après redémarrage, got '+choix.length);
   const stems=choix.map(c=>c.stem).sort();
   assert(stems.includes('hum--9201-stub-cible-noobai'),'OUI persisté après redémarrage : '+JSON.stringify(stems));
   const historique=await api('casting_history',{sync:false});
   const fini=historique.find(r=>r.decision==='oui');
   assert(fini&&fini.runState==='completed'&&fini.phases?.['6']?.state==='succeeded','finition terminée persiste au redémarrage');
   const listing=await api('generation_browser');
   // Startup cleanup deletes the pass 2-5 previews (lineage rule); the final and the
   // two rotation pass-1 images stay.
   assert(listing.totalImages>=3,'images du navigateur persistées : '+listing.totalImages);
   report.checks.push('redémarrage : 1 OUI (finition 6 passes terminée) et les images du navigateur survivent au processus');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
