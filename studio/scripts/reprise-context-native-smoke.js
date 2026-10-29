(async()=>{
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),pause=ms=>new Promise(r=>setTimeout(r,ms));
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(fn,m)=>{const end=Date.now()+30000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(100)}};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click();await pause(300)};
 const button=text=>[...document.querySelectorAll('button')].find(n=>n.textContent.trim()===text);
 const field=text=>[...document.querySelectorAll('.character-form label')].find(n=>n.textContent.startsWith(text))?.querySelector('input,textarea');
 const set=(n,v)=>{assert(n,'input');n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}))};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
 await wait(()=>innerWidth===1920&&innerHeight===1080,'fullscreen');let expected=window.__V12_SMOKE_PREVIOUS__?.expected;
 if(report.phase==='initial'){
 await nav('Galerie');await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled,'index enabled');button('Indexer / actualiser').click();
 await wait(async()=>!(await api('index_status')).running&&(await api('gallery',{filters:{}})).total===130,'index fixtures');
 const c=(await api('characters'))[0];expected={id:c.id,identity:c.identity.identity_prompt??'',draft:'Brouillon temporaire,  espaces exacts\nseconde ligne',anatomy:'anatomie distincte'};
 await nav('Personnages');await wait(()=>[...document.querySelectorAll('.character-list button')].find(n=>n.textContent.includes(c.name)),'character catalogue');[...document.querySelectorAll('.character-list button')].find(n=>n.textContent.includes(c.name)).click();
 await wait(()=>field('Nom')?.value===c.name,'selected sheet');set(field('Identité'),expected.draft);set(field('Anatomie'),expected.anatomy);
 await pause(300);await nav('Galerie');await nav('Personnages');
 }
 report.expected=expected;
 await nav('Personnages');await wait(()=>field('Identité')?.value===expected.draft,'character draft restored');assert(field('Anatomie').value===expected.anatomy,'anatomy context');
 assert((await api('character_sheet',{id:expected.id})).identityPrompt===expected.identity,'implicit save');
 report.checks.push('character context restored without save');
 await pause(1000);report.characterCapture=await invoke('smoke_capture',{name:'characters-'+report.phase});
 await nav('Galerie');await wait(()=>document.querySelector('[data-stream-asset]'),'gallery items');
 if(report.phase==='initial'){
 document.querySelector('[data-selection-mode]').click();await pause(100);const card=document.querySelector('[data-stream-asset]');expected.asset=card.getAttribute('data-stream-asset');card.click();await pause(200);await nav('Personnages');await nav('Galerie');
 }
 await wait(()=>document.querySelector('[data-selection-mode]')?.checked,'selection mode restored');await wait(()=>document.querySelector(`[data-stream-asset="${expected.asset}"]`)?.getAttribute('aria-pressed')==='true','selected asset restored');
 await pause(1000);report.galleryCapture=await invoke('smoke_capture',{name:'gallery-'+report.phase});report.checks.push('gallery selection restored');
 if(report.phase==='initial'){
 document.querySelector('[data-selection-mode]').click();await pause(100);
 const viewport=document.querySelector('.stream-viewport');viewport.scrollTop=600;await pause(300);const top=viewport.scrollTop;assert(top>400,'scroll fixture');
 const bounds=viewport.getBoundingClientRect();const card=[...document.querySelectorAll('[data-stream-asset]')].find(n=>{const r=n.getBoundingClientRect();return r.top>=bounds.top&&r.bottom<=bounds.bottom});assert(card,'visible scrolled image');card.click();
 await wait(()=>document.querySelector('[data-inspector-open]'),'inspector');document.querySelector('[data-inspector-open]').click();await wait(()=>document.querySelector('.image-dialog[open]'),'image viewer');button('Fermer ×').click();await wait(()=>!document.querySelector('.image-dialog'),'viewer closed');assert(Math.abs(viewport.scrollTop-top)<2,'viewer changed scroll');
 await nav('Personnages');await nav('Galerie');await wait(()=>document.querySelector('.stream-viewport')?.scrollTop>400,'navigation scroll restored');document.querySelector('[data-selection-mode]').click();await pause(300);
 report.checks.push('viewer close and page return restore scroll');
 }

 if(report.phase==='restart'){
 await nav('Personnages');await wait(()=>field('Identité')?.value===expected.draft,'draft before explicit save');button('Enregistrer la fiche').click();await wait(async()=>(await api('character_sheet',{id:expected.id})).identityPrompt===expected.draft,'explicit character save');report.checks.push('explicit save writes the character only when requested');
 }
 assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e)+' | '+String(e?.stack||'');report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
