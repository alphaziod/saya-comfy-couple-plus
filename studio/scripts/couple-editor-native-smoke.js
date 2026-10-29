(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 const wait=async(fn)=>{const end=Date.now()+20000;while(!fn()){if(Date.now()>end)throw new Error('Condition native non atteinte: '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const set=(el,value,type='input')=>{if(!el)throw new Error('Control missing');el.value=value;el.dispatchEvent(new Event(type,{bubbles:true}));};

 const editor=()=>document.querySelector('[data-couple-editor]'),label=(text,root=editor())=>[...root.querySelectorAll('label')].find(l=>l.textContent.startsWith(text)),control=(text)=>label(text)?.querySelector('textarea,select,input');
 const person=i=>editor().querySelector(`[data-couple-person="${i}"]`),selectors=i=>person(i).querySelectorAll('select');
 const preset1='(silver hair:1.2),  exact\nfirst',preset2='dark hair,  exact\nsecond',background='(forest:1.1),  exact\nBG';
 try{
 const status=await api('connection');if(!status.connected)throw new Error('ComfyUI unavailable');const models=await api('models');
    const node=(class_type,inputs)=>({class_type,inputs});
    const graph={model:node('CheckpointLoaderSimple',{ckpt_name:models.checkpoints[0]}),latent:node('EmptyLatentImage',{width:512,height:512,batch_size:1}),mask:node('SayaSplitMask',{direction:'vertical',split:50,blur:0,swap:false,latent:['latent',0]}),
      p1:node('CLIPTextEncode',{text:'old identity one',clip:['model',1]}),p2:node('CLIPTextEncode',{text:'old identity two',clip:['model',1]}),action:node('CLIPTextEncode',{text:'old action',clip:['model',1]}),bg:node('CLIPTextEncode',{text:'old background',clip:['model',1]}),neg:node('CLIPTextEncode',{text:'blur',clip:['model',1]}),
      zone1:node('ConditioningSetMask',{conditioning:['p1',0],mask:['mask',0],strength:1,set_cond_area:'default'}),zone2:node('ConditioningSetMask',{conditioning:['p2',0],mask:['mask',1],strength:1,set_cond_area:'default'}),
      people:node('ConditioningCombine',{conditioning_1:['zone1',0],conditioning_2:['zone2',0]}),scene:node('ConditioningCombine',{conditioning_1:['action',0],conditioning_2:['bg',0]}),positive:node('ConditioningCombine',{conditioning_1:['people',0],conditioning_2:['scene',0]}),
      sample:node('KSampler',{model:['model',0],positive:['positive',0],negative:['neg',0],latent_image:['latent',0],seed:1,steps:1,cfg:5,sampler_name:'euler',scheduler:'normal',denoise:1}),decode:node('VAEDecode',{samples:['sample',0],vae:['model',2]}),out:node('SaveImage',{images:['decode',0],filename_prefix:'SMOKE_NOT_SUBMITTED'})};

 graph.sample.inputs.seed='MAX_SEED_LITERAL';
 const name='Couple editor '+Date.now(),id=await api('import_workflow',{name,text:JSON.stringify(graph).replace('"MAX_SEED_LITERAL"','18446744073709551615')});
 const fields=(await api('workflow',{id})).fields.filter(f=>['p1','p2','action','bg','neg','mask'].includes(f.nodeId)).map(f=>({...f,section:{p1:'P1',p2:'P2',action:'ACTION',bg:'BG',neg:'NEG'}[f.nodeId]??null}));await api('save_bindings',{id,fields});
 const chars=[],variants=[];for(let i=0;i<2;i++){chars.push(await api('save_character',{id:null,name:'Couple '+i,identity:{identity_prompt:'base '+i}}));variants.push(await api('save_variant',{character:chars[i],id:null,name:'Variant '+i,blocks:{P1:i?preset2:preset1,BG:background,ACTION:'unselected action'},constraints:{}}));}
 await nav('Personnages');await wait(()=>document.querySelector('[data-nav="characters"][aria-current="page"]'));await nav('Workflows');await wait(()=>button(name));button(name).click();await wait(()=>editor());editor().open=true;await wait(()=>button('Réinitialiser depuis les textes du workflow'));button('Réinitialiser depuis les textes du workflow').click();await wait(()=>control('Prompt P1 exact').value==='old identity one');
 const select=async i=>{set(selectors(i)[0],chars[i],'change');await wait(()=>[...selectors(i)[1].options].some(o=>o.value===variants[i]));set(selectors(i)[1],variants[i],'change');};
 const compare=async i=>{button('Comparer ce preset pour P'+(i+1)).click();await wait(()=>editor().querySelector('[data-couple-preview]'));};
 const apply=async i=>{await compare(i);button('Appliquer les sections affichées').click();await wait(()=>!editor().querySelector('[data-couple-preview]'));};
 await select(0);label('Charger aussi BG',person(0)).querySelector('input').click();await compare(0);
 if(control('Prompt P1 exact').value!=='old identity one'||control('BG indépendant').value!=='old background')throw new Error('Preview mutated editor');button('Annuler ce chargement Couple').click();await wait(()=>!editor().querySelector('[data-couple-preview]'));await apply(0);
 if(control('Prompt P1 exact').value!==preset1||control('BG indépendant').value!==background||control('ACTION commune').value!=='old action')throw new Error('Wrong preset sections');
 await select(1);control('Ajouter les presets').click();await apply(1);if(control('Prompt P2 exact').value!=='old identity two\n'+preset2)throw new Error('Append lost exact tags');
 const calls=(await invoke('smoke_actions')).length,jobsBefore=JSON.stringify(await api('jobs'));button('Annuler les éditions Couple').click();await wait(()=>control('Prompt P1 exact').value==='old identity one'&&selectors(0)[0].value==='');
 if(control('Prompt P2 exact').value!=='old identity two'||control('BG indépendant').value!=='old background'||control('Ajouter les presets').checked||selectors(1)[1].value!=='')throw new Error('Undo not exact');
 const writes=['prepare','prepare_couple','save_bindings','submit','save_character','save_variant','set_preference'];if((await invoke('smoke_actions')).slice(calls).some(a=>writes.includes(a))||JSON.stringify(await api('jobs'))!==jobsBefore)throw new Error('Undo wrote SQLite');
 await select(0);label('Charger aussi BG',person(0)).querySelector('input').click();await apply(0);await select(1);await apply(1);
 set(control('Zones SayaSplitMask'),'mask','change');await wait(()=>control('Direction'));set(control('Direction'),'horizontal','change');set(control('Séparation (%)'),'40');set(control('Flou (sigma pixels)'),'1.5');control('Inverser les deux masques').click();
 button('Valider et préparer Saya Couple').click();await wait(()=>document.querySelector('[data-prepared-request]')&&!button('Annuler les éditions Couple').disabled);
 const text=document.querySelector('[data-prepared-request] pre').textContent,request=JSON.parse(text);
 if(request.characters[0].variant.id!==variants[0]||request.characters[1].variant.id!==variants[1]||request.prompt.p1.inputs.text!==preset1||request.prompt.p2.inputs.text!==preset2||request.prompt.bg.inputs.text!==background||request.prompt.action.inputs.text!=='old action'||!text.includes('18446744073709551615'))throw new Error('Prepared mismatch');
 if(request.prompt.mask.inputs.direction!=='horizontal'||request.prompt.mask.inputs.split!==40||request.prompt.mask.inputs.blur!==1.5||request.prompt.mask.inputs.swap!==true)throw new Error('Mask mismatch');
 for(const [nodeId,node]of Object.entries(graph))for(const [key,value]of Object.entries(node.inputs))if(Array.isArray(value)&&JSON.stringify(request.prompt[nodeId].inputs[key])!==JSON.stringify(value))throw new Error('Connection changed '+nodeId+'.'+key);
 const preparedJobs=JSON.stringify(await api('jobs'));set(control('Prompt P1 exact'),'successive edit 1');set(control('Prompt P1 exact'),'successive edit 2');set(control('Séparation (%)'),'70');set(selectors(1)[1],'','change');button('Annuler les éditions Couple').click();await wait(()=>control('Prompt P1 exact').value===preset1&&selectors(1)[1].value===variants[1]);
 if(control('Séparation (%)').value!=='40'||document.querySelector('[data-prepared-request]')||JSON.stringify(await api('jobs'))!==preparedJobs)throw new Error('Prepared checkpoint not restored locally');
 button('Réinitialiser depuis les textes du workflow').click();await wait(()=>control('Prompt P1 exact').value==='old identity one');if(selectors(0)[0].value!==''||control('Zones SayaSplitMask').value!=='')throw new Error('Reset retained selections');
 if((await api('workflow',{id})).fields.find(f=>f.nodeId==='p1').value!=='old identity one'||(await api('jobs')).some(j=>j.promptId))throw new Error('Original modified or submitted');
 report.checks.push({previewAndCancel:true,explicitSharedSections:true,exactAppend:true,independentVariants:true,localUndo:true,preparedUndo:true,maskParameters:true,linksPreserved:true,originalPreserved:true,u64Seed:true,engineVersion:status.version,submitted:false});if(!(await invoke('smoke_actions')).includes('prepare_couple'))throw new Error('Native action audit inactive');report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);report.uiErrors=[...document.querySelectorAll('.error')].map(e=>e.textContent);}
 await invoke('finish_smoke',{report});
})();
