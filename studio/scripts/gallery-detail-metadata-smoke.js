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
  await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled,'Ready to index');button('Indexer / actualiser').click();
  await wait(async()=>(await api('gallery',{filters:{}})).total===131&&!(await api('index_status')).running,'Index complete');
  check(document.querySelector('[data-stream-toggle]'),false);check(document.querySelector('[data-metadata-only]'),true);
  const search=document.querySelector('input[placeholder="Rechercher…"]');search.value='metadata.png';search.dispatchEvent(new Event('input',{bubbles:true}));
  await wait(()=>document.querySelectorAll('.image-grid .image-card').length===1,'Metadata-only result');
  document.querySelector('.image-grid .image-card').click();
  await wait(()=>document.querySelector('[data-prompts-box]'),'Detail panel rendered');
  // 1. Compact context line, no more titled "Génération" section.
  const mode=document.querySelector('[data-generation-mode]');
  assert(mode.textContent.includes('SOLO')&&mode.textContent.includes('2 × 2'),'Compact context line wrong: '+mode.textContent);
  assert(mode.className.includes('subtle'),'Context line not discreet');
  assert(![...document.querySelectorAll('h2')].some(h=>['Génération','Provenance','Fichier'].includes(h.textContent)),'Purged section title still displayed');
  // 2. Separate cards, one per prompt node: BG, ACTION, P1, P1 ANATOMIE on this solo pass.
  const box=document.querySelector('[data-prompts-box]');
  assert(!box.querySelector('textarea,input,[contenteditable]'),'Prompts boxes are editable');
  const labels={BG:'BG',ACTION:'Action',P1:'P1','P1 ANATOMY':'P1 anatomie'};
  for(const [role,label] of Object.entries(labels)){
    const cards=box.querySelectorAll(`[data-prompt-role="${role}"]`);
    assert(cards.length===1,`Role ${role}: ${cards.length} card(s) instead of 1 (encoder and node must merge)`);
    const card=cards[0];
    assert(card.querySelector('h4')?.textContent.trim()===`${label} :`,'Label missing on '+role+' card');
    assert(card.querySelectorAll('pre').length===1,'Role '+role+' card must hold exactly one readonly box');}
  assert(box.querySelectorAll('.prompt-entry').length===4,`Expected 4 cards, got ${box.querySelectorAll('.prompt-entry').length}`);
  assert(!box.querySelector('[data-prompt-role="QUALITY"],[data-prompt-role="P2"],[data-prompt-role="P2 ANATOMY"],[data-prompt-role="NEG"]'),'QUALITY, P2 or masked NEG card displayed');
  assert([...document.querySelectorAll('[data-prompt-role="P1"] pre')].some(p=>p.textContent==='young adult woman, (aged up:1.3), pink loose waves'),'P1 text altered');
  assert([...document.querySelectorAll('[data-prompt-role="P1 ANATOMY"] pre')].some(p=>p.textContent==='(small wings:1.25), cute_tail, long tail'),'P1 anatomy text altered');
  assert([...document.querySelectorAll('[data-prompt-role="BG"] pre')].some(p=>p.textContent==='candlelit library at night, brass lamps, rain on the tall windows'),'BG text altered');
  // 3. No technical suffix, no leftover of the removed sections.
  const boxText=box.textContent;
  for(const noise of ['· text','· value','Node ','relié à une sortie','X — vide','entrée source']) assert(!boxText.includes(noise),'Technical caption in prompts box: '+noise);
  const paneText=document.querySelector('.metadata-pane').textContent;
  for(const gone of ['Détails techniques','Métadonnées brutes exactes','Paramètres complets par node','Modèles, VAE et LoRA','Preuves du mode','Workflow embarqué','Graphe de génération','sections de prompts détectées']) assert(!paneText.includes(gone),'Purged section still displayed: '+gone);
  for(const gone of ['Ouvrir l’emplacement','Actualiser l’état du fichier','Gérer cette image']) assert(![...document.querySelectorAll('dialog button')].some(b=>b.textContent.includes(gone)),'Purged action still displayed: '+gone);
  // 4. What serves to qualify the image stays visible.
  for(const label of ['Ajouter à la comparaison','Trouver des images similaires','Retoucher (flou localisé)']) assert([...document.querySelectorAll('dialog button')].some(b=>b.textContent.includes(label)),'Action button missing: '+label);
  assert(document.querySelector('dialog textarea'),'Notes field missing');
  assert(document.body.textContent.includes('Personnages associés'),'Associated characters section missing');
  assert([...document.querySelectorAll('h2')].some(h=>h.textContent==='Annotations'),'Annotations section missing');
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e)}
 window.__SMOKE_STAGE=report;
 await invoke('finish_smoke',{report});
})();
