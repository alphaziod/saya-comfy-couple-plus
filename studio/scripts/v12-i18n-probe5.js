(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const step=s=>{window.__SMOKE_STEP=s.slice(0,180)};
 const course=(p,ms)=>Promise.race([p,new Promise(r=>setTimeout(()=>r('HANG après '+ms+'ms'),ms))]);
 step('test set_preference direct…');
 const r1=await course(api('set_preference',{key:'probe_test',value:'ok'}),8000);
 step('set_preference → '+JSON.stringify(r1).slice(0,80));
 const r2=await course(api('preferences'),8000);
 step('preferences relues → language='+(r2.language??'absent')+' | probe_test='+JSON.stringify(r2.probe_test));
 step('FIN');
 })();
