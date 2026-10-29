(async()=>{
  const invoke=window.__TAURI_INTERNALS__.invoke;
  const api=(action,args={})=>invoke('studio',{request:{action,...args}});
  const report={ok:false,checks:[],errors:[]};
  window.addEventListener('error',e=>report.errors.push(e.message));
  try {
    const start=Date.now();let state;
    do{state=await api('live_state');if((state.connected&&state.lastHttpSync)||(!state.connected&&state.error&&state.httpError&&state.retryMs>0))break;if(Date.now()-start>20000)throw new Error(JSON.stringify(state));await new Promise(r=>setTimeout(r,200));}while(true);
    const prefs=await api('preferences');if(state.clientId!==prefs.comfyClientId)throw new Error('Client identity not persisted');
    report.checks.push({websocketConnected:state.connected,httpRecovered:!!state.lastHttpSync,offlineObserved:!state.connected,clientId:state.clientId,reconnects:state.reconnects,trackedJobs:Object.keys(state.jobs).length});
    await new Promise(r=>setTimeout(r,500));
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
await nav('ComfyUI');
    const deadline=Date.now()+10000;while(!document.body.textContent.includes(state.connected?'WebSocket : connecté':'WebSocket : déconnecté')){if(Date.now()>deadline)throw new Error('Native status not shown');await new Promise(r=>setTimeout(r,100));}
    report.checks.push({nativeStatus:true,submitted:false});report.ok=report.errors.length===0;
  }catch(e){report.failure=String(e?.message??e);}
  await invoke('finish_smoke',{report});
})();
