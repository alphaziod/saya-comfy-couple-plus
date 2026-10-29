(async()=>{
 // Chaos smoke: (1) targeted spam of the risky buttons with invariants (one run, one batch,
 // no duplicate publication), (2) seeded random "monkey" over the whole UI: clicks, double
 // clicks, keys, odd text, fast navigation. Records JS errors, IPC failures, UI freezes.
 // Native pickers, folder openers and network downloads are stubbed out (they would block
 // the run or leave the sandbox).
 const report={ok:false,checks:[],errors:[],ipcFailures:{},freezes:[],trail:[],stats:{}};window.__SMOKE_STAGE=report;
 const rawInvoke=window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
 const BLOCKED=/^(open_|reveal|civitai|download|lora_download|model_download|pick_)/;
 window.__TAURI_INTERNALS__.invoke=async(cmd,args,opts)=>{
  if(String(cmd).startsWith('pick_'))return null;
  const action=cmd==='studio'?args?.request?.action:cmd;
  if(cmd==='studio'&&BLOCKED.test(String(action)))return null;
  try{return await rawInvoke(cmd,args,opts)}catch(e){const k=String(action);report.ipcFailures[k]=report.ipcFailures[k]||{count:0,samples:[]};report.ipcFailures[k].count++;if(report.ipcFailures[k].samples.length<3)report.ipcFailures[k].samples.push(String(e?.message??JSON.stringify(e)).slice(0,200));throw e}
 };
 window.open=()=>null;
 const invoke=rawInvoke,api=(action,args={})=>rawInvoke('studio',{request:{action,...args}});
 window.addEventListener('error',e=>report.errors.push({msg:String(e.message).slice(0,300),trail:report.trail.slice(-8)}));
 window.addEventListener('unhandledrejection',e=>{const m=String(e.reason?.message??e.reason).slice(0,300);if(!report.errors.some(x=>x.msg===m))report.errors.push({msg:m,trail:report.trail.slice(-8),rejection:true})});
 let last=performance.now();setInterval(()=>{const now=performance.now();if(now-last>1500)report.freezes.push({ms:Math.round(now-last),trail:report.trail.slice(-5)});last=now},250);
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error('timeout: '+m);await pause(200)}};
 const press=async(s,ms)=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s,ms);document.querySelector(s).click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const assert=(v,m)=>{if(!v)throw Error(m)};
 const spam=async(s,n=20)=>{for(let i=0;i<n;i++){const b=document.querySelector(s);if(b)b.click()}};
 let seed=Number(window.__CHAOS_SEED__||20261029);const rnd=()=>{seed=(seed*1103515245+12345)%2147483648;return seed/2147483648};const pick=a=>a[Math.floor(rnd()*a.length)];
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){await spam('[data-langue="ar"]',5);await spam('[data-langue="fr"]',5);await spam('[data-langue-confirmer]',10);await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
  report.checks.push('langue : spam des choix et de Confirmer → une seule confirmation');
  // Real images from the GPU pull, if provided.
  if(window.__CHAOS_LIBRARY__){const r=await api('index',{root:window.__CHAOS_LIBRARY__});report.stats.indexed=r.indexed??r.assets??r;}
  // ---- 1. targeted spam with invariants ----
  document.querySelector('[data-nav="generation"]').click();await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await spam('[data-workflow-open]',15);
  await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'session');
  await wait(()=>document.querySelector('[data-saya-start]')&&!document.querySelector('[data-saya-start]').disabled,'start enabled',60000);
  const runsBefore=(await api('saya_runs')).length;
  await spam('[data-saya-start]',25);
  await wait(async()=>(await api('saya_runs')).length>runsBefore,'a run appears after spam',60000);await pause(8000);await spam('[data-saya-start]',10);await pause(5000);
  const runsAfter=(await api('saya_runs')).length;
  report.stats.generateSpam={runsBefore,runsAfter};
  assert(runsAfter-runsBefore===1,'spam Générer ×25 created '+(runsAfter-runsBefore)+' runs');
  report.checks.push('Générer ×25 → 1 seul run');
  await wait(()=>document.querySelector('[data-saya-stop]'),'stop button',30000);await spam('[data-saya-stop]',10);await pause(3000);
  document.querySelector('[data-nav="casting"]').click();
  await wait(()=>(document.querySelector('[data-casting-source]')?.textContent??'').includes('Config consommée'),'casting source');
  set('[data-casting-n]','4');await spam('[data-casting-lancer]',25);
  await wait(async()=>(await api('casting_history',{sync:false})).length>0,'batch appears after spam',120000);await pause(5000);await spam('[data-casting-lancer]',10);await pause(5000);
  const lots=new Set((await api('casting_history',{sync:false})).map(r=>r.batch??r.lot??r.catalogue));
  report.stats.castingSpam={rows:(await api('casting_history',{sync:false})).length,batches:lots.size};
  assert((await api('casting_history',{sync:false})).length===4,'spam Lancer ×25 → '+(await api('casting_history',{sync:false})).length+' trials');
  report.checks.push('Lancer le casting ×25 → 1 seul lot de 4');
  for(let i=0;i<15;i++){await spam('[data-casting-pause]',3);await spam('[data-casting-resume]',3);await spam('[data-casting-runbar-pause]',2);}
  await pause(3000);
  const states=new Set((await api('casting_history',{sync:true})).map(r=>r.runState));report.stats.afterStorm=[...states];
  assert(states.size===1,'pause/resume storm left mixed states: '+[...states]);
  if(states.has('paused')){await press('[data-casting-resume]',30000)}
  await wait(async()=>(await api('casting_history',{sync:true})).filter(r=>r.asset).length>=1,'casting progresses after pause/resume storm',120000);
  await spam('[data-casting-stop]',10);await wait(()=>!document.querySelector('[data-casting-runbar]'),'stopped after spam',60000);
  report.checks.push('pause/reprise ×45 puis stop ×10 → arrêt propre');
  const unfinished=(await api('casting_history',{sync:true})).filter(r=>!r.asset).length;report.stats.unfinishedAfterStop=unfinished;
  if(unfinished){await wait(()=>{const b=document.querySelector('[data-casting-retry]');return b&&!b.disabled},'finish button',30000);
  await spam('[data-casting-retry]',20);}
  await wait(async()=>{const rows=await api('casting_history',{sync:true});return rows.length===4&&rows.every(r=>r.asset)},'4 images after finish spam',180000);
  report.checks.push('Terminer le lot ×20 → 4/4, pas de doublon');
  const g0=(await api('gallery',{filters:{}})).items.length;
  document.querySelectorAll('[data-casting-select]').forEach(b=>b.click());
  await spam('[data-casting-publish-selection]',20);await pause(4000);
  const g1=(await api('gallery',{filters:{}})).items.length;
  assert(g1-g0<=4,'publish spam added '+(g1-g0));report.stats.publishSpam={g0,g1};
  report.checks.push(`Publier ×20 → +${g1-g0} (≤4, sans doublon)`);
  // Step-by-step mode under spam: one batch, releases never run ahead of the user.
  const stepRows=async()=>(await api('casting_history',{sync:true})).filter(r=>r.step===true);
  await spam('[data-casting-step-start]',10);
  await wait(async()=>(await stepRows()).length>=1,'step batch drawn',120000);
  await wait(()=>document.querySelector('[data-casting-step]'),'step view',30000);
  const stepLots=new Set((await stepRows()).map(r=>r.catalogue));assert(stepLots.size===1,'« Une par une » ×10 → '+stepLots.size+' lots');
  for(let i=0;i<6;i++){
   await wait(async()=>(await stepRows()).some(r=>r.released&&r.asset),'step image ready',120000);
   await spam('[data-casting-step-next]',20);await pause(1500);
   if(i===2){await spam('[data-casting-step-full]',10);await pause(1500)}
  }
  const step=await stepRows();report.stats.step={drawn:step.length,released:step.filter(r=>r.released).length,full:step.filter(r=>r.decision==='oui').length};
  assert(report.stats.step.full<=1,'Full ×10 launched '+report.stats.step.full+' full workflows');
  assert(report.stats.step.released<=7,'Suivante spam released '+report.stats.step.released+' trials (≤ 7 expected)');
  report.checks.push(`« Une par une » : 1 lot, ${report.stats.step.released} images libérées pour 6 cycles de spam Suivante ×20, Full ×10 → ${report.stats.step.full} workflow complet`);
  await spam('[data-casting-step-quit]',5);await pause(2000);
  document.querySelector('[data-nav="gallery"]').click();await wait(()=>document.querySelector('[data-gallery-sort]'),'gallery');
  for(let i=0;i<40;i++){set('[data-gallery-sort]',pick(['newest','oldest','name_asc','name_desc','favorites','accepted','character','resolution','filesize','shuffle']));if(i%7===0)await pause(50)}
  await pause(2000);assert(document.querySelector('[data-gallery-count]'),'gallery still rendered after sort storm');
  report.checks.push('tri changé 40× en rafale → galerie toujours rendue');
  document.querySelector('[data-nav="characters"]').click();await wait(()=>document.querySelector('.new-btn'),'characters');
  const first=document.querySelector('.character-item');
  if(first){first.click();await pause(300);const arch=()=>[...document.querySelectorAll('button')].find(b=>/Archiver la fiche|Réactiver la fiche/.test(b.textContent));for(let i=0;i<20;i++){arch()?.click()}await pause(3000);report.checks.push('Archiver/Réactiver ×20 → pas de crash');}
  for(let i=0;i<10;i++)document.querySelector('.new-btn').click();
  report.checks.push('« + Nouveau personnage » ×10');
  // ---- 2. monkey ----
  const NAVS=[...document.querySelectorAll('[data-nav]')].map(n=>n.getAttribute('data-nav'));
  const KEYS=['Escape','Enter',' ','ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Tab','Delete','Backspace','Home','End','PageDown','z','a','s'];
  const TEXTS=['','   ','0','-1','99999999999999999999','1e309','NaN','é à ç 😀🔥','مرحبا بالعالم','<img src=x onerror=alert(1)>','"; DROP TABLE assets; --','../../../etc/passwd','x'.repeat(5000),'{"a":1}','\u0000‮'];
  const visible=el=>{const r=el.getBoundingClientRect();return r.width>0&&r.height>0&&getComputedStyle(el).visibility!=='hidden'};
  const STEPS=Number(window.__CHAOS_STEPS__||2000);const counts={};
  for(let i=0;i<STEPS;i++){
   const kind=pick(['click','click','click','click','dbl','nav','key','key','type','type','select','scroll']);counts[kind]=(counts[kind]||0)+1;
   window.confirm=()=>rnd()<0.5;window.prompt=()=>pick(TEXTS);window.alert=()=>{};
   try{
    if(kind==='nav'){const id=pick(NAVS);report.trail.push('nav '+id);document.querySelector(`[data-nav="${id}"]`)?.click();}
    else if(kind==='click'||kind==='dbl'){const els=[...document.querySelectorAll('button:not([disabled]),[role=button],a,summary,input[type=checkbox],input[type=radio],label')].filter(visible);if(els.length){const el=pick(els);report.trail.push(kind+' '+(el.getAttribute('data-nav')||[...el.attributes].find(a=>a.name.startsWith('data-'))?.name||el.textContent.trim().slice(0,30)));el.click();if(kind==='dbl'){el.click();el.dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))}}}
    else if(kind==='key'){const k=pick(KEYS);const mod=rnd()<0.2;report.trail.push('key '+(mod?'ctrl+':'')+k);const target=document.activeElement||document.body;for(const t of ['keydown','keyup'])target.dispatchEvent(new KeyboardEvent(t,{key:k,bubbles:true,ctrlKey:mod}));}
    else if(kind==='type'){const els=[...document.querySelectorAll('input:not([type=checkbox]):not([type=radio]):not([type=file]):not([disabled]),textarea:not([disabled])')].filter(visible);if(els.length){const el=pick(els),v=pick(TEXTS);report.trail.push('type '+(el.getAttribute('aria-label')||el.placeholder||el.name||el.tagName)+' = '+v.slice(0,20));el.focus();el.value=v;el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));if(rnd()<0.3)el.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));}}
    else if(kind==='select'){const els=[...document.querySelectorAll('select:not([disabled])')].filter(visible);if(els.length){const el=pick(els);const opts=[...el.options];if(opts.length){el.value=pick(opts).value;report.trail.push('select '+(el.getAttribute('aria-label')||'')+' = '+el.value);el.dispatchEvent(new Event('change',{bubbles:true}))}}}
    else if(kind==='scroll'){const els=[...document.querySelectorAll('.wb-scroll,.wb-center,main')];const el=pick(els);if(el)el.scrollTop=rnd()*el.scrollHeight;}
   }catch(e){report.errors.push({msg:'sync: '+String(e?.message||e).slice(0,200),trail:report.trail.slice(-8)})}
   if(report.trail.length>400)report.trail.splice(0,200);
   if(i%25===0)await pause(30+rnd()*120);else await pause(rnd()<0.7?0:15);
  }
  report.stats.monkey=counts;
  // ---- 3. the app must still work ----
  await pause(3000);
  for(const k of Object.keys(window))if(k==='confirm')window.confirm=()=>true;
  const conn=await api('connection');const prefs=await api('preferences');const g=await api('gallery',{filters:{}});
  report.stats.after={connected:conn.connected,gallery:g.items.length,language:prefs.language};
  document.querySelector('[data-nav="gallery"]').click();await wait(()=>document.querySelector('[data-gallery-sort]'),'gallery reachable after monkey',20000);
  document.querySelector('[data-nav="casting"]').click();await wait(()=>document.querySelector('[data-casting-n]'),'casting reachable after monkey',20000);
  report.checks.push('après '+STEPS+' actions aléatoires : app réactive, galerie et casting accessibles');
  report.domErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent.trim().slice(0,200)).filter(Boolean).slice(0,20);
  report.ok=report.errors.length===0;
  if(!report.ok)report.failure=report.errors.length+' JS errors (see errors)';
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.trailTail=report.trail.slice(-20)}
 await invoke('finish_smoke',{report});
})();
