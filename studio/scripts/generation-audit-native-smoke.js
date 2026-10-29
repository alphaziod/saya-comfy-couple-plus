// Read-only audit: open the workflow, visit every pass, dump each rendered control and capture it. No launch, no edit.
(async()=>{
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],passes:[]},invoke=window.__TAURI_INTERNALS__.invoke,pause=ms=>new Promise(r=>setTimeout(r,ms));
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(fn,m)=>{const end=Date.now()+45000;while(!await fn()){const errors=[...document.querySelectorAll('.error')].map(n=>n.textContent).filter(Boolean);if(errors.length)throw Error(m+': '+errors.join('; '));if(Date.now()>end)throw Error(m);await pause(150)}};
 const button=async label=>{const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===label);assert(b,'button '+label);b.click();await pause(200)};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click();await pause(200)};
 const control=n=>n.tagName==='SELECT'?`select(${n.options.length})`:n.type==='checkbox'?'checkbox':n.tagName==='TEXTAREA'?'textarea':`input:${n.type}`;
 try{
  await nav('Génération');await wait(()=>document.querySelector('[data-generation-workspace]'),'workspace');
  const source=document.querySelector('[data-saya-source]');source.value=window.__V15_UI_WORKFLOW__;source.dispatchEvent(new Event('input',{bubbles:true}));
  await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'open enabled');document.querySelector('[data-workflow-open]').click();await wait(()=>document.querySelectorAll('[data-prompt-node]').length===7,'prompts '+document.querySelectorAll('[data-prompt-node]').length);
  await button('Nodes par passe');
  for(const label of ['Communs','Passe 1','Passe 2','Passe 3','Passe 4','Passe 5','Passe 6']){
   await button(label);await wait(()=>![...document.querySelectorAll('.pass-tabs button')].some(b=>b.disabled),'pass '+label);await pause(300);
   const sections=[...document.querySelectorAll('.pass-group')].map(s=>({title:s.querySelector(':scope>h2')?.textContent,cards:[...s.querySelectorAll('[data-node-card]')].map(c=>({node:c.dataset.nodeCard,title:c.querySelector('h2')?.textContent,fields:[...c.querySelectorAll('label.node-option')].map(l=>{const n=l.querySelector('input,select,textarea');return {name:l.querySelector('span')?.textContent,control:n?control(n):'none',value:n?(n.type==='checkbox'?n.checked:n.value):null}})}))}));
   const tag=label.toLowerCase().replace(' ','');let capture=null;try{capture=await invoke('smoke_capture',{name:'audit-'+tag})}catch(e){capture=String(e)}
   report.passes.push({label,sections,capture});
  }
  document.querySelector('[data-generation-loras]').open=true;await pause(300);
  report.loraTab=[...document.querySelectorAll('.lora-loaders [data-node-card]')].map(c=>({node:c.dataset.nodeCard,rows:c.querySelectorAll('[data-lora-index]').length,nameControl:c.querySelector('[data-lora-index] [aria-label="Nom LoRA"]')?control(c.querySelector('[data-lora-index] [aria-label="Nom LoRA"]')):null}));
  try{report.loraCapture=await invoke('smoke_capture',{name:'audit-lora'})}catch(e){report.loraCapture=String(e)}
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e)}
 await invoke('finish_smoke',{report});
})();
