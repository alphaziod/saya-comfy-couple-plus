(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const click=async(text)=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click()};
 const change=(el,value)=>{el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 const check=(el,value)=>{el.checked=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 const total=async(state='')=>(await api('gallery',{filters:{availability:state}})).total;
 const cards=()=>[...document.querySelectorAll('.image-grid .image-card')];
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  await wait(()=>document.querySelector('[data-availability-filter]'),'Gallery ready');
  if(window.__V12_SMOKE_PHASE__==='restart'){
   report.expected=window.__V12_SMOKE_PREVIOUS__.expected;
   assert(await total()===129&&await total('missing')===1,'Trash state not persisted');
   assert((await api('detail',{id:report.expected.id})).availability==='missing','Historical asset lost');
   assert(document.querySelector('[data-availability-filter]').value==='missing','Missing filter not persisted');
   report.checks.push('restart: missing filter and historical identity preserved');
  }else{
   await click('Indexer / actualiser');await wait(async()=>await total()===130&&!(await api('index_status')).running,'Index complete');
   check(document.querySelector('[data-stream-toggle]'),false);await wait(()=>cards().length===48,'Paged cards');
   check(document.querySelector('[data-selection-mode]'),true);cards()[0].click();cards()[1].click();
   await click('Retirer du catalogue');await wait(async()=>await total()===128,'Catalogue removal');
   assert(await total('removed')===2,'Removed view missing');
   change(document.querySelector('[data-availability-filter]'),'removed');await wait(()=>cards().length===2,'Removed cards');
   cards()[0].click();cards()[1].click();await click('Restaurer au catalogue');await wait(async()=>await total()===130,'Restore');
   change(document.querySelector('[data-availability-filter]'),'');await wait(()=>cards().length===48,'Active cards');
   cards()[0].click();await click('Mettre à la corbeille…');await wait(()=>document.querySelector('[data-confirm-trash]'),'Confirmation dialog');
   const first=(await api('gallery',{filters:{}})).items[0];
   const detail=await api('detail',{id:first.id});assert(document.querySelector('dialog').textContent.includes(detail.path),'Exact target path absent');
   await click('Annuler');assert(await total()===130,'Cancel changed catalogue');
   await click('Mettre à la corbeille…');await click('Confirmer la mise à la corbeille');
   await wait(async()=>await total()===129&&await total('missing')===1,'Gio trash failed');
   change(document.querySelector('[data-availability-filter]'),'missing');await wait(()=>cards().length===1,'Missing image view');
   report.expected={id:first.id,path:detail.path,sha256:first.contentSha256};
   report.checks.push('multi-selection catalogue remove/restore, real dialog cancellation, confirmed Gio trash on isolated fixture, missing view');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e)}await invoke('finish_smoke',{report});
})();
