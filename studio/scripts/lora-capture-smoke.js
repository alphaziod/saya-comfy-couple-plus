(async()=>{
 // Visual check of the Models (LoRA) page on synthetic model files (--lora-fixture).
 const report={ok:false,checks:[]};const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m,ms=60000)=>{const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(200)}};
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await pause(500)}
  document.querySelector('[data-nav="lora"]').click();
  await wait(()=>document.querySelectorAll('[data-lora-item]').length>0,'lora items');await pause(1500);
  report.items=document.querySelectorAll('[data-lora-item]').length;
  report.addVisible=[...document.querySelectorAll('.add-btn')].filter(b=>getComputedStyle(b).opacity!=='0').length;
  report.kinds=[...document.querySelectorAll('.kind-list button')].map(b=>b.textContent.trim());
  report.toolbar=[...document.querySelectorAll('.lora-top button')].map(b=>b.textContent.trim());
  report.capture1=await invoke('smoke_capture',{name:'lora-v1-grid'});
  document.querySelector('[data-lora-item] .cover')?.click();await pause(1200);
  document.querySelector('.add-btn')?.click();await pause(600);
  report.capture2=await invoke('smoke_capture',{name:'lora-v1-detail'});
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e)}
 await invoke('finish_smoke',{report});
})();
