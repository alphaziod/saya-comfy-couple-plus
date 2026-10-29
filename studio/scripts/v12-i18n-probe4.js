(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const step=s=>{window.__SMOKE_STEP=s};
 const wait=async(fn,m)=>{const end=Date.now()+20000;while(!await fn()){if(Date.now()>end)throw Error('wait '+m);await new Promise(r=>setTimeout(r,150))}};
 step('nav…');await wait(()=>document.querySelector('nav'),'navigation');
 step('prefs ON MOUNT faite ? lecture…');const p1=await api('preferences');step('prefs1 lues: '+JSON.stringify(p1).slice(0,150));
 await wait(()=>document.querySelector('[data-langue-setup]'),'popup visible');
 step('popup visible; lecture prefs2…');const p2=await api('preferences');step('prefs2 lues: '+JSON.stringify(p2).slice(0,150));
 step('clic fr…');document.querySelector('[data-langue-setup] [data-langue="fr"]').click();
 await new Promise(r=>setTimeout(r,300));
 step('clic confirm…');document.querySelector('[data-langue-confirmer]').click();
 await new Promise(r=>setTimeout(r,1500));
 step('après confirm: modal='+!!document.querySelector('[data-langue-setup]'));
 step('lecture prefs3…');const p3=await api('preferences');step('prefs3: '+JSON.stringify(p3).slice(0,150));
 step('html lang='+document.documentElement.getAttribute('lang'));
 })();
