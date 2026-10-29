(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]},invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),pause=ms=>new Promise(r=>setTimeout(r,ms));window.__SMOKE_STAGE=report;
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+45000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const exactSeeds=async()=>{const workflows=await api('workflows'),fields=(await Promise.all(workflows.map(w=>api('workflow',{id:w.id})))).flatMap(w=>w.fields).filter(f=>['seed','noise_seed'].includes(f.inputName));return fields.length>0&&fields.every(f=>f.value==='102095739791283')};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}))};
 try{
 await wait(()=>document.querySelector('[data-nav]'),'nav');await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');
 if(report.phase==='initial'){
 set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);await press('[data-workflow-open]');await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
 const button=async label=>{await wait(()=>[...document.querySelectorAll('button')].some(b=>b.textContent.trim()===label&&!b.disabled),'button '+label);[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===label).click();await pause(100)};
 await button('Nodes par passe');
 for(let phase=1;phase<=6;phase++){
   await button('Passe '+phase);
   await wait(()=>document.querySelector('.pass-tabs .chosen')?.textContent==='Passe '+phase&&!document.querySelector('.pass-tabs .chosen').disabled,'loaded settings '+phase);
   assert(document.querySelectorAll('[data-pass-section] input,[data-pass-section] select').length>0,'missing settings phase '+phase);
   if(phase===3){const sampler=document.querySelector('[data-scope-node="100088"][data-input="steps"]');assert(sampler,'phase 3 sampler settings');sampler.value='7';sampler.dispatchEvent(new Event('change',{bubbles:true}));}
 }
 await press('[data-generation-save]');await wait(()=>!document.querySelector('[data-generation-save]').disabled,'preset saved');
 await button('Passe 3');await wait(()=>document.querySelector('[data-scope-node="100088"][data-input="steps"]')?.value==='7','phase 3 edit retained');
 const state=(await api('preferences')).generationWorkspace;const checked=await api('graph_prepare_api',{session:state.session});assert(checked.kind==='saya_preset'&&checked.phases.length===6&&checked.report.complete,'preset API verification');
 report.checks.push('all six passes have editable fields; phase 3 sampler edit saved; six API previews complete');
 await press('[data-generation-tools]');await wait(()=>document.querySelector('[data-saya-root]'),'advanced root');set('[data-saya-seed]','102095739791283');set('[data-saya-root]','mock-slow');await pause(100);await press('[data-saya-start]');
 for(let phase=1;phase<=6;phase++){
 await wait(()=>document.querySelector(`[data-saya-phase="${phase}"]`)?.getAttribute('data-state')==='succeeded','phase '+phase+' mock result');
 if(phase===1){await wait(()=>document.querySelector('[data-saya-review-popup]'),'phase 1 popup');await press('[data-review-continue]');}
 else assert(!document.querySelector('[data-saya-review-popup]'),'unexpected later popup '+phase);
 if(phase===2){
   await nav('Galerie');
   await wait(async()=> (await api('saya_runs'))[0]?.state==='completed','background chain while viewing gallery');
   await nav('Génération');
   await wait(()=>document.querySelector('[data-saya-phase="6"]')?.getAttribute('data-state')==='succeeded','recover background completion');
 }
 }
 const runs=await api('saya_runs');assert(runs.length===1&&runs[0].state==='completed','six phase completion');report.expected={run:runs[0].id};
 assert(await exactSeeds(),'phase seed drift');
 window.confirm=()=>true;await press('[data-saya-restart]');await wait(()=>document.querySelector('[data-saya-phase="1"]')?.getAttribute('data-state')==='queued','restart queued');
 await press('[data-saya-stop]');await wait(()=>document.querySelector('[data-saya-state]')?.textContent.includes('Arrêtée'),'stop recorded');
 assert(!document.querySelector('[data-saya-continue]'),'stopped run offers continue');
 assert((await api('saya_runs'))[0].masterSeed!=='102095739791283','restart must draw a new seed');
 report.expected.restartSeed=(await api('saya_runs'))[0].masterSeed;
 report.checks.push('one phase 1 popup; phases 2 through 6 finish automatically even on Gallery page','numeric seed fields exact; checkpoint seed links covered by API parity test','Restart after completed','Stop queued job; no Continue when stopped');
 }else{
 report.expected=window.__V12_SMOKE_PREVIOUS__.expected;await wait(()=>document.querySelector('[data-saya-runs]')?.value===report.expected.run,'recover stopped run');await wait(()=>document.querySelector('[data-saya-state]')?.textContent.includes('Arrêtée'),'stopped persisted');assert((await api('saya_runs'))[0].masterSeed===report.expected.restartSeed,'persisted restart seed');report.checks.push('stopped run recovery after process restart');
 }
 report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}await invoke('finish_smoke',{report});
})();
