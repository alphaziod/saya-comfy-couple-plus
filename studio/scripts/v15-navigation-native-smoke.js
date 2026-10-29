(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const click=async(text)=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click()};
 const check=(el,value)=>{el.checked=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 const change=(el,value)=>{el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}))};
 const title=()=>document.querySelector('.image-dialog .dialog-toolbar strong')?.textContent;
 const ready=()=>document.querySelector('[data-viewer-position]')&&!document.querySelector('[data-viewer-position]').textContent.includes('Chargement');
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  await wait(()=>document.querySelector('[data-stream-toggle]'),'Gallery ready');
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   await click('Indexer / actualiser');await wait(async()=>(await api('gallery',{filters:{}})).total===130&&!(await api('index_status')).running,'Index complete');
   const search=document.querySelector('input[placeholder="Rechercher…"]');search.value='alpha';search.dispatchEvent(new Event('input',{bubbles:true}));
   change(document.querySelector('select option[value="name_desc"]').parentElement,'name_desc');
  }else assert(document.querySelector('input[placeholder="Rechercher…"]').value==='alpha','Search not persisted');
  check(document.querySelector('[data-stream-toggle]'),false);
  await wait(()=>document.querySelectorAll('.image-grid .image-card').length===48,'Filtered page');
  const page=await api('gallery',{filters:{search:'alpha',sort:'name_desc'}});assert(page.total>48,'Fixture insufficient');
  document.querySelector('.image-grid .image-card').click();await wait(()=>document.querySelector('[data-inspector-open]'),'Inspector');document.querySelector('[data-inspector-open]').click();await wait(ready,'First viewer');
  assert(title()===page.items[0].path,'Wrong first image');assert(document.querySelector('[data-viewer-previous]').disabled,'Start boundary');
  document.querySelector('[data-viewer-next]').click();await wait(()=>ready()&&title()===page.items[1].path,'Next button');
  document.querySelector('.image-dialog').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));await wait(()=>ready()&&title()===page.items[2].path,'Right arrow');
  const note=document.querySelector('[data-annotation-note]');note.focus();note.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true,cancelable:true}));await new Promise(r=>setTimeout(r,200));assert(title()===page.items[2].path,'Textarea intercepted');
  document.querySelector('.image-dialog').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true,cancelable:true}));await wait(()=>ready()&&title()===page.items[1].path,'Left arrow');
  await click('Fermer ×');await wait(()=>!document.querySelector('.image-dialog'),'Close');
  check(document.querySelector('[data-stream-toggle]'),true);await wait(()=>document.querySelectorAll('[data-stream-asset]').length>10,'Stream mounted');
  const viewport=document.querySelector('.stream-viewport');viewport.scrollTop=850;viewport.dispatchEvent(new Event('scroll'));await new Promise(r=>setTimeout(r,200));
  const top=viewport.scrollTop;assert(top>0,'Scroll fixture not tall');const cards=[...document.querySelectorAll('[data-stream-asset]')];const visible=cards.find(c=>c.getBoundingClientRect().top>=viewport.getBoundingClientRect().top);assert(visible,'Visible anchor absent');visible.click();await wait(()=>document.querySelector('[data-inspector-open]'),'Scrolled inspector');document.querySelector('[data-inspector-open]').click();await wait(ready,'Scrolled viewer');
  if(!document.querySelector('[data-viewer-next]').disabled){document.querySelector('[data-viewer-next]').click();await wait(ready,'Scrolled next');}
  await click('Fermer ×');await wait(()=>!document.querySelector('.image-dialog'),'Return to stream');await new Promise(r=>setTimeout(r,100));assert(Math.abs(viewport.scrollTop-top)<1,'Exact scroll not restored');
  report.scroll={before:top,after:viewport.scrollTop};report.checks.push('filtered descending order, boundaries, next/previous, keyboard, editable arrows preserved, page/stream context, exact pixel scroll, restart filters');
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e)}await invoke('finish_smoke',{report});
})();
