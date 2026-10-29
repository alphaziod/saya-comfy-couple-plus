(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false};
 const wait=async fn=>{const end=Date.now()+30000;while(!await fn()){if(Date.now()>end)throw Error('Timeout: '+fn);await new Promise(r=>setTimeout(r,50));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 try{
  await wait(()=>innerWidth===1920&&innerHeight===1080);report.viewport={width:innerWidth,height:innerHeight};
  await wait(()=>button('Reprendre ce dossier')&&!button('Reprendre ce dossier').disabled);
  const before=(await api('index_status')).lastRun;
  if(before.state!=='interrupted')throw Error('Interrupted fixture missing');
  let ticks=0;const timer=setInterval(()=>ticks++,20);
  button('Reprendre ce dossier').click();
  await wait(async()=>{const state=await api('index_status');return state.running&&state.completed>=2&&state.completed<state.total;});
  await wait(()=>button('Interrompre proprement')&&!button('Interrompre proprement').disabled);
  button('Interrompre proprement').click();
  await wait(async()=>{const state=await api('index_status');return !state.running&&state.lastRun.id!==before.id&&state.lastRun.state==='interrupted';});
  const stopped=(await api('index_status')).lastRun;
  if(stopped.completed>=stopped.total)throw Error('Cancellation happened too late');
  await wait(()=>document.querySelector(`[data-index-run="${stopped.id}"]`)&&button('Reprendre ce dossier')&&!button('Reprendre ce dossier').disabled);
  button('Reprendre ce dossier').click();
  await wait(async()=>{const state=await api('index_status');return state.running;});
  const began=Date.now();await api('preferences');report.concurrentReadMs=Date.now()-began;
  await wait(async()=>{const state=await api('index_status');return !state.running&&state.lastRun.id!==before.id&&state.lastRun.state==='completed';});
  clearInterval(timer);
  const state=await api('index_status');if(state.lastRun.completed!==1001||state.lastRun.errorCount!==1||state.lastRun.unchanged<stopped.completed)throw Error('Wrong completed counters');
  await wait(()=>button('Lire les erreurs enregistrées'));button('Lire les erreurs enregistrées').click();
  await wait(()=>document.querySelector('[data-index-run]')?.textContent.includes('broken.png'));
  if((await api('gallery',{filters:{}})).total!==1000)throw Error('Missing valid images');
  if(!ticks||report.concurrentReadMs>5000)throw Error('Index blocks interaction');
  if((await invoke('smoke_actions')).includes('submit'))throw Error('Unexpected generation');
  report.ok=true;report.ticksDuringIndex=ticks;report.state=state.lastRun;report.errorsPersisted=true;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
