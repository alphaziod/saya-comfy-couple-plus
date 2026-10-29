(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(f,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+60000;while(!await f()){const err=[...document.querySelectorAll('.error')].map(n=>n.textContent).filter(Boolean);if(err.length)throw Error(m+': '+err.join(';'));if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,100))}};
 const button=t=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===t);
 try{
 await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
 if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
 await nav('Génération');await wait(()=>document.querySelector('[data-generation-workspace]'),'workspace');
 const before=JSON.stringify(await api('graph_sessions')),workflows=JSON.stringify(await api('workflows'));
 await invoke('smoke_drop_image',{path:window.__V15_UI_WORKFLOW__});await wait(()=>document.querySelector('[data-temporary-workflow]'),'drop');
 const session=document.querySelector('[data-temporary-workflow]').dataset.temporaryWorkflow;
 const view=await api('graph_view',{session});const source=JSON.parse(view.sourceDetails),enabled=w=>(w.nodes??[]).every(n=>n.mode===0)&&(w.definitions?.subgraphs??[]).every(enabled)&&(w.extra?.saya_preset?.phases??[]).every(enabled);assert(enabled(source),'muted imported node');
 assert(document.querySelectorAll('[data-saya-couple]').length===1&&!document.querySelector('[data-image-submit]'),'separate rendering UI');
 button('Nodes par passe').click();
 for(let phase=1;phase<=6;phase++){
  await wait(()=>button('Passe '+phase)&&!button('Passe '+phase).disabled,'pass button');button('Passe '+phase).click();
  await wait(()=>!button('Passe '+phase).disabled&&document.querySelector('.pass-tabs .chosen')?.textContent==='Passe '+phase,'settings pass '+phase);
  assert(document.querySelectorAll('[data-pass-section] input,[data-pass-section] select').length>0,'settings missing '+phase);
 }
 const seed=document.querySelector('[data-saya-seed]');seed.value='102095739791283';seed.dispatchEvent(new Event('input',{bubbles:true}));
 await wait(()=>!document.querySelector('[data-saya-start]').disabled,'launch');document.querySelector('[data-saya-start]').click();
 await wait(()=>document.querySelector('[data-saya-review-popup]'),'same phase 1 review');
 const runs=await api('saya_runs');assert(runs.length===1,'run not using existing Saya engine');assert(runs[0].sourcePath===session,'temporary source exported to file');assert(runs[0].masterSeed==='102095739791283','seed not passed');
 await wait(()=>!document.querySelector('[data-review-continue]').disabled,'phase 1 validation ready');
 document.querySelector('[data-review-continue]').click();
 await wait(()=>document.querySelector('[data-saya-phase="6"]')?.dataset.state==='succeeded','six passes finish');
 assert(!document.querySelector('[data-saya-review-popup]'),'extra review popup');
 [...document.querySelectorAll('.surface-tabs button')].find(b=>b.textContent.startsWith('Prompts')).click();
 await wait(()=>document.querySelectorAll('[data-pass-comparison]').length===5,'five phase comparisons');
 const stage=document.querySelector('[data-pass-comparison]'),slider=document.querySelector('[data-pass-comparison-slider]');
 await wait(()=>stage.querySelectorAll('img').length===2,'two overlaid images');
 const rects=[...stage.querySelectorAll('img')].map(n=>n.getBoundingClientRect());
 assert(Math.abs(rects[0].x-rects[1].x)<1&&Math.abs(rects[0].width-rects[1].width)<1,'images must be overlaid at same width');
 const box=stage.getBoundingClientRect();
 assert(Math.abs(box.width-box.height)<2&&box.width<550,'compact square comparison');
 for(const rect of rects)assert(Math.abs(rect.y-box.y)<2&&Math.abs(rect.height-box.height)<2,'image fits comparison bounds');
 const cards=[...document.querySelectorAll('[data-pass-comparison]')].map(n=>n.getBoundingClientRect());
 assert(cards.filter(r=>Math.abs(r.y-cards[0].y)<2).length>=3,'at least three comparisons per row');
 for(const value of ['0','100','50']){slider.value=value;slider.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(r=>setTimeout(r,40));assert(stage.querySelector('.divider').style.left===value+'%','separator '+value);}
 report.checks.push('five overlaid comparisons; separator works at 0, 50, 100 percent');

 document.querySelector('[data-saya-phase="6"] button').click();await wait(()=>document.querySelector('img[alt="Résultat de la phase 6"]'),'same result renderer');
 assert(JSON.stringify(await api('graph_sessions'))===before,'temporary editor persisted');assert(JSON.stringify(await api('workflows'))===workflows,'import polluted saved workflows');
 document.querySelector('[data-image-close]').click();await wait(()=>!document.querySelector('[data-temporary-workflow]'),'close');
 let closed=false;try{await api('graph_view',{session})}catch{closed=true}assert(closed,'temporary editor still present');
 report.checks.push('import activates all nodes and exposes all six passes','same launch, phase 1 review, automatic phases 2–6 and result renderer','exact seed and temporary editor removed on close');report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
