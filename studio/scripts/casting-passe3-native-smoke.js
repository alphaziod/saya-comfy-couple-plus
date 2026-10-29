(async()=>{
 // Pass 3 native smoke — new Casting flow (Saya's feedback 2026-10-26):
 //  1. EXCLUSION of actions ("Exclure du tirage" tab, same grid, the invalidated one
 //     hidden via disponible) → chip "N exclues" → casting_lancer payload with
 //     exclude[] (invoke spy — UI→contract proof) — backend semantics: free draw
 //     / fixed pose / targeted actions accept it, character targeting and character
 //     rotation refuse it.
 //  2. GLOBAL progress bar: trials done / total, current trial and pass,
 //     honest pause label; disappears when everything is done.
 //  3. Pause / Resume / Cancel (casting_pause / resume / stop contract, slice B) —
 //     only if the backend exposes them; otherwise report.attendu, NEVER a PASS.
 //  4. Click on an image: two separate actions — "Enregistrer ce personnage"
 //     (casting_enregistrer_perso, real sheet, NEVER launches passes 2-6) and
 //     "Lancer le workflow complet" (6 real passes on THIS result).
 //  5. Character targeting: persos[] payload WITHOUT exclude (mutual exclusion).
 //  6. Scripts (real modes = backend ENGINE_SCRIPTS: attente/poses/lcs/duo/bny20):
 //     wiring + choice persistence — no execution (real GPU script).
 //  7. Immediate persistence (set_preference castingWorkspace): readable DURING the
 //     page, after navigation, after process restart.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],attendu:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+100000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const texte=s=>{const n=document.querySelector(s);return n?n.textContent.trim():''};
 // Click on a button that had to be waited for until ACTIVE: removing the disabled attribute is a render,
 // a same-tick click would hit a still-inactive button (silent no-op).
 const press=async s=>{await wait(()=>{const b=document.querySelector(s);return b&&!b.disabled},s+' actif');document.querySelector(s).click()};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const tile=id=>document.querySelector(`[data-cibler-perso="${id}"]`);
 // Payload proof: the stub engine's argv journal (sortie/casting/
 // argv_journal.jsonl, written by the stub) shows WHAT THE BACKEND ACTUALLY PASSED
 // to the engine — end-to-end proof stronger than a JS spy: asserted in the .sh
 // after the run (POST-RUN OK). No invoke spy here: writing on
 // __TAURI_INTERNALS__.invoke may be silently ignored depending on the webview.
 const actif=etat=>['submitted','submitting','queued','running','submission_unknown'].includes(etat);
 try{
  // Backend probe: slice B commands. serde "unknown variant" = absent;
  // any other error (invalid run…) = present. NEVER probe engine_script:
  // it would launch a real GPU script.
  const backend={};
  if(report.phase==='initial'){
   for(const a of ['casting_stop','casting_pause','casting_resume','casting_enregistrer_perso']){
    try{await api(a,{run:'sonde-invalide',stem:'sonde',nom:'sonde'});backend[a]=true;}
    catch(e){backend[a]=!/unknown variant|variante inconnue|unsupported|non support/i.test(String(e?.message||e));}
   }
   report.backend={...backend};
  }
  if(report.phase==='initial'){
   // Configuration in Generation (the config is set THERE, casting consumes it).
   await wait(()=>document.querySelector('[data-nav]'),'nav');
   await nav('Génération');
   await wait(()=>document.querySelector('[data-saya-source]'),'source');
   set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);
   document.querySelector('[data-workflow-open]').click();
   await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
   await wait(async()=>{const p=await api('preferences');return !!p.generationWorkspace?.session},'préférence generationWorkspace écrite');
   await nav('Casting');
   await wait(()=>document.querySelector('[data-casting-lancer]'),'casting menu');
   await wait(()=>texte('[data-casting-source]').includes('Config consommée'),'config consommée');
   // The tabs and YES/NO buttons are gone from the interface.
   assert(!document.querySelector('[data-casting-gardes]')&&!document.querySelector('[data-casting-non]')&&!document.querySelector('[data-casting-oui]'),'tri OUI/NON retiré (onglets + boutons)');
   // 1. Actions window in TARGET mode: CONVERGED A/B rule (slice D) — the window
   //    shows EXACTLY the engine draw pool: x_vieux (valide:false) and x_duo
   //    (pas_solo) left out, x_pinces and x_bridon + their _flaccide sisters (4 tiles).
   await press('[data-casting-cibler-actions]');
   await wait(()=>document.querySelector('[data-actions-fenetre]'),'actions window open');
   await wait(()=>document.querySelectorAll('[data-action-item]').length===4,'quatre actions disponibles (2 sources + 2 sœurs flaccide)');
   assert(!document.querySelector('[data-action-item="x_vieux"]'),'invalidée non affichée');
   assert(!document.querySelector('[data-action-item="x_duo"]'),'pas_solo non affichée (règle convergée A/B : pas_solo disqualifie)');
   assert(document.querySelector('[data-actions-mode-cibler]')?.getAttribute('aria-selected')==='true','mode cibler actif');
   document.querySelector('[data-actions-fermer]').click();
   await wait(()=>!document.querySelector('[data-actions-fenetre]'),'actions window closed');
   // 2. Same window in EXCLUDE FROM DRAW mode: selecting an exclusion.
   await press('[data-casting-exclure-actions]');
   await wait(()=>document.querySelector('[data-actions-fenetre]'),'exclusion window open');
   await wait(()=>document.querySelector('[data-actions-mode-exclure]')?.getAttribute('aria-selected')==='true','mode exclure actif');
   await wait(()=>document.querySelectorAll('[data-action-item]').length===4,'même grille (pool exact) en mode exclure');
   document.querySelector('[data-action-item="x_pinces"]').click();
   await wait(()=>document.querySelector('[data-action-item="x_pinces"]')?.getAttribute('data-selectionne')==='oui','action exclue sélectionnée');
   assert(texte('[data-actions-compteur]').includes('1 exclue'),'compteur mode exclure : '+texte('[data-actions-compteur]'));
   await press('[data-actions-valider]');
   await wait(()=>document.querySelector('[data-casting-actions-exclues]'),'chip exclusion');
   assert(texte('[data-casting-actions-exclues]').includes('1 action exclue'),'chip « 1 action exclue » : '+texte('[data-casting-actions-exclues]'));
   // Mutual exclusion: only CHARACTER targeting locks with an exclusion
   // (backend semantics: the exclusion combines with targeted actions and fixed pose).
   assert(document.querySelector('[data-casting-cibler]').disabled,'Cibler des personnages désactivé avec exclusion active');
   assert(!document.querySelector('[data-casting-cibler-actions]').disabled,'Cibler des actions reste disponible avec exclusion (l\'exclusion gagne)');
   report.checks.push('exclusion : onglet dédié, même grille, invalidée cachée, chip « 1 action exclue », ciblage persos verrouillé');
   // 3. FREE BATCH with exclusion: N=2, family bny → launch. The exclude[] payload
   //     is proven at the ENGINE level by the argv journal (POST-RUN in the .sh).
   set('[data-casting-n]','2');
   set('[data-casting-famille]','bny');await pause(100);
   await press('[data-casting-lancer]');
   await wait(()=>document.querySelector('.lot-manager label select'),'filtre du lot');
   set('.lot-manager label select','all');
   await wait(()=>document.querySelectorAll('[data-casting-shot]').length===2,'deux essais du lot avec exclusion');
   // 4. Global bar in flight (mock-live: ~6 s per trial).
   await wait(()=>document.querySelector('[data-casting-runbar]'),'barre globale visible en vol');
   assert(texte('[data-casting-runbar-compte]').includes('/ 2 essais'),'compte essais : '+texte('[data-casting-runbar-compte]'));
   assert(texte('[data-casting-runbar-phase]').includes('Passe 1'),'passe courante affichée : '+texte('[data-casting-runbar-phase]'));
   assert(texte('[data-casting-pause-note]').includes("s'arrêtera à la fin de l'essai en cours")&&texte('[data-casting-pause-note]').includes('suspendre un sampler'),'libellé honnête de la pause : '+texte('[data-casting-pause-note]'));
   report.checks.push('barre globale : essais terminés / total, passe courante, libellé pause honnête');
   // 5. PAUSE / RESUME in flight — only if the backend exposes them.
   if(backend.casting_pause){
    await press('[data-casting-pause]');
    await wait(async()=>{const rows=await api('casting_history',{sync:false});return rows.some(r=>r.runState==='paused')},'état paused persisté');
    await wait(()=>document.querySelector('[data-casting-resume]'),'bouton Reprendre apparu');
    assert(!!document.querySelector('[data-casting-runbar-pause]'),'état pause affiché dans la barre');
    await press('[data-casting-resume]');
    await wait(()=>!document.querySelector('[data-casting-resume]'),'reprise : bouton Reprendre retiré');
    report.checks.push('pause/reprise : état paused persisté puis reprise de la file');
   }else report.attendu.push('pause/reprise : commandes casting_pause/casting_resume absentes du binaire (tranche B) — câblage UI seul, non exécuté');
   await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].length===2&&[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'les deux essais libres réussis');
   await wait(()=>document.querySelector('[data-casting-runbar]')===null,'barre cachée une fois le lot terminé');
   report.checks.push('barre globale disparaît quand tous les essais sont terminés');
   // 6. SAVE THIS CHARACTER: real sheet, never passes 2-6.
   const cartes=()=>[...document.querySelectorAll('[data-casting-shot]')];
   if(backend.casting_enregistrer_perso){
    cartes()[0].querySelector('[data-casting-preview]').click();
    await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-enregistrer]'),'détail : enregistrer présent');
    await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'détail : workflow complet présent');
    set('[data-casting-save-nom]','Saya Passe3 Test');
    await press('[data-casting-enregistrer]');
    await wait(()=>texte('[data-casting-enregistre-note]').includes('enregistrée'),'confirmation visible : '+texte('[data-casting-enregistre-note]'));
    assert(texte('[data-casting-enregistre-note]').includes('Saya Passe3 Test'),'confirmation porte le nom');
    const fiches=await api('characters');
    assert(fiches.some(f=>f.name==='Saya Passe3 Test'),'fiche réellement créée côté app');
    const stemZero=cartes()[0].getAttribute('data-casting-shot');
    let historique=await api('casting_history',{sync:false});
    let ligne=historique.find(r=>r.stem===stemZero);
    assert(ligne&&!ligne.phases?.['2'],'enregistrer n\'a PAS lancé les passes 2-6');
    report.checks.push('enregistrer perso : confirmation visible, fiche créée, aucune passe 2-6 lancée');
    // 7. FULL WORKFLOW: 6 real passes on THIS result (existing mechanics).
    await press('[data-casting-complet]');
    await wait(()=>cartes()[0].getAttribute('data-decision')==='oui','workflow complet lancé');
    await wait(()=>!!document.querySelector('[data-casting-runbar] [data-run-progress]'),'progression globale pendant la finition');
    await wait(async()=>{const rows=await api('casting_history',{sync:false});const l=rows.find(r=>r.stem===stemZero);return l&&l.runState==='completed'&&l.phases?.['6']?.state==='succeeded'&&l.finalAsset},'finition 6 passes terminée avec image finale');
    report.checks.push('workflow complet : RunProgress réel dans la barre globale, 6 passes, image finale');
   }else report.attendu.push('enregistrer perso / workflow complet via détail : casting_enregistrer_perso absente du binaire (tranche B) — workflow complet vérifié par les autres smokes');
   // 8. CHARACTER TARGETING: persos[] payload WITHOUT exclude (the two buttons are
   //    exclusive; the exclusion is removed first).
   document.querySelector('[data-casting-actions-exclues-retirer]').click();
   await wait(()=>!document.querySelector('[data-casting-actions-exclues]'),'exclusion retirée');
   await press('[data-casting-cibler]');
   await wait(()=>document.querySelector('[data-cibler-fenetre]'),'fenêtre cibler');
   await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===2,'deux tuiles du registre');
   tile('CH--9301').click();
   await wait(()=>tile('CH--9301')?.getAttribute('data-selected')==='oui','tuile ciblée');
   await press('[data-cibler-valider]');
   await wait(()=>document.querySelector('[data-casting-ciblage]'),'chip ciblage');
   set('[data-casting-famille]','');await pause(100);
   await press('[data-casting-lancer]');
   await wait(()=>cartes().some(c=>c.getAttribute('data-casting-shot')==='hum--9301-stub-cible-noobai'&&c.getAttribute('data-state')==='succeeded'),'essai ciblé réussi');
   report.checks.push('ciblage : CH--9301 seul au casting, essai réussi (payload --persos sans --exclude prouvé par le journal argv en POST-RUN)');
   // 9. CANCEL: relaunch ON the targeting (1 trial CH--9301), real interruption on
   //    the mock (slice B contract). The targeting stays set — it must persist as is.
   if(backend.casting_stop){
    await press('[data-casting-lancer]');
    await wait(()=>document.querySelector('[data-casting-runbar]'),'barre du lot à annuler');
    await press('[data-casting-stop]');
    await wait(async()=>{const rows=await api('casting_history',{sync:false});const l=rows[0];return l&&!actif(l.state)&&(l.runState==='stopped'||['cancelled','interrupted'].includes(l.state))},'essai annulé (état persisté, plus rien en vol)');
    await wait(()=>document.querySelector('[data-casting-runbar]')===null,'barre cachée après annulation');
    await wait(()=>!document.querySelector('[data-casting-lancer]').disabled,'nouveau casting disponible après annulation');
    report.checks.push('annuler : run arrêté, barre cachée, nouveau casting disponible');
   }else report.attendu.push('annuler : casting_stop absente du binaire (tranche B) — non exécuté');
   // 10. Scripts (real modes = backend ENGINE_SCRIPTS): WIRING only — engine_script
   //     is NOT invoked (real GPU script); the choice persists.
   await wait(()=>document.querySelector('[data-casting-scripts]'),'section scripts');
   const options=[...document.querySelectorAll('[data-casting-script] option')].map(o=>o.value).filter(Boolean);
   assert(JSON.stringify(options)===JSON.stringify(['attente','poses','lcs','duo','bny20']),'5 modes réels du moteur (ENGINE_SCRIPTS) listés : '+JSON.stringify(options));
   set('[data-casting-script]','poses');
   await wait(()=>document.querySelector('[data-casting-script-perso]'),'champ ID du perso apparu');
   set('[data-casting-script-perso]','CH--9301');
   report.checks.push('scripts : 5 modes réels listés, champ ID conditionnel, aucune invocation GPU (le bouton n\'est jamais cliqué — vrai script)');
   // 11. IMMEDIATE PERSISTENCE: readable in the preferences WHILE the page is open.
   set('[data-casting-tri]','anciens');
   await wait(async()=>{const p=await api('preferences');return p.castingWorkspace?.grilleTri==='anciens'},'tri persisté immédiatement');
   let prefs=await api('preferences');
   assert(JSON.stringify(prefs.castingWorkspace?.persos)===JSON.stringify(['CH--9301']),'persos ciblés persistés');
   assert(prefs.castingWorkspace?.script==='poses','mode script persisté');
   report.checks.push('persistance immédiate : tri, ciblages et script lisibles dans castingWorkspace sans quitter la page');
   // 12. Round-trip navigation: exact state found again.
   await nav('Personnages');
   await wait(()=>document.querySelector('[data-nav="characters"][aria-current="page"]'),'page personnages');
   await nav('Casting');
   await wait(()=>document.querySelector('[data-casting-lancer]'),'casting re-render');
   await wait(()=>document.querySelector('[data-casting-script]')?.value==='poses','mode script restauré après navigation');
   await wait(()=>document.querySelector('[data-casting-script-perso]'),'champ ID restauré');
   await wait(()=>document.querySelector('[data-casting-tri]')?.value==='anciens','tri restauré');
   report.expected={};
   report.capture=await invoke('smoke_capture',{name:'casting-passe3'});
   report.checks.push('retour sur la page : script, champ ID et filtres retrouvés exactement');
  }else{
   // RESTART: the options and results survive the process.
   await wait(()=>document.querySelector('[data-nav]'),'nav');
   await nav('Casting');
   await wait(()=>document.querySelector('[data-casting-lancer]'),'casting menu');
   await wait(()=>document.querySelector('[data-casting-script]')?.value==='poses','mode script survit au redémarrage');
   const prefs=await api('preferences');
   assert(prefs.castingWorkspace?.grilleTri==='anciens','tri survit au redémarrage');
   assert(JSON.stringify(prefs.castingWorkspace?.persos)===JSON.stringify(['CH--9301']),'ciblage survit au redémarrage');
   assert(prefs.castingWorkspace?.script==='poses','mode script survit au redémarrage (préférence)');
   const historique=await api('casting_history',{sync:false});
   const fini=historique.find(r=>r.decision==='oui');
   if(fini)assert(fini.runState==='completed'&&fini.phases?.['6']?.state==='succeeded','finition terminée persiste');
   else report.attendu.push('finition non testée au run initial (backend B absent) — rien à vérifier au redémarrage');
   assert(document.querySelector('[data-casting-runbar]')===null,'aucune barre de run au repos');
   report.checks.push('redémarrage : options castingWorkspace et résultats du casting restaurés');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
