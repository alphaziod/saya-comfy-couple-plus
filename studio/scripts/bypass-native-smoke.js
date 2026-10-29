(async()=>{
 // Bypass of a node from Generation: ComfyUI mode 4 in the working copy,
 // node absent from the API payload, re-enable, then a real launch (fake ComfyUI).
 const report={ok:false,checks:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m,ms=65000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,120))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
 const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const byText=t=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===t);
 const apiText=async()=>{await press('[data-generation-api]');await wait(()=>document.querySelector('.secondary pre'),'API preview');const text=document.querySelector('.secondary pre').textContent;document.querySelector('.secondary header button').click();await wait(()=>!document.querySelector('.secondary pre'),'API preview closed');return text};
 try{
 await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
 if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
 await wait(()=>document.querySelector('nav'),'navigation');await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');
 set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');await wait(()=>document.querySelector('[data-generation-api]'),'workflow opened');
 const ensureNodes=()=>wait(async()=>{if(!byText('Communs')){byText('Nodes par passe')?.click();await new Promise(r=>setTimeout(r,600))}return !!byText('Communs')},'nodes tab');
 await new Promise(r=>setTimeout(r,3000));await ensureNodes();
 const found=new Set();let target='';
 for(const label of ['Passe 1','Communs']){await ensureNodes();byText(label).click();await wait(async()=>{await ensureNodes();const b=byText(label);if(b&&!b.disabled&&!b.classList.contains('chosen'))b.click();return !!b&&!b.disabled&&b.classList.contains('chosen')&&!!document.querySelector('[data-node-card]')},'pass '+label,240000);report.passMs=(report.passMs??[]);await new Promise(r=>setTimeout(r,2500));
  (report.passes??=[]).push({label,cards:document.querySelectorAll('[data-node-card]').length,buttons:document.querySelectorAll('[data-node-bypass]').length,disabled:document.querySelectorAll('[data-node-bypass]:disabled').length,ids:[...document.querySelectorAll('[data-node-bypass]')].map(n=>n.dataset.nodeBypass).slice(0,40)});
  for(const n of document.querySelectorAll('[data-node-bypass]'))if(!n.disabled)found.add(n.dataset.nodeBypass);
  target=['100342','664','663','662','655','654'].find(id=>document.querySelector(`[data-node-bypass="${id}"]:not(:disabled)`))||'';if(target)break;}
 report.bypassable=[...found];if(!target)throw Error('no preferred bypassable node: '+[...found].join(','));report.target=target;
 const before=await apiText();if(!before.includes(`"${target}"`))throw Error('target absent before bypass');
 const button=()=>document.querySelector(`[data-node-bypass="${target}"]`);
 button().click();await wait(()=>button()?.getAttribute('aria-pressed')==='true'&&button().textContent.trim()==='Réactiver','bypassed state');
 const bypassed=await apiText();if(bypassed.includes(`"${target}"`)){const i=bypassed.indexOf(`"${target}"`);report.context=bypassed.slice(Math.max(0,i-300),i+200);const session=(await api('preferences')).generationWorkspace?.session;const prep=await api('graph_prepare_api',{session});report.prep=prep.report;report.nodeInApi=!!prep.api?.[target];const v=await api('graph_view',{session});report.viewMode=JSON.parse(v.sourceDetails||'{}').nodes?.find(n=>String(n.id)===target)?.mode;report.sessionId=session;throw Error('bypassed node still in API payload')}
 report.checks.push(`node ${target} bypassed: mode 4, absent from API payload, workflow still converts`);
 button().click();await wait(()=>button()?.getAttribute('aria-pressed')==='false'&&button().textContent.trim()==='Bypass','restored state');
 const restored=await apiText();if(!restored.includes(`"${target}"`))throw Error('restored node missing from API payload');
 report.checks.push(`node ${target} restored: back in API payload`);
 button().click();await wait(()=>button()?.getAttribute('aria-pressed')==='true','bypassed again for launch');
 await press('[data-saya-start]');
 await wait(async()=>(await api('saya_runs')).some(r=>r.phases?.['1']?.job),'phase 1 submitted');
 report.checks.push('generation launched with the bypass applied');report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
