(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+20000;while(!await fn()){if(Date.now()>end)throw Error('wait '+m);await new Promise(r=>setTimeout(r,120))}};
 let info={etape:'début'};
 try{
  await wait(()=>document.querySelector('nav'),'navigation');
  info.etape='nav OK';
  await wait(()=>document.querySelector('[data-langue-setup]'),'popup visible');
  info.etape='popup visible';
  info.prefsAvant=await api('preferences');
  info.etape='prefs avant lues';
  document.querySelector('[data-langue-setup] [data-langue="fr"]').click();
  await new Promise(r=>setTimeout(r,300));
  info.etape='fr cliqué';
  document.querySelector('[data-langue-confirmer]').click();
  await new Promise(r=>setTimeout(r,1500));
  info.etape='confirm cliqué';
  info.modalRestante=!!document.querySelector('[data-langue-setup]');
  info.erreurAffichee=[...document.querySelectorAll('.error,[role=alert]')].map(n=>n.textContent).join(' | ');
  info.etape='lecture prefs après';
  info.prefsApres=await api('preferences');
  info.etape='prefs après lues';
  info.htmlLang=document.documentElement.getAttribute('lang');
 }catch(e){
  info.erreur=String(e);
  try{info.prefsApres=await api('preferences')}catch(x){info.prefsApres='hang: '+String(x).slice(0,80)}
  info.erreurAffichee=[...document.querySelectorAll('.error,[role=alert]')].map(n=>n.textContent).join(' | ');
 }
 throw Error('PROBE:'+JSON.stringify(info));
 })();
