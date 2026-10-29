(async()=>{
 // Injected by reopen-check-smoke.sh: lets the startup cleanup finish, then reports what
 // the gallery shows (count, capture role/phase per image) and the UI counter.
 const report={ok:false};const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 try{
  await pause(Number(window.__REOPEN_WAIT__||25000));
  const page=await api('gallery',{filters:{}});
  report.gallery=page.items.length;
  report.roles=page.items.reduce((m,a)=>{const k=(a.capture_role??a.captureRole??'?')+':'+(a.capture_phase??a.capturePhase??'?');m[k]=(m[k]||0)+1;return m},{});
  document.querySelector('[data-langue-confirmer]')?.click();
  document.querySelector('[data-nav="gallery"]')?.click();await pause(2500);
  report.uiCount=document.querySelector('[data-gallery-count]')?.textContent??null;
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e)}
 await invoke('finish_smoke',{report});
})();
