(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke("studio",{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const wait=async(p,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+30000;while(!await p()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,80));}};
 const button=(text)=>[...document.querySelectorAll("button")].find(b=>b.textContent.trim()===text);
 const click=async(text)=>{await wait(()=>button(text)&&!button(text).disabled,text);button(text).click()};
 window.addEventListener("error",e=>report.errors.push(e.message));
 window.addEventListener("unhandledrejection",e=>report.errors.push(String(e.reason)));
 try{
  await wait(()=>document.querySelector("[data-library-sources]"),"Sources visible");
  assert(document.querySelector("[data-browse-directories]"),"Native chooser control missing");
  const prefs=await api("preferences"),root=prefs.libraryRoot;
  if(window.__V12_SMOKE_PHASE__!=="restart"){
   await click("Indexer / actualiser");
   await wait(async()=>!(await api("index_status")).running&&(await api("gallery",{filters:{}})).total===130,"130 indexed images");
   await wait(()=>document.querySelectorAll("[data-source-id]").length===1,"Source automatically registered");
   const input=document.querySelector(".root-field input");input.closest("details").open=true;
   input.value=root;input.dispatchEvent(new Event("input",{bubbles:true}));
   await click("Ajouter ce dossier");
   await wait(()=>!button("Ajouter ce dossier").disabled,"Duplicate add done");
   assert((await api("library_sources")).length===1,"Duplicate source inserted");
   await click("Retirer cette source");
   await wait(async()=>(await api("gallery",{filters:{}})).total===0,"Disabled source still active");
   assert((await api("library_sources"))[0].enabled===false,"Disabled source lost");
   await click("Réactiver cette source");
   await wait(async()=>(await api("gallery",{filters:{}})).total===130,"Reactivation lost assets");
   await click("Réindexer toutes les sources");
   await wait(()=>!document.querySelector("[data-index-all]").disabled,"Reindex complete");
   assert((await api("library_sources"))[0].state==="ready","Source not ready");
   report.checks.push("manual add, duplicate prevention, remove, reactivate, reindex-all via actual UI");
  }else{
   const sources=await api("library_sources");assert(sources.length===1&&sources[0].enabled&&sources[0].path===root,"Sources not persisted");
   assert((await api("gallery",{filters:{}})).total===130,"Assets not persisted");
   await wait(()=>document.querySelector("[data-source-id]"),"Source list restored");
   report.checks.push("native restart: source, 130 images and status restored");
  }
  assert(report.errors.length===0,report.errors.join(";"));report.ok=true;
  report.picker="Control wired to Tauri/XDG Portal; interactive OS dialog selection not exercised by this smoke";
 }catch(e){report.failure=String(e);}
 await invoke("finish_smoke",{report});
})();
