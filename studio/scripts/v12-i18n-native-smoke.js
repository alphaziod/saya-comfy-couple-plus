(async()=>{
 const report={ok:false,checks:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+65000;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,120))}};
 const check=(v,m)=>{report.checks.push({m,ok:!!v});if(!v)throw Error(m)};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 try{
  await wait(()=>document.querySelector('nav'),'navigation');
  const initial=window.__V12_SMOKE_PHASE__==='initial';
  if(initial)await wait(()=>document.querySelector('[data-langue-setup]'),'popup premier démarrage');
  const popup=document.querySelector('[data-langue-setup]');
  if(popup){
   // PHASE 1 — first launch without a preference: popup, 14 languages, French chosen.
   check(popup.querySelectorAll('[data-langue]').length===14,'14 langues proposées');
   check(popup.querySelector('[data-langue]').textContent.trim()==='Français','Français en premier');
   popup.querySelector('[data-langue="fr"]').click();
   await new Promise(r=>setTimeout(r,400));
   popup.querySelector('[data-langue-confirmer]').click();
   await new Promise(r=>setTimeout(r,1200));
   await wait(()=>!document.querySelector('[data-langue-setup]'),'popup fermée après confirmation');
   check((await api('preferences')).language==='fr','langue fr persistée');
   check(document.documentElement.getAttribute('lang')==='fr','attribut lang = fr');
   report.ok=true;await invoke('finish_smoke',{report});return;
  }
  // PHASE 2 — relaunch: popup absent, language kept, changes in Paramètres.
  const prefs=await api('preferences');
  check(!document.querySelector('[data-langue-setup]'),'popup absente au redémarrage');
  check(prefs.language==='fr','langue fr conservée après redémarrage');
  check(prefs.language_setup_completed===true,'language_setup_completed persistant');
  await nav('Paramètres');
  await wait(()=>document.querySelector('[data-settings-langues]'),'section Langue visible');
  // Japanese: the interface changes immediately.
  document.querySelector('[data-settings-langues] [data-langue="ja"]').click();
  await wait(()=>document.documentElement.getAttribute('lang')==='ja','switch immédiat → ja');
  {const titre=document.querySelector('[data-settings-interface]');check(!!titre&&titre.textContent.trim()!=='Interface','titre de section traduit en japonais');}
  check(document.querySelector('.page-heading h1')?.textContent.trim()!=='Paramètres','titre traduit en japonais');
  // Arabe : RTL.
  document.querySelector('[data-settings-langues] [data-langue="ar"]').click();
  await wait(()=>document.documentElement.getAttribute('dir')==='rtl','arabe → dir=rtl');
  await wait(()=>!document.querySelector('[data-settings-langues] [data-langue="en"]').disabled,'sauvegarde arabe terminée');
  // Back to English: LTR restored.
  document.querySelector('[data-settings-langues] [data-langue="en"]').click();
  await wait(()=>document.documentElement.getAttribute('dir')==='ltr'&&document.documentElement.getAttribute('lang')==='en','retour anglais → LTR');
  await wait(async()=>(await api('preferences')).language==='en','préférence en persistée');
  report.ok=true;
 }catch(e){report.failure=String(e)}
 window.__SMOKE_STAGE=report;
 await invoke('finish_smoke',{report});
 })();
