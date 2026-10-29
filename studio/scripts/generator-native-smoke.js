(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));
 const wait=async(fn)=>{const end=Date.now()+10000;while(!fn()){if(Date.now()>end)throw new Error('Condition native non atteinte: '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const set=(el,value,type='input')=>{el.value=value;el.dispatchEvent(new Event(type,{bubbles:true}));};
 const required=key=>document.querySelector(`[data-generator-attribute="${key}"] select`);
 const current=key=>document.querySelector(`[data-generator-result="${key}"] td:nth-last-child(2)`)?.textContent;
 const seed=()=>document.querySelector('[data-generator-seed]');
 try{
  const initial=await api('characters',{include_archived:true}),initialIds=new Set(initial.map(c=>c.id));const old=initial.find(c=>c.name==='Generator native fixture');
  if(old){if(old.identity.generator.version!==2||old.identity.generator.seed!=='42')throw new Error('Generator restart persistence failed');report.checks.push({restartPersistence:true});}
  await nav('Personnages');await wait(()=>document.querySelector('.generator summary'));document.querySelector('.generator summary').click();await wait(()=>required('world'));
  set(required('world'),'','change');set(required('species'),'elf','change');set(required('role'),'scholar','change');
  const denied=document.querySelector('[data-generator-attribute="hairColor"] select[multiple]');for(const option of denied.options)option.selected=option.value==='black';denied.dispatchEvent(new Event('change',{bubbles:true}));
  set(seed(),'18446744073709551615');button('Calculer la proposition').click();await wait(()=>document.querySelector('[data-generator-used-seed]')?.textContent.includes('18446744073709551615'));
  if(current('world')!=='fantasy'||current('species')!=='elf')throw new Error('Joint constraints failed');
  const hair=current('hairColor');if(hair==='black')throw new Error('Forbidden value selected');document.querySelector('[data-generator-attribute="hairColor"] input[type="checkbox"]').click();set(seed(),'42');
  await wait(()=>button('Utiliser dans une nouvelle fiche').disabled);
  button('Calculer la proposition').click();await wait(()=>document.querySelector('[data-generator-used-seed]')?.textContent==='Seed utilisée : 42');
  if(current('hairColor')!==hair||!document.querySelector('table').textContent.includes('Résultat précédent'))throw new Error('Lock/compare failed');
  set(required('world'),'modern','change');button('Calculer la proposition').click();await wait(()=>document.querySelector('.generator [role="alert"]'));
  if(!button('Utiliser dans une nouvelle fiche').disabled)throw new Error('Impossible proposal remains applicable');
  set(required('world'),'fantasy','change');button('Calculer la proposition').click();await wait(()=>!button('Utiliser dans une nouvelle fiche').disabled);
  const prompt=document.querySelector('[data-generator-prompt]').textContent;
  button('Utiliser dans une nouvelle fiche').click();await wait(()=>document.querySelector('.character-form textarea')?.value.includes('scholar'));
  set(document.querySelector('.character-form input'),'Generator native fixture');button('Enregistrer la fiche').click();await wait(()=>document.querySelector('[role="status"]')?.textContent.includes('Fiche enregistrée'));
  const records=await api('characters',{include_archived:true}),saved=records.filter(c=>c.name==='Generator native fixture').find(c=>!initialIds.has(c.id)&&c.identity.identity_prompt===prompt&&c.identity.generator.seed==='42');
  if(!saved||saved.identity.generator.locked.hairColor!==hair)throw new Error('Saved generator mismatch');
  const sourceIdentity=JSON.stringify(saved.identity);await wait(()=>!document.querySelector('[data-generator-variant-mode]').disabled);document.querySelector('[data-generator-variant-mode]').click();
  set(required('role'),'guardian','change');button('Calculer la proposition').click();await wait(()=>document.querySelector('[data-generator-base-prompt]')?.textContent===prompt&&document.querySelector('[data-generator-prompt]').textContent.includes('plate armor'));
  if(!document.querySelector('[data-generator-attribute="species"] input[type="checkbox"]').disabled||current('hairColor')!==hair)throw new Error('Permanent identity is not locked');
  set(document.querySelector('[data-generated-variant-name]'),'Armure native');await wait(()=>!button('Confirmer la création de cette variante').disabled);button('Confirmer la création de cette variante').click();await wait(()=>document.querySelector('.generator [role="status"]')?.textContent.includes('Variante enregistrée'));
  const variant=(await api('variants',{id:saved.id})).find(v=>v.name==='Armure native');const after=(await api('characters',{include_archived:true})).find(c=>c.id===saved.id);
  if(!variant||JSON.stringify(after.identity)!==sourceIdentity||variant.constraints.basePrompt!==prompt||!variant.promptBlocks.P1.includes('plate armor'))throw new Error('Saved variant or source mismatch');
  report.checks.push({confirmedVariantSaved:true,permanentTraitsLocked:true,sourceIdentityUnchanged:true});
  if(document.documentElement.scrollWidth>window.innerWidth+1)throw new Error('Horizontal overflow');
  report.checks.push({u64Seed:true,jointConstraints:true,forbiddenRespected:true,lockPreserved:true,comparison:true,impossibleRejected:true,staleApplyBlocked:true,formSaved:true,viewport:[window.innerWidth,window.innerHeight],submitted:false});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
