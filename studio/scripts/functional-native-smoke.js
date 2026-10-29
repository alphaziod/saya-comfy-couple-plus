(async () => {
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const invoke=window.__TAURI_INTERNALS__.invoke;
  const api=(action,args={})=>invoke('studio',{request:{action,...args}});
  const assert=(ok,text)=>{if(!ok)throw new Error(text);};
  const wait=async fn=>{const start=Date.now();while(!fn()){if(Date.now()-start>30000)throw new Error('Timeout '+fn);await new Promise(r=>setTimeout(r,100));}};
  const button=text=>{const b=[...document.querySelectorAll('button')].find(n=>n.textContent.trim()===text);assert(b,'Missing button '+text);return b;};
  const fill=(prefix,value)=>{const label=[...document.querySelectorAll('label')].find(n=>n.textContent.trim().startsWith(prefix));assert(label,'Missing label '+prefix);const input=label.querySelector('input,textarea,select');input.value=value;input.dispatchEvent(new Event(input.tagName==='SELECT'?'change':'input',{bubbles:true}));};
  const report={ok:false,checks:[],errors:[]};
  window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
  try {
    await wait(()=>document.querySelector('nav'));
    await nav('Personnages');await wait(()=>document.querySelector('[data-nav="characters"][aria-current="page"]'));await new Promise(r=>setTimeout(r,400));
    fill('Nom','Native fixture');fill('Identité / prompt complet','adult, silver hair, exact  spacing');button('Enregistrer la fiche').click();
    await wait(()=>document.querySelector('.notice')?.textContent.includes('Fiche enregistrée'));
    const characters=await api('characters');assert(characters.some(c=>c.name==='Native fixture'),'Character persistence');report.checks.push('character_created_from_native_form');
    await nav('Prompts');await wait(()=>document.querySelector('.subtabs button.on[data-nav="assistant"]'));await new Promise(r=>setTimeout(r,400));
    fill('Dossier Saya Prompt Engine','/tmp/saya-prompt-engine-fixture');
    fill('Prompt complet exact','P1: adult, silver hair\nBG: forest');
    fill('LoRA documentées',JSON.stringify([{name:'fixture',documented_triggers:[{tag:'silver ribbon',source:'test documentation'}]}]));
    await wait(()=>!button('Proposer une modification').disabled); button('Proposer une modification').click();await wait(()=>document.querySelector('h2')?.textContent==='Comparer avant application' || document.querySelector('.error')); assert(!document.querySelector('.error'),document.querySelector('.error')?.textContent);
    const accept=button('Accepter et appliquer dans cet éditeur');assert(accept.disabled,'Human confirmation missing');
    const checkbox=document.querySelector('input[type=checkbox]');checkbox.click();await wait(()=>!accept.disabled);accept.click();
    await wait(()=>[...document.querySelectorAll('label')].find(n=>n.textContent.startsWith('Prompt complet exact'))?.querySelector('textarea')?.value.includes('silver ribbon'));
    assert((await api('proposals'))[0].decision==='accept','Decision not persisted');report.checks.push('local_proposal_diff_confirmation_apply_history');
    const state=await api('connection');assert(state.connected,'Live ComfyUI unavailable');
    const models=await api('models');assert(models.checkpoints.length>0,'No checkpoint schema choices');
    report.checks.push({engineVersion:state.version,checkpoints:models.checkpoints.length,nodeClasses:models.nodeClasses});
    const node=(class_type,inputs)=>({class_type,inputs});
    const graph={model:node('CheckpointLoaderSimple',{ckpt_name:models.checkpoints[0]}),latent:node('EmptyLatentImage',{width:512,height:512,batch_size:1}),mask:node('SayaSplitMask',{direction:'vertical',split:50,blur:0,swap:false,latent:['latent',0]}),
      p1:node('CLIPTextEncode',{text:'old identity one',clip:['model',1]}),p2:node('CLIPTextEncode',{text:'old identity two',clip:['model',1]}),action:node('CLIPTextEncode',{text:'old action',clip:['model',1]}),bg:node('CLIPTextEncode',{text:'old background',clip:['model',1]}),neg:node('CLIPTextEncode',{text:'blur',clip:['model',1]}),
      zone1:node('ConditioningSetMask',{conditioning:['p1',0],mask:['mask',0],strength:1,set_cond_area:'default'}),zone2:node('ConditioningSetMask',{conditioning:['p2',0],mask:['mask',1],strength:1,set_cond_area:'default'}),
      people:node('ConditioningCombine',{conditioning_1:['zone1',0],conditioning_2:['zone2',0]}),scene:node('ConditioningCombine',{conditioning_1:['action',0],conditioning_2:['bg',0]}),positive:node('ConditioningCombine',{conditioning_1:['people',0],conditioning_2:['scene',0]}),
      sample:node('KSampler',{model:['model',0],positive:['positive',0],negative:['neg',0],latent_image:['latent',0],seed:1,steps:1,cfg:5,sampler_name:'euler',scheduler:'normal',denoise:1}),decode:node('VAEDecode',{samples:['sample',0],vae:['model',2]}),out:node('SaveImage',{images:['decode',0],filename_prefix:'SMOKE_NOT_SUBMITTED'})};
    const id=await api('import_workflow',{name:'Native Couple schema fixture',text:JSON.stringify(graph)});
    const fields=(await api('workflow',{id})).fields.filter(f=>['p1','p2','action','bg','mask'].includes(f.nodeId)).map(f=>({...f,section:{p1:'P1',p2:'P2',action:'ACTION',bg:'BG'}[f.nodeId]??null}));
    await api('save_bindings',{id,fields});
    const p1=characters.find(c=>c.name==='Native fixture').id;
    const p2=await api('save_character',{id:null,name:'Native P2 fixture',identity:{identity_prompt:'adult, dark hair'}});
    const variant=await api('save_variant',{character:p2,id:null,name:'Native variant',blocks:{P1:'adult, dark hair'},constraints:{}});
    const prepared=await api('prepare_couple',{id,input:{characters:[{role:'P1',characterId:p1,variantId:null},{role:'P2',characterId:p2,variantId:variant}],p1:'adult, silver hair',p2:'adult, dark hair',action:'standing',bg:'forest',mask:{nodeId:'mask',direction:'horizontal',split:40,blur:1.5,swap:true}}});
    const request=JSON.parse(prepared.requestJson);
    assert(request.characters.length===2&&request.characters[0].characterId===p1&&request.characters[1].variant.id===variant,'Identity snapshot mismatch');
    for(const [nodeId,n] of Object.entries(graph))for(const [key,value] of Object.entries(n.inputs))if(Array.isArray(value))assert(JSON.stringify(request.prompt[nodeId].inputs[key])===JSON.stringify(value),'Link changed '+nodeId+'.'+key);
    assert((await api('workflow',{id})).fields.find(f=>f.nodeId==='p1'&&f.inputName==='text').value==='old identity one','Original changed');
    assert((await api('jobs')).every(j=>!j.promptId),'A job was unexpectedly submitted');
    report.checks.push({couplePrepared:true,identitiesAndVariantFrozen:true,nodes:prepared.nodeCount,linksPreserved:true,submitted:false});
    assert(report.errors.length===0,'WebView errors');report.ok=true;
  } catch(error) { report.failure=String(error)+'\n'+String(error?.stack||'');report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent); }
  window.__SMOKE_STAGE=report;
  await invoke('finish_smoke',{report});
})();
