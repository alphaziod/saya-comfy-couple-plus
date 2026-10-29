(async () => {
  const report={ok:false,phase:'gpu-ui',checks:[],errors:[]};window.__SMOKE_STAGE=report;
  const invoke=window.__TAURI_INTERNALS__.invoke, assert=(v,m)=>{if(!v)throw Error(m)};
  const wait=async(p,m,t=360000)=>{const end=Date.now()+t;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,500))}};
  const phase=n=>document.querySelector(`[data-saya-phase="${n}"]`);
  window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
  try{
    await wait(()=>document.querySelector('[data-nav]'),'Génération nav',20000);
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
await nav('Génération');
    await wait(()=>document.querySelector('[data-saya-couple]'),'Saya panel');
    const source=document.querySelector('[data-saya-source]');source.value='/tmp/ILUSTCOUPLECLEAN-Q5KM.json';source.dispatchEvent(new Event('input',{bubbles:true}));
    const seed=document.querySelector('[data-saya-seed]');seed.value='424242';seed.dispatchEvent(new Event('input',{bubbles:true}));
    document.querySelector('[data-saya-start]').click();
    await wait(()=>phase(1)?.textContent.includes('succeeded'),'phase 1 review',420000);
    report.checks.push('phase1');await wait(()=>document.querySelector('[data-saya-phase="1"] img'),'phase 1 image',20000);
    document.querySelector('[data-saya-phase="1"] [data-saya-continue]').click();
    await wait(()=>phase(2)?.textContent.includes('succeeded'),'phase 2',420000);
    report.checks.push('phase2');await wait(()=>document.querySelector('[data-saya-phase="2"] img'),'phase 2 image',20000);
    document.querySelector('[data-saya-phase="2"] [data-saya-continue]').click();
    await wait(()=>phase(3)?.textContent.includes('succeeded'),'phase 3',600000);
    report.checks.push('phase3');await wait(()=>document.querySelector('[data-saya-phase="3"] img'),'phase 3 image',20000);
    const imgs=[...document.querySelectorAll('[data-saya-phase] img')];assert(imgs.length>=3,'three phase images');
    report.images=imgs.map(i=>i.getAttribute('src')?.slice(0,32));
    assert(document.querySelector('[data-saya-state]')?.textContent.includes('completed'),'run not completed');
    assert(report.errors.length===0,report.errors.join(';'));report.ok=true;
  }catch(e){report.failure=String(e?.stack||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent);}
  window.__SMOKE_STAGE=report;if(typeof invoke==='function')await invoke('finish_smoke',{report}).catch(()=>{});
})();
