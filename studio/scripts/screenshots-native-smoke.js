(async()=>{
 // README screenshots in an isolated profile (stub engine, mock ComfyUI serving the real PNGs of
 // MOCK_COMFY_IMAGES). First launch prepares everything (a full six-pass run, a casting batch, kept
 // images); the restart opens the app on that data and only takes the captures: Generation, Casting,
 // Gallery. Captures land in the isolated data dir.
 const report={ok:false,checks:[],captures:{}};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,150))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const go=async id=>{await wait(()=>document.querySelector(`[data-nav="${id}"]`),'nav '+id);document.querySelector(`[data-nav="${id}"]`).click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const loaded=sel=>[...document.querySelectorAll(sel)].filter(i=>i.complete&&i.naturalWidth>64).length;
 const shot=async name=>{await new Promise(r=>setTimeout(r,2000));report.captures[name]=await invoke('smoke_capture',{name})};
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language set')}
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   // Preparation, no capture.
   await go('generation');await wait(()=>document.querySelector('[data-saya-source]'),'source');
   set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
   await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');
   await press('[data-saya-start]');
   await wait(()=>document.querySelector('[data-saya-review-popup]'),'phase 1 review',180000);
   await press('[data-review-continue]');
   await wait(()=>document.querySelector('[data-saya-phase="6"]')?.dataset.state==='succeeded','six passes',300000);
   await go('casting');await wait(()=>document.querySelector('[data-casting-lancer]'),'casting ready');
   set('[data-casting-n]','6');await press('[data-casting-lancer]');
   await wait(async()=>(await api('casting_history',{sync:true})).filter(r=>r.asset).length>=6,'six casting images',420000);
   for(const b of [...document.querySelectorAll('[data-casting-select]')].slice(0,6))b.click();
   await press('[data-casting-publish-selection]');
   await wait(async()=>(await api('casting_history',{sync:false})).filter(r=>r.published).length>=6,'kept in gallery');
  }else{
   // Restart: everything is already there; only the captures.
   await go('generation');await wait(()=>document.querySelector('[data-saya-state]'),'run restored');
   await wait(()=>loaded('main img')>0,'generation preview',60000);
   const source=document.querySelector('[data-saya-source]');if(source)source.scrollLeft=source.scrollWidth;  // file name, not the folder
   await shot('readme-1-generation');
   await go('casting');await wait(()=>loaded('[data-casting-shot] img')>=6,'casting images',60000);
   await shot('readme-2-casting');
   await go('gallery');await wait(()=>loaded('img')>=6,'gallery images',60000);
   await shot('readme-3-gallery');
  }
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
