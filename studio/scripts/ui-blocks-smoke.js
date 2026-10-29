(async () => {
  const report={ok:false,phase:'ui',checks:[],errors:[]};window.__SMOKE_STAGE=report;
  const invoke=window.__TAURI_INTERNALS__.invoke, assert=(v,m)=>{if(!v)throw Error(m)};
  const wait=async(p,m,t=20000)=>{const end=Date.now()+t;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,200))}};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
  try{
    await nav('Génération');
    await wait(()=>document.querySelector('[data-nav="generation"][aria-current="page"]'),'Génération heading');
    const present={p1:!!document.querySelector('[data-up-p1]'),prompt:!!document.querySelector('[data-up-prompt-p1]'),anatomy:!!document.querySelector('[data-up-anatomy-p1]'),browse:!!document.querySelector('[data-up-browse-p1]'),action:!!document.querySelector('[data-up-action-text]'),drop:!!document.querySelector('[data-image-drop]'),pick:!!document.querySelector('[data-image-pick]'),mode:!!document.querySelector('[data-up-seed-mode]')};assert(Object.values(present).every(Boolean),'missing '+JSON.stringify(present));
    await new Promise(r=>setTimeout(r,1200));
    const restored=await invoke('studio',{request:{action:'image_inspect',path:'__SMOKE_REPO_ROOT__/crates/studio-core/tests/fixtures/restore.png'}});
    assert(restored.hasPrompt&&restored.hasWorkflow,'image metadata');assert(restored.fields.seeds[0].value==='42','seed metadata');
    await invoke('smoke_drop_image',{path:'__SMOKE_REPO_ROOT__/crates/studio-core/tests/fixtures/restore.png'});
    await wait(()=>document.querySelector('.pass-group [data-input]'),'PNG temporary fields');
    assert(document.querySelector('[data-image-submit]').disabled,'PNG launched without confirmation');
    assert(document.querySelector('[data-image-close]'),'PNG close missing');
    report.checks.push('PNG temporary preparation; no automatic submission');
    assert(document.querySelector('[data-up-lora]'),'LoRA manager');assert(document.querySelector('[data-saya-couple]'),'Saya review');
    document.querySelector('[data-up-lora]').open=true;
    assert(document.querySelector('[data-up-lora-load]'),'LoRA load');assert(document.querySelector('[data-up-lora-dir-pick]'),'LoRA folder picker');
    await nav('Galerie');await wait(()=>document.querySelector('[data-nav="gallery"][aria-current="page"]'),'Galerie heading');
    assert(document.querySelector('.filter-drawer'),'gallery filter drawer');
    await nav('LoRA');await wait(()=>document.querySelector('[data-nav="lora"][aria-current="page"]'),'LoRA heading');assert(document.querySelector('[data-lora-refresh]'),'LoRA manager');assert(document.querySelector('[data-lora-pick]'),'LoRA folder picker');report.checks.push('lora-page');
    await nav('Labo A/B');await wait(()=>document.querySelector('.subtabs button.on[data-nav="ablab"]'),'AB heading');
    await nav('Pipelines');await wait(()=>document.querySelector('.subtabs button.on[data-nav="pipelines"]'),'Pipelines heading');
    assert(document.querySelector('[data-pipeline-graph]'),'pipeline graph');
    await nav('Éditeur graphique');await wait(()=>document.querySelector('.subtabs button.on[data-nav="graph"]'),'graph heading');await wait(()=>{const b=document.querySelector('[data-graph-new]');return b&&!b.disabled},'new workflow');document.querySelector('[data-graph-new]').click();await wait(()=>document.querySelector('[data-graph-catalog-open]'),'catalog button');await wait(()=>!document.querySelector('[data-graph-catalog-open]').disabled,'catalog enabled');document.querySelector('[data-graph-catalog-open]').click();await wait(()=>document.querySelector('[data-node-catalog]'),'catalog');const cat=document.querySelector('[data-node-catalog]');assert(cat.getBoundingClientRect().width<=370,'catalog width');assert(document.querySelector('[data-node-search]'),'node search');await wait(()=>document.querySelector('.node-catalog-item'),'node result');document.querySelector('.node-catalog-item').click();await wait(()=>document.querySelectorAll('.graph-node').length>0,'node added');report.checks.push('node-added');cat.querySelector('.node-catalog-head button')?.click();
    report.capture=await invoke('smoke_capture',{name:'editor'});
    report.checks.push('generation','p1','lora','gallery','recipes','ab','pipelines','graph-catalog');
    assert(report.errors.length===0,report.errors.join(';'));report.ok=true;
  }catch(e){report.failure=String(e?.message||e)+' | '+String(e?.stack||'');report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent);}
  window.__SMOKE_STAGE=report;if(typeof invoke==='function')await invoke('finish_smoke',{report}).catch(()=>{});
})();
