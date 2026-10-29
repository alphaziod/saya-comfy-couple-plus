(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,errors:[]};
 const wait=async fn=>{const end=Date.now()+20000;while(!fn()){if(Date.now()>end)throw Error('UI timeout: '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const field=text=>[...document.querySelectorAll('label')].find(l=>l.textContent.startsWith(text))?.querySelector('input,textarea');
 const set=(element,value)=>{element.value=value;element.dispatchEvent(new Event('input',{bubbles:true}));};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 try{
  await nav('Prompts');await wait(()=>field('Dossier Saya Prompt Engine'));
  const prefs=await api('preferences');await wait(()=>field('Dossier Saya Prompt Engine').value===(prefs.promptEngineRoot??''));
  set(field('Dossier Saya Prompt Engine'),window.__SUBGRAPH_ENGINE_ROOT__);
  const graph={nodes:[{id:1,type:'PrimitiveStringMultiline',outputs:[{type:'STRING',links:[10]}],widgets_values:['source exact']},{id:2,type:'sub',inputs:[{name:'prompt',type:'STRING',link:10}]},{id:3,type:'sub',inputs:[{name:'prompt',type:'STRING',link:null,widget:{name:'prompt'}}],widgets_values:['ambiguous default']}],links:[[10,1,0,2,0,'STRING']],definitions:{subgraphs:[{id:'sub',inputNode:{id:-10},inputs:[{name:'prompt',type:'STRING',linkIds:[20]}],nodes:[{id:5,type:'CLIPTextEncode',inputs:[{name:'text',type:'STRING',link:20}]}],links:[{id:20,origin_id:-10,origin_slot:0,target_id:5,target_slot:0,type:'STRING'}]}]}};
  const text=JSON.stringify(graph);set(field('JSON'),text);
  const before=JSON.stringify(await api('workflows'));button('Inspecter sans importer ni exécuter').click();
  await wait(()=>document.querySelector('[data-exposed-inputs]'));
  const panel=document.querySelector('[data-exposed-inputs]');
  if(panel.querySelectorAll('article').length!==2||!panel.textContent.includes('Câblage sauvegardé vérifié')||!panel.textContent.includes('valeur du widget promu non prouvée')||!panel.textContent.includes('5.text'))throw Error('Missing instance diagnostics');
  if(panel.querySelector('input,textarea,select')||field('JSON').value!==text||JSON.stringify(await api('workflows'))!==before)throw Error('Read-only inspection mutated data');
  const broken=structuredClone(graph);broken.links[0][3]=999;set(field('JSON'),JSON.stringify(broken));await wait(()=>!document.querySelector('[data-exposed-inputs]'));
  button('Inspecter sans importer ni exécuter').click();await wait(()=>document.querySelector('[data-exposed-inputs]')?.textContent.includes('Lien parent incohérent'));
  if((await invoke('smoke_actions')).some(a=>['prepare','prepare_couple','submit','save_bindings'].includes(a)))throw Error('Unexpected workflow mutation');
  report.ok=!report.errors.length;report.instances=2;report.ambiguousRefused=true;report.sourceUnchanged=true;report.gpuSubmitted=false;
 }catch(e){report.failure=String(e?.message??e);report.uiErrors=[...document.querySelectorAll('.error')].map(e=>e.textContent);}
 await invoke('finish_smoke',{report});
})();
