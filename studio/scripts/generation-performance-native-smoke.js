(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],timings:{}};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,pause=ms=>new Promise(r=>setTimeout(r,ms));
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(f,m)=>{window.__SMOKE_STEP=m;const until=Date.now()+60000;while(!await f()){if(Date.now()>until)throw Error(m);await pause(25)}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const press=async text=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click();await pause(0)};
 try{
 await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');
 const input=document.querySelector('[data-saya-source]');input.value=window.__V15_UI_WORKFLOW__;input.dispatchEvent(new Event('input',{bubbles:true}));
 await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'open enabled');let t=performance.now();document.querySelector('[data-workflow-open]').click();await pause(50);await wait(()=>document.querySelectorAll('[data-prompt-node]').length>0&&!document.querySelector('[data-workflow-open]').disabled,'workflow open');report.timings.open=Math.round(performance.now()-t);
 await press('Nodes par passe');await wait(()=>!button('Passe 1')?.disabled,'common ready');
 for(let n=1;n<=6;n++){t=performance.now();await press('Passe '+n);await wait(()=>!button('Passe '+n).disabled&&document.querySelector('[data-pass-section] [data-node-card]'),'pass '+n);report.timings['pass'+n]=Math.round(performance.now()-t)}
 const before=(await invoke('smoke_actions')).filter(a=>a==='graph_scoped_widget').length;
 t=performance.now();await press('Passe 1');await wait(()=>!button('Passe 1').disabled,'cached pass');report.timings.cachedPass1=Math.round(performance.now()-t);
 const after=(await invoke('smoke_actions')).filter(a=>a==='graph_scoped_widget').length;assert(before===after,'cached pass requested graph again');
 report.checks.push('six passes editable, revisiting pass performs zero scoped graph requests');
 for(const menu of ['Personnages','Galerie','Génération']){t=performance.now();await nav(menu);await wait(()=>document.querySelector(`[data-nav="${NAV[menu]}"][aria-current="page"]`),'menu '+menu);report.timings[menu]=Math.round(performance.now()-t)}
 assert(!document.querySelector('.error'),'UI error');report.ok=true;
 }catch(e){report.failure=String(e);report.errors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
