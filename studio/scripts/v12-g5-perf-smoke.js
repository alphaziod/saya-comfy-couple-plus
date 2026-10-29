(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 // G5 smoke (brief point 7): before/after instrumentation of the paths
 // reported as slow — page change, view loading, IPC, SQL via
 // the real app (test Tauri window, isolated fixture, ComfyUI mock without
 // GPU). The __SAYA_IPC__ probe (api.ts) records each `call()` action:
 // count and duration.
 const report={ok:false,checks:[],timings:{},ipc:null};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,pause=ms=>new Promise(r=>setTimeout(r,ms));
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(f,m,ms=90000)=>{window.__SMOKE_STEP=m;const until=Date.now()+ms;while(!await f()){if(Date.now()>until)throw Error(m);await pause(25)}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const press=async text=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click();await pause(0)};
 const time=async(label,act,until)=>{const t=performance.now();if(act)act();if(until)await wait(until,'step '+label,60000);report.timings[label]=Math.round(performance.now()-t)};
 const pageOn=text=>{const id=NAV[text]??text;return !!document.querySelector(`[data-nav="${id}"][aria-current="page"],.subtabs button.on[data-nav="${id}"],[data-nav="${id}"].active`)};
 try{
  // IPC probe installed BEFORE the flow; the app works identically.
  window.__SAYA_IPC__={counts:{},ms:{}};
  const ui=String(window.__V15_UI_WORKFLOW__||'');
  await wait(()=>document.querySelector('[data-nav]'),'initial mount');
  await pause(300);
  // 1. Gallery: fixture indexing, then cards displayed.
  await nav('Galerie');
  await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled,'index button');
  const indexStart=performance.now();
  button('Indexer / actualiser').click();
  await wait(()=>{const count=document.querySelector('[data-gallery-count]');return count&&parseInt(count.textContent.replace(/\D/g,''))>0&&document.querySelector('.image-card')},'gallery indexed and cards rendered',240000);
  report.timings.gallery_index_cold=Math.round(performance.now()-indexStart);
  await pause(500);
  // 2. Pages view (pagination) to measure a page change.
  const streamToggle=document.querySelector('.stream-toggle input');
  if(streamToggle&&streamToggle.checked){streamToggle.click();await pause(300);}
  if(button('Suivantes →')){
    await time('gallery_page_next',()=>button('Suivantes →').click(),()=>button('← Précédentes')&&!button('← Précédentes').disabled&&document.querySelectorAll('.image-card').length>0);
    await time('gallery_page_prev',()=>button('← Précédentes').click(),()=>button('Suivantes →')&&!button('Suivantes →').disabled&&document.querySelectorAll('.image-card').length>0);
    report.checks.push('gallery paging measured in pages mode');
  }
  // 3. Open an image: detail + neighbours + thumbnails (viewer).
  const card=document.querySelector('.image-card');
  if(card){await time('gallery_open_image',()=>card.click(),()=>document.querySelector('[data-viewer-position]')&&document.querySelector('.image-dialog'));
    const next=()=>[...document.querySelectorAll('button')].find(b=>b.textContent.includes('Suivante →')&&!b.disabled);
    if(next()){await time('viewer_next',()=>next().click(),()=>{const b=[...document.querySelectorAll('button')].find(x=>x.textContent.includes('Suivante →'));return b&&b.disabled!==undefined&&document.querySelector('[data-viewer-position]')});await time('viewer_next2',()=>{const b=[...document.querySelectorAll('button')].find(x=>x.textContent.includes('Suivante →')&&!x.disabled);b&&b.click();},()=>true);}
    const close=button('Fermer ×');if(close){await time('viewer_close',()=>close.click(),()=>!document.querySelector('.image-dialog'));}
  }
  // 4. Page changes (main views) — unmount/remount + IPC.
  const menus=['Génération','Casting','Galerie','Personnages','LoRA','Scènes'];
  for(const menu of menus){
    await nav(menu);
    await time('menu_'+menu,()=>pageOn(menu));
    await pause(120);
  }
  // 5. Tools (heavier secondary views).
  for(const menu of ['Workflows','Modules','Pipelines','Éditeur graphique','ComfyUI','Données']){
    await nav(menu);await time('menu_'+menu,()=>pageOn(menu)||true);await pause(120);
  }
  // 6. Generation: opening a real workflow (parsing + preparation).
  if(ui){
    await nav('Génération');await pause(150);
    const input=document.querySelector('[data-saya-source]');
    if(input){input.value=ui;input.dispatchEvent(new Event('input',{bubbles:true}));
      await wait(()=>!document.querySelector('[data-workflow-open]')?.disabled,'open enabled');
      await time('generation_open_workflow',()=>document.querySelectorAll('[data-prompt-node]').length>0);
    }
  }
  // 7. Back to Gallery (hot view) then full IPC report.
  await nav('Galerie');await time('menu_Galerie_warm',()=>pageOn('Galerie'));await pause(250);
  report.ipc=JSON.parse(JSON.stringify(window.__SAYA_IPC__));
  report.checks.push('per-action IPC probe captured across index, paging, viewer, page changes, tools and workflow open');
  assert(!document.querySelector('.error'),'UI error');
  report.ok=true;
 }catch(e){report.failure=String(e);report.errors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
