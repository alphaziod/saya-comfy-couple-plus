(async()=>{
 const report={ok:false,checks:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+20000;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,120))}};
 const erreur=()=>[...document.querySelectorAll('.error,[role=alert]')].map(n=>n.textContent).filter(Boolean).join(' | ');
 try{
  await wait(()=>document.querySelector('nav'),'navigation');
  await wait(()=>document.querySelector('[data-langue-setup]'),'popup visible');
  const infos={prefsAvant:await api('preferences')};
  document.querySelector('[data-langue-setup] [data-langue="fr"]').click();
  await new Promise(r=>setTimeout(r,300));
  document.querySelector('[data-langue-confirmer]').click();
  await new Promise(r=>setTimeout(r,1200));
  infos.apres=await api('preferences').catch(e=>'PREFS ERR: '+e);
  infos.erreurAffichee=erreur();
  infos.modalEncoreLa=!!document.querySelector('[data-langue-setup]');
  infos.htmlLang=document.documentElement.getAttribute('lang');
  report.probe=infos;
  report.ok=!infos.modalEncoreLa;
 }catch(e){report.failure=String(e);report.erreurAffichee=(()=>{try{return[...document.querySelectorAll('.error,[role=alert]')].map(n=>n.textContent).join(' | ')}catch{return '?'}})()}
 window.__SMOKE_STAGE=report;
 })();
