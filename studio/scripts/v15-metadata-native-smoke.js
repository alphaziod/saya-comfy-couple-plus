(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const check=(el,value)=>{el.checked=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  await wait(()=>document.querySelector('[data-metadata-only]'),'Gallery ready');
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled,'Ready to index');button('Indexer / actualiser').click();
   await wait(async()=>(await api('gallery',{filters:{}})).total===131&&!(await api('index_status')).running,'Index complete');
   check(document.querySelector('[data-stream-toggle]'),false);check(document.querySelector('[data-metadata-only]'),true);
   const search=document.querySelector('input[placeholder="Rechercher…"]');search.value='metadata.png';search.dispatchEvent(new Event('input',{bubbles:true}));
  }else assert(document.querySelector('[data-metadata-only]').checked,'Metadata filter did not persist');
  await wait(()=>document.querySelectorAll('.image-grid .image-card').length===1,'Metadata-only result');
  document.querySelector('.image-grid .image-card').click();
  await wait(()=>document.querySelector('[data-generation-mode]'),'Metadata rendered');
  assert(document.querySelector('[data-generation-mode]').textContent.includes('SOLO'),'Mode not SOLO');
  assert(!document.querySelector('[data-prompt-role="P2"]'),'Inactive P2 displayed');
  assert([...document.querySelectorAll('[data-prompt-role="P1"] pre')].some(p=>p.textContent==='  exact, (tag:1.2)\nété 🌸\n'),'Prompt whitespace altered');
  // The exact seed stays covered by the Rust tests (display removed from the panel).
  assert(!document.querySelector('[data-prompt-role="NEG"]'),'Empty NEG must stay hidden');
  // The exact seed stays covered by the Rust tests (display removed from the panel).
  report.checks.push('native PNG indexing, metadata-only persisted, SOLO active branches, exact multiline Unicode prompt, empty NEG hidden');
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e)}await invoke('finish_smoke',{report});
})();
