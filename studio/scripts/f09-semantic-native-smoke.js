(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m,ms=120000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,100));}};
 const click=async text=>{const b=()=>[...document.querySelectorAll('button')].find(n=>n.textContent.trim()===text);await wait(()=>b()&&!b().disabled,text);b().click();};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  await wait(()=>document.querySelector('[data-nav="gallery"][aria-current="page"]'),'Galerie');
  await click('Indexer / actualiser');
  await wait(async()=>{const g=await api('gallery',{filters:{}});const s=await api('index_status');return g.total>=130&&!s.running;},'indexed',60000);
  await wait(()=>document.querySelector('.stream-card'),'stream card');
  const card=document.querySelector('.stream-card');const asset=card.dataset.streamAsset;
  card.click();
  await wait(()=>document.querySelector('[data-find-similar]'),'viewer');
  document.querySelector('[data-find-similar]').click();
  await wait(()=>document.querySelector('[data-semantic-status]'),'semantic section');
  const diagnostic=await api('semantic_diagnose');assert(diagnostic.ok===true,'sidecar diagnostic failed: '+JSON.stringify(diagnostic));assert(diagnostic.imports&&diagnostic.imports.torch,'torch import missing');report.diagnostic={ok:diagnostic.ok,imports:diagnostic.imports};
  const status=await api('semantic_status');report.debug=status;assert(status.available===true,'semantic unavailable: '+JSON.stringify(status));
  document.querySelector('[data-semantic-index]').click();
  await wait(async()=>{const s=await api('semantic_status');return s.indexed>0;},'semantic indexed',150000);
  const after=await api('semantic_status');
  assert(after.sidecar.resident===true,'sidecar should be resident');
  const launches=after.sidecar.launches;
  const rssBefore=after.sidecar.rss;
  await new Promise(r=>setTimeout(r,4000));
  const idle=await api('semantic_status');
  assert(idle.sidecar.resident===false,'sidecar did not stop after idle');
  assert(idle.sidecar.lastRss>0,'sidecar RSS was not measured before idle');
  document.querySelector('[data-semantic-index]').click();
  await wait(async()=>{const s=await api('semantic_status');return s.indexed>after.indexed;},'second semantic batch',150000);
  const restarted=await api('semantic_status');
  assert(restarted.sidecar.launches>launches,'sidecar did not restart');
  const result=await api('semantic_search',{asset,limit:5});
  assert(result.matches.length>0,'no semantic matches');
  if(!document.querySelector('[data-semantic-find]')) {
    const close=[...document.querySelectorAll('dialog[open] button')].find(b=>b.textContent.includes('Fermer'));
    if(close) close.click();
    await wait(()=>document.querySelector('[data-find-similar]'),'reopen viewer');
    document.querySelector('[data-find-similar]').click();
    await wait(()=>document.querySelector('[data-semantic-find]'),'semantic section reopened');
  }
  document.querySelector('[data-semantic-find]').click();
  await wait(()=>document.querySelector('[data-semantic-matches] .image-card'),'semantic matches rendered',30000);
  report.checks.push({available:status.available,model:status.model,dim:status.dim,indexed:after.indexed,secondIndexed:restarted.indexed,launches,restartedLaunches:restarted.sidecar.launches,matches:result.matches.length,rssBefore,idleRss:idle.sidecar.lastRss,top:result.matches[0].score});
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e.stack||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
