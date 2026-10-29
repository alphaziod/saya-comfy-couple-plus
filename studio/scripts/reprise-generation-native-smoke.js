(async()=>{
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{const end=Date.now()+20000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(100)}};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click();await pause(300);if(t==='Génération'){document.querySelector('[data-generation-tools]').click();await pause(100)}};
 const input=(selector,value)=>{const n=document.querySelector(selector);assert(n,selector);n.value=value;n.dispatchEvent(new Event('input',{bubbles:true}))};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
 await wait(()=>innerWidth===1920&&innerHeight===1080,'fullscreen');
 let expected=window.__V12_SMOKE_PREVIOUS__?.expected;
 if(report.phase==='initial'){
    const created = await api('save_character', { id:null, name:'Alpha', identity:{identity_prompt:'full P1,  exact\nligne', anatomy_prompt:'anatomie exacte'} });
    await api('save_character',{id:null,name:'Beta',identity:{identity_prompt:'second identity exact',anatomy_prompt:'second anatomy'}});
    await api('save_variant', { character:created, id:null, name:'BG Garden', blocks:{BG:'jardin exact'}, constraints:{} });
    const card = { id:created, name:'Alpha' };
    const sheet = await api('character_sheet', { id: card.id });
    assert(typeof sheet.identityPrompt === 'string' && sheet.identityPrompt.length > 0, 'Character P1 prompt missing');
    const graph = {
      '1':{class_type:'CheckpointLoaderSimple',inputs:{ckpt_name:'fixture-ckpt_name.safetensors'}},
      '2':{class_type:'CLIPTextEncode',inputs:{text:'old P1',clip:['1',1]}},
      '3':{class_type:'CLIPTextEncode',inputs:{text:'old anatomy',clip:['1',1]}},
      '4':{class_type:'CLIPTextEncode',inputs:{text:'old BG',clip:['1',1]}},
      '5':{class_type:'CLIPTextEncode',inputs:{text:'old action',clip:['1',1]}},
      '6':{class_type:'EmptyLatentImage',inputs:{width:512,height:512,batch_size:1}},
      '7':{class_type:'KSampler',inputs:{seed:42,steps:20,cfg:6.0,sampler_name:'euler',scheduler:'normal',denoise:1.0,model:['1',0],positive:['2',0],negative:['4',0],latent_image:['6',0]}},
      '8':{class_type:'VAEDecode',inputs:{samples:['7',0],vae:['1',2]}},
      '9':{class_type:'SaveImage',inputs:{images:['8',0],filename_prefix:'universal'}}
    };
    const workflow = await api('import_workflow', { name:'Universal fixture', text:JSON.stringify(graph) });
    const original = (await api('workflow', { id: workflow })).fields.map(f=>[f.nodeId,f.inputName,f.value]);
    for (const [key,node,input] of [['P1','2','text'],['ANATOMY_P1','3','text'],['BG','4','text'],['ACTION','5','text'],['QUICK_STEPS','7','steps']]) {
      await api('save_semantic_binding', { id:workflow, key, node, input });
    }
    const config = {
      p1:{characterId:card.id,variantId:null}, p2:{characterId:''},
      bg:{source:'fixed',text:'jardin exact',selected:'jardin exact'},
      action:{source:'fixed',text:'running',selected:'running'},
      mappings:{P1:{nodeId:'2',inputName:'text'},ANATOMY_P1:{nodeId:'3',inputName:'text'},BG:{nodeId:'4',inputName:'text'},ACTION:{nodeId:'5',inputName:'text'}},
      quick:[{nodeId:'7',inputName:'steps',value:'33',visible:true}],
      favorites:[card.id]
    };
    const profile = await api('save_prepare_profile', { id:null, name:'Native universal', workflow, mode:'solo', config });

 expected={workflow,created,identity:sheet.identityPrompt};
 }
 report.expected=expected;
 await nav('Génération');
 await wait(()=>document.querySelector('[data-generation-prepare]'),'generation');
 await wait(()=>document.querySelectorAll('[data-saya-phase]').length===6,'six phase cards');
 await wait(()=>document.querySelector('[data-saya-phase="1"]')?.getAttribute('data-state')==='succeeded','run phase synchronized from database');
 assert(document.querySelector('[data-saya-runs]').value==='fixture-run-0001','run recovered');
 assert(document.querySelector('[data-saya-continue]')?.textContent.includes('2'),'next phase control');
 assert(!document.querySelector('[data-saya-stop]'),'no stop for inactive run');
 report.checks.push('six phases; existing manifest recovery; database state synchronization');
document.querySelector('[data-generation-prepare]').open=true;
 await wait(()=>document.querySelector('[data-up-prompt-p1]')?.value?.length,'profile loaded');
 if(report.phase==='initial'){
 input('[data-up-master-seed]','9007199254740993');input('[data-up-prompt-p1]','temporary portrait, exact');
 await pause(300);document.querySelector('[data-up-preview]').click();
 await wait(()=>document.querySelector('[data-prepared-draft]'),'preview');
 const exact=document.querySelector('[data-prepared-draft]').textContent;assert(exact.includes('9007199254740993'),'exact u64 preview');
 assert(await api('last_queued_seed',{workflow:expected.workflow})===null,'preview advanced last seed');
 assert((await api('character_sheet',{id:expected.created})).identityPrompt===expected.identity,'implicit character save');
 const removal=document.querySelector('[data-draft-allow-removals]');if(removal)removal.click();document.querySelector('[data-draft-confirm]').click();await pause(100);
 [...document.querySelectorAll('[data-prepared-draft] button')].find(n=>n.textContent==='Soumettre à ComfyUI').click();
 await wait(()=>document.querySelector('[data-draft-submitted]'),'mock submission');
 assert(await api('last_queued_seed',{workflow:expected.workflow})==='9007199254740993','last submitted seed');
 report.checks.push('exact u64 preview and accepted HTTP submission','no implicit character save');
 await nav('Galerie');await nav('Génération');document.querySelector('[data-generation-prepare]').open=true;
 }
 await wait(()=>document.querySelector('[data-up-master-seed]')?.value==='9007199254740993','seed restored');
 assert(document.querySelector('[data-up-prompt-p1]').value==='temporary portrait, exact','temporary prompt restored');
 assert((await api('character_sheet',{id:expected.created})).identityPrompt===expected.identity,'character preserved');
 await pause(500);
 report.capture=await invoke('smoke_capture',{name:'generation-'+report.phase});
 if(report.phase==='initial'){
 const workflowsBeforePng=await api('workflows');
 await invoke('smoke_drop_image',{path:'__SMOKE_REPO_ROOT__/crates/studio-core/tests/fixtures/restore.png'});
 await wait(()=>document.querySelector('[data-node="2"][data-input="text"]'),'PNG temporary prompt');
 assert(document.querySelector('[data-node="2"][data-input="text"]').value==='restored prompt from PNG','PNG prompt exact');
 assert(document.querySelector('[data-image-submit]').disabled,'PNG must wait for launch confirmation');
 assert(JSON.stringify(await api('workflows'))===JSON.stringify(workflowsBeforePng),'PNG created saved workflow');
 document.querySelector('[data-image-close]').click();
 await wait(()=>!document.querySelector('[data-temporary-workflow]'),'PNG closed');
 assert((await api('character_sheet',{id:expected.created})).identityPrompt===expected.identity,'PNG saved character');
 report.checks.push('PNG temporary preparation closes without saving a workflow or character');
 const mode=[...document.querySelectorAll('[data-generation-prepare] label')].find(n=>n.textContent.startsWith('Mode')&&!n.textContent.startsWith('Mode seed'))?.querySelector('select');assert(mode,'mode selector');mode.value='duo';mode.dispatchEvent(new Event('change',{bubbles:true}));
 await wait(()=>document.querySelector('[data-up-p2] select'),'P2 selector');const p2=document.querySelector('[data-up-p2] select');const option=[...p2.options].find(n=>n.value&&n.value!==expected.created);assert(option,'second character option');p2.value=option.value;p2.dispatchEvent(new Event('change',{bubbles:true}));
 const second=await api('character_sheet',{id:option.value});await wait(()=>document.querySelector('[data-up-prompt-p2]')?.value===second.identityPrompt,'P2 prompt loaded');assert(document.querySelector('[data-up-prompt-p1]').value==='temporary portrait, exact','P2 selection changed P1');report.checks.push('P1/P2 selectors remain independent');

 }

 report.checks.push('navigation or restart draft recovery');assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e)+' | '+String(e?.stack||'');report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
