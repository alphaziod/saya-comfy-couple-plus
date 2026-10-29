(async()=>{
 // Visual tour for UI review: synthetic casting batch (stub engine), then one
 // window capture per main page. Captures land in the isolated data dir.
 const report={ok:false,checks:[],captures:{}};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,150))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
 const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const shot=async name=>{await new Promise(r=>setTimeout(r,1200));report.captures[name]=await invoke('smoke_capture',{name})};
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){await shot('ui-00-language');document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
  await wait(()=>document.querySelector('nav'),'navigation');
  await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
  await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');
  await shot('ui-01-generation');
  await nav('Casting');await wait(()=>document.querySelector('[data-casting-source]')?.textContent.includes('Config consommée'),'casting source');
  set('[data-casting-n]','6');await press('[data-casting-lancer]');
  await wait(async()=>(await api('casting_history',{sync:true})).filter(r=>r.asset).length>=6,'six images',240000);
  await shot('ui-02-casting');
  document.querySelector('[data-casting-preview]').click();await wait(()=>document.querySelector('[data-casting-preview-dialog] img'),'preview');
  await shot('ui-03-casting-preview');document.querySelector('[data-casting-preview-fermer]').click();
  for(const b of [...document.querySelectorAll('[data-casting-select]')].slice(0,3))b.click();
  await press('[data-casting-publish-selection]');await wait(async()=>(await api('casting_history',{sync:false})).filter(r=>r.published).length===3,'published');
  for(const [n,label] of [['04-gallery','Galerie'],['05-characters','Personnages'],['06-lora','LoRA'],['07-scenes','Scènes'],['08-workflows','Workflows'],['09-pipelines','Pipelines'],['10-modules','Modules'],['11-graph','Éditeur graphique'],['12-engine','ComfyUI'],['13-data','Données'],['14-settings','Paramètres'],['15-ablab','Labo A/B']]){
   await nav(label);await shot('ui-'+n);
  }
  // Arabic: right-to-left layout check on the densest pages.
  await nav('Paramètres');await wait(()=>document.querySelector('[data-langue="ar"]'),'settings languages');document.querySelector('[data-langue="ar"]').click();
  await wait(()=>document.documentElement.dir==='rtl','rtl applied');await shot('ui-20-ar-settings');
  for(const [n,id] of [['21-ar-casting','casting'],['22-ar-gallery','gallery'],['23-ar-generation','generation']]){document.querySelector(`[data-nav="${id}"]`)?.click();await shot('ui-'+n);}
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
