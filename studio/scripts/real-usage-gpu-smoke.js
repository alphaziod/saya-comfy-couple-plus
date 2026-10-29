(async()=>{
 // REAL-USAGE GPU smoke (real ComfyUI on 8188, real engine SANDBOX copy patched to SFW
 // prompts, SFW preset copy). Phase "initial": six-pass generation from Create, casting
 // start → pause → stop. App restart. Phase "restart": casting batch resumed and finished,
 // full workflow on one trial, save a trial as character, publish to gallery, gallery
 // sorts and favourites, create then archive (remove) a character, delete the batch.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],timings:{}};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error('timeout: '+m);await pause(500)}};
 const press=async(s,ms)=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s,ms);document.querySelector(s).click()};
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const NAV={'Créer':'generation','Casting':'casting','Galerie':'gallery','Persos':'characters'};
 const nav=async t=>{const b=document.querySelector(`[data-nav="${NAV[t]??t}"]`);if(!b)throw Error('nav '+t);b.click();await pause(400)};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const text=s=>document.querySelector(s)?.textContent??'';
 const t0=Date.now(),lap=k=>{report.timings[k]=Math.round((Date.now()-t0)/1000)};
 const galleryIds=async(sort='name_asc')=>(await api('gallery',{filters:{sort}})).items.map(a=>a.id);
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
  await wait(()=>document.querySelector('[data-nav]'),'navigation');
  const conn=await api('connection');assert(conn.connected,'real ComfyUI connected: '+JSON.stringify(conn).slice(0,200));
  report.comfy={version:conn.version};
  if(report.phase==='initial'){
   // 1. Six-pass generation from Create.
   await nav('Créer');await wait(()=>document.querySelector('[data-saya-source]'),'source field');
   set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
   await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'session opened');
   await press('[data-saya-start]',120000);
   await wait(()=>document.querySelector('[data-saya-phase="1"]')?.getAttribute('data-state')==='succeeded','phase 1 rendered',1200000);lap('phase1');
   report.checks.push('Créer : passe 1 rendue sur le vrai ComfyUI');
   await press('[data-review-continue]',60000);
   await wait(()=>[1,2,3,4,5,6].every(n=>document.querySelector(`[data-saya-phase="${n}"]`)?.getAttribute('data-state')==='succeeded'),'phases 2-6 rendered',3600000);lap('phase6');
   const runs=await api('saya_runs');report.generationRun=runs[0]?.id;
   report.checks.push('Créer : 6 passes enchaînées et réussies');
   await wait(async()=>(await galleryIds()).length>=1,'final image visible in gallery',120000);
   report.galleryAfterGeneration=(await galleryIds()).length;
   report.checks.push('Galerie : finale de la génération présente ('+report.galleryAfterGeneration+')');
   // 2. Casting: start 3 trials, pause after the first submission, stop with 1 image.
   await nav('Casting');await wait(()=>text('[data-casting-source]').includes('Config consommée'),'casting source');
   set('[data-casting-n]','3');await press('[data-casting-lancer]',120000);lap('castingStart');
   await wait(async()=>(await api('casting_history',{sync:false})).some(r=>['submitted','queued','running'].includes(r.state)),'first trial submitted',600000);
   report.checks.push('Casting : lot de 3 lancé (vrai moteur, copie SFW)');
   await press('[data-casting-pause]',60000);
   await wait(async()=>{const rows=await api('casting_history',{sync:true});return rows.length===3&&rows.filter(r=>r.asset).length===1},'one image done while paused',900000);
   await pause(20000);
   const paused=await api('casting_history',{sync:true});
   assert(paused.filter(r=>r.asset).length===1&&!paused.some(r=>['submitted','queued','running'].includes(r.state)),'pause holds: nothing new submitted');
   report.checks.push('Casting : pause respectée (1 image, rien de nouveau soumis en 20 s)');
   await press('[data-casting-stop]',60000);await wait(()=>!document.querySelector('[data-casting-runbar]'),'batch stopped',120000);lap('castingStopped');
   report.before=paused;report.checks.push('Casting : arrêt à 1/3');
   report.ok=true;await invoke('finish_smoke',{report});return;
  }
  // ---- after app restart ----
  const {before}=window.__V12_SMOKE_PREVIOUS__||{};assert(before,'initial phase evidence');
  const done=before.find(r=>r.asset);
  await nav('Casting');await wait(()=>text('[data-casting-lot]').includes('1/3'),'1/3 kept after restart',60000);
  report.checks.push('Redémarrage : lot toujours à 1/3');
  await wait(()=>{const b=document.querySelector('[data-casting-retry]');return b&&!b.disabled&&b.textContent.includes('Terminer le lot')},'finish button');
  document.querySelector('[data-casting-retry]').click();lap('resume');
  await wait(async()=>{const rows=await api('casting_history',{sync:true});return rows.length===3&&rows.every(r=>r.asset)},'three images',1800000);lap('castingDone');
  const after=await api('casting_history',{sync:false});
  assert(after.find(r=>r.run===done.run).asset===done.asset,'first image not regenerated');
  await wait(()=>text('[data-casting-lot]').includes('3/3'),'3/3');
  report.checks.push('Casting : reprise « Terminer le lot » → 3/3, image déjà faite conservée');
  // Full workflow on one trial + save it as a character.
  const cards=()=>[...document.querySelectorAll('[data-casting-shot]')];
  cards()[0].querySelector('[data-casting-preview]').click();
  await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'detail actions');
  set('[data-casting-preview-dialog] [data-casting-save-nom]','Essai Réel SFW');
  await press('[data-casting-preview-dialog] [data-casting-enregistrer]');
  await wait(async()=>(await api('characters')).some(c=>c.name==='Essai Réel SFW'),'character saved from casting');
  report.checks.push('Persos : essai casting enregistré comme personnage');
  document.querySelector('[data-casting-preview-dialog] [data-casting-complet]').click();
  if(document.querySelector('[data-casting-preview-fermer]'))document.querySelector('[data-casting-preview-fermer]').click();
  await wait(async()=>{const r=(await api('casting_history',{sync:true})).find(x=>x.decision==='oui');return r&&[1,2,3,4,5,6].every(n=>r.phases?.[String(n)]?.state==='succeeded')},'full workflow on trial',3600000);lap('castingFull');
  report.checks.push('Casting : workflow complet (6 passes) sur un essai');
  // Publish selection to the gallery (no duplicates).
  const g0=(await galleryIds()).length;
  const boxes=[...document.querySelectorAll('[data-casting-select]')];assert(boxes.length>=2,'selection boxes');
  boxes.slice(0,2).forEach(b=>b.click());await press('[data-casting-publish-selection]');
  await wait(async()=>(await galleryIds()).length>=g0+1,'published to gallery',60000);
  const g1=(await galleryIds()).length;
  const again=await api('casting_publish',{runs:(await api('casting_history',{sync:false})).filter(r=>r.published).map(r=>r.run)});
  assert(again.published===0&&(await galleryIds()).length===g1,'second publish adds nothing');
  report.checks.push(`Galerie : publication ${g0} → ${g1}, re-publication sans doublon`);
  // Gallery sorts and favourites.
  await nav('Galerie');await wait(()=>document.querySelector('[data-gallery-sort]'),'gallery');
  const asc=await galleryIds('name_asc'),desc=await galleryIds('name_desc');
  assert(asc.length>=3&&asc.join()===[...desc].reverse().join(),'name A→Z is the reverse of Z→A');
  const newest=await galleryIds('newest'),oldest=await galleryIds('oldest');
  assert(newest.length===oldest.length&&[...newest].sort().join()===[...oldest].sort().join(),'newest/oldest list the same images');
  assert(newest[0]!==oldest[0]||newest.length===1,'newest and oldest start differently');
  for(const s of ['favorites','accepted','character','resolution','filesize','shuffle']){const ids=await galleryIds(s);assert(ids.length===asc.length,'sort '+s+' keeps every image');}
  for(const s of ['name_desc','newest','resolution']){set('[data-gallery-sort]',s);await wait(()=>document.querySelector('[data-stream-asset]'),'tiles after sort '+s);}
  await api('annotate',{id:asc[asc.length-1],favorite:true,note:'',review:'unreviewed'});
  const fav=await galleryIds('favorites');assert(fav[0]===asc[asc.length-1],'favourite first');
  const favOnly=(await api('gallery',{filters:{favorite:true}})).items.map(a=>a.id);assert(favOnly.length===1,'favourites filter');
  report.checks.push('Galerie : 10 tris cohérents ('+asc.length+' images), favori en tête, filtre Favoris = 1');
  // Characters: create a new one, then archive (remove) it, then restore.
  await nav('Persos');await wait(()=>document.querySelector('.new-btn'),'characters page');
  document.querySelector('.new-btn').click();await pause(300);
  const form=document.querySelector('#character-sheet-form');assert(form,'sheet form');
  const nameInput=form.querySelector('input[required]');nameInput.value='Perso Test Ajout';nameInput.dispatchEvent(new Event('input',{bubbles:true}));
  const areas=form.querySelectorAll('textarea');areas[1].value='1girl, solo, casual clothes, fully clothed';areas[1].dispatchEvent(new Event('input',{bubbles:true}));
  await pause(200);document.querySelector('button[form="character-sheet-form"]').click();
  await wait(async()=>(await api('characters')).some(c=>c.name==='Perso Test Ajout'),'character created');
  report.checks.push('Persos : personnage ajouté via le formulaire');
  const archiveBtn=()=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Archiver la fiche');
  await wait(()=>archiveBtn()&&!archiveBtn().disabled,'archive button');archiveBtn().click();
  await wait(async()=>(await api('characters',{include_archived:true})).find(c=>c.name==='Perso Test Ajout')?.archived===true,'character archived');
  assert(!(await api('characters')).some(c=>c.name==='Perso Test Ajout'),'archived character hidden from active list');
  report.checks.push('Persos : personnage retiré (archivé), absent de la liste active');
  // Delete the casting batch: published images stay.
  await nav('Casting');window.confirm=()=>true;
  const kept=await galleryIds();await press('[data-casting-delete]');
  await wait(async()=>(await api('casting_history',{sync:false})).length===0,'batch deleted',60000);
  assert((await galleryIds()).join()===kept.join(),'gallery unchanged after batch deletion');
  report.checks.push('Casting : lot supprimé, images publiées conservées');
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
