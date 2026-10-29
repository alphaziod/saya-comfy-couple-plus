(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const click=async(text)=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click()};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   await nav('Galerie personnages');await click('Importer des personnages');
   const input=document.querySelector('[data-grok-paths]');input.value=window.__V15_GROK_FILES__.join('\n');input.dispatchEvent(new Event('input',{bubbles:true}));
   await click('Analyser les fichiers');await wait(()=>document.querySelectorAll('[data-grok-entry]').length===2,'Two previews');
   assert(document.querySelector('.grok-dialog').textContent.includes('exact'),'Preview content absent');
   let boxes=document.querySelectorAll('[data-grok-entry]');boxes[0].checked=true;boxes[0].dispatchEvent(new Event('change',{bubbles:true}));
   await click('Confirmer l’import (1)');await wait(()=>document.querySelector('[data-grok-report]'),'First import');
   let cards=await api('character_cards');assert(cards.length===1,'Unexpected merge or extra character');const id=cards[0].id;report.expected={id};
   await click('Analyser les fichiers');await wait(()=>!document.querySelector('[data-grok-analyze]').disabled&&document.querySelector('[data-grok-entry]')?.disabled,'Duplicate preview');
   boxes=document.querySelectorAll('[data-grok-entry]');boxes[1].checked=true;boxes[1].dispatchEvent(new Event('change',{bubbles:true}));
   const target=document.querySelectorAll('[data-grok-target]')[1];target.value=id;target.dispatchEvent(new Event('change',{bubbles:true}));
   await click('Confirmer l’import (1)');await wait(()=>document.querySelector('[data-grok-report]'),'Variant import');
   assert((await api('character_cards')).length===1,'Variant created another character');assert((await api('variants',{id})).length===4,'Variants lost');
   const sheet=await api('character_sheet',{id});assert(sheet.identityPrompt===' exact  \nété 🌸','Identity overwritten or normalized');assert(sheet.anatomyPrompt===' anatomy ','Anatomy lost');
   report.checks.push('two files previewed, selected creation, duplicate detected, explicit existing-character variant, exact prompts and BG variants');
  }else{report.expected=window.__V12_SMOKE_PREVIOUS__.expected;assert((await api('variants',{id:report.expected.id})).length===4,'Restart lost imported variants');assert((await api('character_cards')).length===1,'Restart lost character');report.checks.push('character and four variants persist after restart');}
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e)}await invoke('finish_smoke',{report});
})();
