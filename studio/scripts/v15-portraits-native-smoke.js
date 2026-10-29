(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,70));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const click=async(text)=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click()};
 const decoded=selector=>{const img=document.querySelector(selector);return img?.complete&&img.naturalWidth>0};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   await click('Indexer / actualiser');await wait(async()=>(await api('gallery',{filters:{}})).total===130&&!(await api('index_status')).running,'Index complete');
   const card=(await api('character_cards')).find(c=>c.name==='Alpha Test');
   const images=(await api('gallery',{filters:{character:card.id}})).items;
   report.expected={character:card.id,first:images[0].id,second:images[1].id};
   await api('choose_portrait',{character:card.id,asset:images[0].id});
   await nav('Galerie personnages');await wait(()=>decoded(`[data-character="${card.id}"] img`),'Present portrait decoded');
   report.checks.push('present image decoded through native IPC');
  }else{
   report.expected=window.__V12_SMOKE_PREVIOUS__.expected;const {character,first,second}=report.expected;
   await wait(()=>document.querySelector(`[data-character="${character}"]`),'Cards restored');
   const card=document.querySelector(`[data-character="${character}"]`);assert(card.textContent.includes('Portrait manquant'),'Missing reason absent on card');card.click();
   await wait(()=>document.querySelector('[data-portrait-reason]'),'Missing reason in sheet');
   assert((await api('character_sheet',{id:character})).portrait===first,'Silent portrait substitution');
   await click('Choisir un portrait…');await wait(()=>document.querySelector(`[data-portrait-option="${second}"]`),'Options');
   assert(document.querySelector(`[data-portrait-option="${first}"]`).disabled,'Missing file selectable');
   document.querySelector(`[data-portrait-option="${second}"]`).click();await wait(()=>decoded('.sheet-portrait img'),'Replacement decoded');
   const sheet=await api('character_sheet',{id:character});assert(sheet.portrait===second,'Explicit choice not saved');assert(sheet.references.some(r=>r.assetId===first),'Historical reference lost');
   await click('← Toutes les fiches');await wait(()=>decoded(`[data-character="${character}"] img`),'Updated card decoded');
   report.checks.push('missing physical file diagnosed, no automatic substitution, unavailable option blocked, explicit choice decoded, historical reference preserved');
  }
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e)}await invoke('finish_smoke',{report});
})();
