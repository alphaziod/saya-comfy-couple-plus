(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));
 const wait=async(fn)=>{const end=Date.now()+10000;while(!fn()){if(Date.now()>end)throw new Error('Condition native non atteinte: '+fn);await new Promise(r=>setTimeout(r,100));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const refs=()=>document.querySelector('section[aria-label="Références ordonnées"]');
 const candidate=asset=>refs().querySelector(`[data-reference-candidate="${asset}"]`);
 const row=asset=>refs().querySelector(`[data-reference-id="${asset}"]`);
 const rowButton=(asset,text)=>[...row(asset).querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 try{
  // Idempotent fixture preparation: links only, before the UI is opened.
  for(const r of await api('references',{character:'alpha'}))await api('reference_edit',{character:'alpha',variant:r.variantId,asset:r.assetId,operation:'remove'});
  await nav('Personnages');await wait(()=>[...document.querySelectorAll('.character-list button')].some(b=>b.querySelector('strong')?.textContent==='Alpha'));
  [...document.querySelectorAll('.character-list button')].find(b=>b.querySelector('strong')?.textContent==='Alpha').click();await wait(()=>refs());
  refs().querySelector('details summary').click();await wait(()=>candidate('pair-job-0'));candidate('pair-job-0').click();await wait(()=>!button('Ajouter à ces références').disabled);button('Ajouter à ces références').click();await wait(()=>row('pair-job-0'));
  candidate('pair-job-1').click();await wait(()=>!button('Ajouter à ces références').disabled);button('Ajouter à ces références').click();await wait(()=>row('pair-job-1'));
  rowButton('pair-job-1','Définir principale').click();await wait(()=>row('pair-job-1').textContent.includes('Image principale'));
  row('pair-job-1').querySelector('[aria-label="Monter la référence"]').click();await wait(()=>refs().querySelector('[data-reference-id]')?.getAttribute('data-reference-id')==='pair-job-1');
  const scope=refs().querySelector('select');scope.value='alpha-variant';scope.dispatchEvent(new Event('change',{bubbles:true}));await wait(()=>!row('pair-job-0'));await wait(()=>candidate('pair-job-0'));candidate('pair-job-0').click();await wait(()=>!button('Ajouter à ces références').disabled);button('Ajouter à ces références').click();await wait(()=>row('pair-job-0'));
  rowButton('pair-job-0','Définir principale').click();await wait(()=>row('pair-job-0').textContent.includes('Image principale'));
  const persisted=await api('references',{character:'alpha'});if(persisted.length!==3||!persisted.some(r=>r.variantId==='alpha-variant'&&r.primary))throw new Error('Scope persistence mismatch');
  row('pair-job-0').querySelector('[aria-label="Ouvrir cette référence"]').click();await wait(()=>document.querySelector('dialog'));
  const image=await api('detail',{id:'pair-job-0'});if(image.generations[0].jobId!=='pair-job')throw new Error('Generation provenance lost');
  [...document.querySelectorAll('dialog .association button')].find(b=>b.textContent==='Alpha').click();await wait(()=>refs()&&document.querySelector('.character-form input')?.value==='Alpha');
  const historySummary=[...refs().querySelectorAll('summary')].find(e=>e.textContent==='Historique des références');historySummary.click();await wait(()=>refs().textContent.includes('Ordre modifié'));
  const audit=await api('reference_history',{character:'alpha',before:null});if(!audit.some(e=>e.operation==='order')||!audit.some(e=>e.operation==='primary'))throw new Error('Reference audit missing');
  const eventButton=[...historySummary.parentElement.querySelectorAll('button')].find(b=>b.textContent==='Voir les changements');eventButton.click();await wait(()=>historySummary.parentElement.querySelector('[aria-label="Ouvrir cette image historique"]'));
  report.checks.push({referenceAuditVisible:true});
  const exchangeSummary=[...document.querySelectorAll('summary')].find(e=>e.textContent==='Importer / exporter une fiche');exchangeSummary.click();
  button('Afficher le JSON à exporter').click();const exchange=exchangeSummary.parentElement.querySelector('textarea');await wait(()=>exchange.value.includes('"version": 2'));
  const exported=JSON.parse(exchange.value);if(exported.references.length!==3)throw new Error('Export refs lost');
  // Distinct name makes repeated isolated UI runs unambiguous.
  exported.name='Imported reference smoke '+Date.now();exchange.value=JSON.stringify(exported);exchange.dispatchEvent(new Event('input',{bubbles:true}));button('Importer comme nouvelle fiche').click();
  await wait(()=>document.querySelector('.character-form input')?.value===exported.name);
  const imported=(await api('characters',{include_archived:true})).find(c=>c.name===exported.name);
  const roundtrip=await api('export_character',{id:imported.id});
  const layout=doc=>doc.references.map(({hash,variantName,position,primary,historicalVariant})=>({hash,variantName,position,primary,historicalVariant}));
  if(JSON.stringify(layout(roundtrip))!==JSON.stringify(layout(exported)))throw new Error('Portable reference layout mismatch');
  if((await api('references',{character:'alpha'})).length!==3)throw new Error('Source refs altered');
  report.checks.push({portableExportImport:true,sourceUnchanged:true});
  report.checks.push({referencePrimary:true,manualOrder:true,variantScope:true,characterToImage:true,imageToCharacter:true,generationProvenancePreserved:true,submitted:false});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
