// Screenshot helper for scripts/gallery-detail-capture.py.
// Opens the gallery detail panel on the synthetic metadata.png fixture, then
// holds the window open. Each panel state is signalled to the harness through
// the persisted galleryFilters preference (search sentinel), because
// set_preference refuses unknown keys. Never calls finish_smoke.
(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(p,m)=>{const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const check=(el,value)=>{el.checked=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 const searchSentinel=text=>{const search=document.querySelector('input[placeholder="Rechercher…"]');search.value=text;search.dispatchEvent(new Event('input',{bubbles:true}));};
 try{
  await wait(()=>document.querySelector('[data-metadata-only]'),'Gallery ready');
  await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled,'Ready to index');button('Indexer / actualiser').click();
  await wait(async()=>(await api('gallery',{filters:{}})).total===131&&!(await api('index_status')).running,'Index complete');
  check(document.querySelector('[data-stream-toggle]'),false);check(document.querySelector('[data-metadata-only]'),true);
  const search=document.querySelector('input[placeholder="Rechercher…"]');search.value='metadata.png';search.dispatchEvent(new Event('input',{bubbles:true}));
  await wait(()=>document.querySelectorAll('.image-grid .image-card').length===1,'Metadata-only result');
  document.querySelector('.image-grid .image-card').click();
  await wait(()=>document.querySelector('[data-prompts-box]'),'Detail panel rendered');
  // State 1: top of the panel (compact context + Prompts cards).
  searchSentinel('metadata.png ready');
  await wait(async()=>((await api('preferences')).galleryFilters??{}).search==='metadata.png ready','Top state persisted');
  // The top capture is done: the harness writes galleryDetailShot.
  await wait(async()=>(await api('preferences')).galleryDetailShot==='1-done','Top capture done');
  // State 2: bottom of the panel (annotations + associated characters).
  const pane=document.querySelector('.metadata-pane');
  pane.scrollTop=pane.scrollHeight;
  await wait(()=>pane.scrollTop+pane.clientHeight>=pane.scrollHeight-4,'Pane scrolled to bottom');
  searchSentinel('metadata.png done');
  await wait(async()=>((await api('preferences')).galleryFilters??{}).search==='metadata.png done','Bottom state persisted');
 }catch(e){console.error('capture smoke failed:',e);searchSentinel('capture-error')}
 await new Promise(()=>{});
})();
