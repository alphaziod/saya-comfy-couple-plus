(async () => {
  // Requires --mock-comfy --mock-accept --restart. Two-sampler job through a
  // one-pass pipeline, comparator opened from the pipeline step results.
  // Re-anchored on 2026-10-25 (UI decision, PASSATION_ZCODE_2026-10-25.md §6):
  // the "[data-sampler-open]" button of the gallery detail panel is
  // removed. The comparator now lives in the pass results
  // (Pipelines.svelte: [data-run-compare] → SamplerComparators) and in the
  // prepared drafts (PreparedDraft.svelte). The graph keeps two chained
  // samplers; each branch ends with a SaveImage because the mock only
  // materialises SaveImage outputs (mock-comfy.py) and comparators.rs
  // attaches each output to its upstream sampler — V1 receives f14-v1, V2 receives
  // f14-native. The restart phase reopens the comparator of the SAME job via
  // the persisted run.
  const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};window.__SMOKE_STAGE=report;
  const invoke=window.__TAURI_INTERNALS__.invoke, api=(a,args={})=>invoke('studio',{request:{action:a,...args}}), assert=(v,m)=>{if(!v)throw Error(m)};
  const wait=async(p,m,t=180000)=>{window.__SMOKE_STEP=m+'@'+Date.now();const end=Date.now()+t;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,200));}};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const click=async text=>{const b=()=>[...document.querySelectorAll('button')].find(x=>x.textContent.trim()===text);await wait(()=>b()&&!b().disabled,'Button unavailable: '+text);b().click();};
  window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
  try {
    const restart=window.__V12_SMOKE_PHASE__==='restart';
    let pipeline,run,job;
    if(!restart){
      const graph={"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},"2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"red apple"}},"3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"blurry"}},"4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},"5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":1,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},"6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},"7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"f14-v1"}},"8":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["5",0],"seed":2,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":0.5}},"9":{"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["1",2]}},"10":{"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"f14-native"}}};
      const workflow=await api('import_workflow',{name:'F14 native',text:JSON.stringify(graph)});
      await nav('Pipelines');
      await wait(()=>document.querySelector('[data-pipelines]'),'Pipelines page',15000);
      const nameInput=document.querySelector('[data-pipeline-builder] label input');assert(nameInput,'pipeline name field');nameInput.value='F14 comparateurs';nameInput.dispatchEvent(new Event('input',{bubbles:true}));
      const stepSelect=document.querySelector('[data-pipeline-step="0"] select');assert(stepSelect,'pipeline step workflow select');
      await wait(()=>[...stepSelect.options].some(o=>o.value===workflow),'workflow option missing');
      stepSelect.value=workflow;stepSelect.dispatchEvent(new Event('change',{bubbles:true}));
      await wait(()=>!document.querySelector('[data-pipeline-save]').disabled,'pipeline save disabled');
      document.querySelector('[data-pipeline-save]').click();
      await wait(async()=>(await api('pipelines')).length===1,'pipeline not saved');
      pipeline=(await api('pipelines'))[0].id;
      await click('Nouvelle exécution');
      await wait(()=>document.querySelector('[data-run-next]'),'run not opened');
      run=(await api('pipeline_runs',{pipeline}))[0].run;
      document.querySelector('[data-run-confirm]').click();
      await wait(()=>!document.querySelector('[data-run-next]').disabled,'confirmation not applied');
      document.querySelector('[data-run-next]').click();
      await wait(async()=>(await api('pipeline_run',{run})).steps[0].state==='completed','real two-sampler pass');
      job=(await api('pipeline_run',{run})).steps[0].job;assert(job,'pass produced no job');
    } else {
      pipeline=window.__V12_SMOKE_PREVIOUS__.pipeline;run=window.__V12_SMOKE_PREVIOUS__.run;job=window.__V12_SMOKE_PREVIOUS__.job;
      assert(pipeline&&run&&job,'previous phase lost pipeline/run/job');
      const detail=await api('pipeline_run',{run});
      assert(detail.steps[0].job===job&&detail.steps[0].state==='completed','restart lost the pass result');
    }
    report.pipeline=pipeline;report.run=run;report.job=job;
    // Gallery: the job outputs land in the catalog and open in the viewer;
    // the compact metadata line ties the image back to its job.
    const asset=(await api('gallery',{filters:{job:report.job}})).items[0];assert(asset,'job output asset missing');
    await wait(()=>document.querySelector('[data-nav]'),'Galerie nav');
    await nav('Galerie');
    await wait(()=>document.querySelector('[data-nav="gallery"][aria-current="page"]'),'page galerie');
    const searchInput=document.querySelector('input[placeholder="Rechercher…"]');assert(searchInput,'search');searchInput.value=asset.path.split(/[\\/]/).pop();searchInput.dispatchEvent(new Event('input',{bubbles:true}));
    // The card label drops the extension (StreamGrid.svelte/Gallery.svelte): compare without '.png'.
    // The stream may empty during a re-fetch: search, click and
    // wait for the detail in the same loop (retry if a render wipes the card).
    const label=asset.path.split(/[\\/]/).pop().replace(/\.(png|jpe?g|webp)$/i,'');
    await wait(async()=>{
      const c=[...document.querySelectorAll('.image-card')].find(x=>x.textContent.includes(label));
      if(!c)return false;
      c.click();
      return !!document.querySelector('[data-generation-metadata] [data-generation-mode]');
    },'asset card opens detail');
    await wait(()=>document.querySelector('[data-generation-metadata] [data-generation-mode]'),'gallery detail metadata',15000);
    const modeLine=document.querySelector('[data-generation-mode]').textContent.replace(/\s+/g,' ');
    assert(modeLine.includes('job Saya Studio'),'Detail line does not tie the image to its job: '+modeLine);
    // V1: the detail is in the inspector, no viewer to close.
    // Comparator: reopened for the job from the pipeline step results.
    await nav('Pipelines');
    await wait(()=>document.querySelector('[data-pipelines]'),'Pipelines page',15000);
    await wait(()=>document.querySelector(`[data-pipeline-item="${pipeline}"]`),'saved pipeline missing',15000);
    document.querySelector(`[data-pipeline-item="${pipeline}"]`).click();
    await wait(()=>document.querySelector(`[data-pipeline-run="${run}"]`),'saved run missing',15000);
    document.querySelector(`[data-pipeline-run="${run}"]`).click();
    await wait(()=>document.querySelector(`[data-run-compare="${report.job}"]`),'comparator entry in step results',15000);
    document.querySelector(`[data-run-compare="${report.job}"]`).click();
    await wait(()=>document.querySelector(`[data-sampler-comparators="${report.job}"]`),'integrated comparator',15000);
    await wait(()=>document.querySelectorAll('[data-sampler-step]').length>=2,'two sampler steps');
    const steps=[...document.querySelectorAll('[data-sampler-step]')];assert(steps.length===2,'expected exactly two steps');
    await wait(()=>document.querySelector('[data-sampler-single]'),'V1 single view');
    assert(!document.querySelector('[data-sampler-compare]'),'V1 must not be a comparator');
    steps[1].click();
    await wait(()=>document.querySelector('[data-sampler-compare]'),'V2 comparator');
    await wait(()=>document.querySelectorAll('[data-sampler-compare] img').length===2,'two images required',15000);
    const original=document.querySelector('[data-sampler-orientation]');assert(original,'orientation toggle');
    assert(!document.querySelector('[data-sampler-compare]').classList.contains('sampler-horizontal'),'default vertical');
    original.click();
    await wait(()=>document.querySelector('[data-sampler-compare]')?.classList.contains('sampler-horizontal'),'horizontal toggle',5000);
    await wait(()=>document.querySelectorAll('[data-sampler-compare] img').length===2,'images lost after toggle',8000);
    const zoomBefore=document.querySelector('[data-sampler-compare] img').getAttribute('style');
    document.querySelector('[data-sampler-zoom-in]')?.click();
    await wait(()=>document.querySelector('[data-sampler-compare] img').getAttribute('style')!==zoomBefore,'zoom not applied',5000);
    original.click();
    await wait(()=>!document.querySelector('[data-sampler-compare]')?.classList.contains('sampler-horizontal'),'return vertical',5000);
    await wait(()=>document.querySelectorAll('[data-sampler-compare] img').length===2,'images lost on return');
    report.checks.push({pipeline:report.pipeline,run:report.run,job:report.job,steps:steps.length,vertical:true,horizontal:true,imagesPreserved:true,zoom:true,restart:window.__V12_SMOKE_PHASE__==='restart'});
    assert(report.errors.length===0,report.errors.join(';'));report.ok=true;
  } catch(e){report.failure=String(e?.message||e)+(e?.stack?'\n'+e.stack:'');report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent);report.h1=document.querySelector('h1')?.textContent;report.stream=document.querySelector('[data-stream-status]')?.outerHTML.slice(0,300)??null;report.cards=document.querySelectorAll('.image-card').length;report.step=window.__SMOKE_STEP;}
  window.__SMOKE_STAGE=report;if(typeof invoke==='function')await invoke('finish_smoke',{report}).catch(()=>{});
})();
