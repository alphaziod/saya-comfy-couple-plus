(async()=>{
 // Real GPU: four casting batches of five pass-1 trials (families hum, kmn, elf, cute; SFW
 // sandbox engine with varied hair/outfit/place/activity), a full six-pass workflow on one
 // trial of three batches, everything published. Real test images for the next smokes.
 const report={ok:false,checks:[],timings:{},batches:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error('timeout: '+m);await pause(1000)}};
 const press=async(s,ms)=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s,ms);document.querySelector(s).click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const t0=Date.now(),lap=k=>{report.timings[k]=Math.round((Date.now()-t0)/1000)};
 const history=sync=>api('casting_history',{sync});
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
  document.querySelector('[data-nav="generation"]').click();await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
  await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'session');
  document.querySelector('[data-nav="casting"]').click();
  await wait(()=>(document.querySelector('[data-casting-source]')?.textContent??'').includes('Config consommée'),'casting source');
  const families=['hum','kmn','elf','cute'];let total=0;
  for(const [i,family] of families.entries()){
   await wait(()=>document.querySelector('[data-casting-famille]')&&!document.querySelector('[data-casting-famille]').disabled,'family select free',1800000);
   set('[data-casting-famille]',family);set('[data-casting-n]','5');await press('[data-casting-lancer]',1800000);
   total+=5;
   await wait(async()=>{const rows=await history(true);return rows.length>=total&&rows.filter(r=>r.asset).length>=total},'batch '+family,2400000);
   lap('batch_'+family);report.batches.push(family);
   if(i<3){
    const card=document.querySelector('[data-casting-shot]');card.querySelector('[data-casting-preview]').click();
    await wait(()=>document.querySelector('[data-casting-preview-dialog] [data-casting-complet]'),'detail '+family);
    document.querySelector('[data-casting-preview-dialog] [data-casting-complet]').click();
    await pause(1500);document.querySelector('[data-casting-preview-fermer]')?.click();
    report.checks.push('lot '+family+' : 5 essais P1, workflow complet lancé sur 1 essai');
   }else report.checks.push('lot '+family+' : 5 essais P1');
  }
  await wait(async()=>{const rows=await history(true);report.fullDone=rows.filter(r=>r.decision==='oui'&&r.phases?.['6']?.state==='succeeded').length;report.autoErrors=rows.map(r=>r.autoError).filter(Boolean);return report.fullDone>=3},'three full workflows',5400000);
  lap('fullWorkflows');report.checks.push('3 workflows complets (P6) terminés');
  const rows=await history(false);
  report.trials=rows.length;report.families=[...new Set(rows.map(r=>r.famille??r.shot?.famille??r.family))];
  const res=await api('casting_publish',{runs:rows.map(r=>r.run)});report.published=res.published;
  const gallery=(await api('gallery',{filters:{}})).items;report.gallery=gallery.length;
  report.checks.push(`publication : ${res.published} ; galerie : ${gallery.length} images (attendu 20 : 17 P1 + 3 finales)`);
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP}
 await invoke('finish_smoke',{report});
})();
