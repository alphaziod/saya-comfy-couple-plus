(async()=>{
 const report={ok:false,checks:[],errors:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},wait=async(f,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+20000;while(!await f()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,50))}};
 const root='__SMOKE_REPO_ROOT__',fixture='/tmp/saya-temporary-smoke.png';
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const active=()=>document.querySelector('[data-temporary-workflow]')?.getAttribute('data-temporary-workflow');
 const field=input=>document.querySelector('.pass-group [data-input="'+input+'"]');
 window.addEventListener('error',e=>report.errors.push(e.message));
 try{
 const normal=await api('graph_open_file',{path:root+'/scripts/fixtures/v15.2-subgraph-ui.json'});
 await api('set_preference',{key:'generationWorkspace',value:{session:normal.session,name:'Prior preparation',sourcePath:root+'/scripts/fixtures/v15.2-subgraph-ui.json',drafts:{},scopeDrafts:{}}});
 const original=JSON.stringify(await api('workflows')),sessions=JSON.stringify(await api('graph_sessions')),initialJobs=(await api('jobs')).length;
 await nav('Génération');await wait(()=>document.querySelector('.preparation-bar input')?.value==='Prior preparation','normal editor restored');
 const title=document.querySelector('.preparation-bar input');title.value='Unsaved prior preparation';title.dispatchEvent(new Event('input',{bubbles:true}));
 await invoke('smoke_drop_image',{path:fixture});await wait(()=>active()&&field('seed'),'PNG in normal workspace');
 assert(document.querySelectorAll('[data-generation-workspace]').length===1&&!document.querySelector('[data-image-restore]'),'extra lower panel');
 assert(field('seed').value==='18446744073709551615','seed precision');assert(field('text').value==='exact,  prompt\nline','prompt exact');
 assert(!document.querySelector('[data-generation-save]'),'temporary offers save');
 const token=active();document.querySelector('[data-image-close]').click();await wait(()=>!active(),'close temporary');
 assert(document.querySelector('.preparation-bar input').value==='Unsaved prior preparation','lost previous preparation');
 let rejected=false;try{await api('graph_view',{session:token})}catch{rejected=true}assert(rejected,'temporary editor survived closing');
 assert(JSON.stringify(await api('graph_sessions'))===sessions,'temporary saved editor');assert(JSON.stringify(await api('workflows'))===original&&(await api('jobs')).length===initialJobs,'drop persisted workflow/job');
 report.checks.push('PNG replaces normal workspace; Close restores previous work; no extra panel or saved session');
 await invoke('smoke_drop_image',{path:fixture});await wait(()=>active()&&field('text'),'second temporary');
 field('text').value='edited,  exact\ntext';field('text').dispatchEvent(new Event('input',{bubbles:true}));
 assert(!document.querySelector('[data-image-submit]'),'separate launch removed');await wait(()=>!document.querySelector('[data-saya-start]').disabled,'submit ready');document.querySelector('[data-saya-start]').click();await wait(()=>document.querySelector('.model-dialog'),'missing model popup');
 assert((await api('jobs')).length===initialJobs,'no job submitted before model replacement');
 const replacement=document.querySelector('.model-dialog select');replacement.value='available.safetensors';replacement.dispatchEvent(new Event('change',{bubbles:true}));
 await wait(()=>!document.querySelector('.model-dialog .primary').disabled,'replacement chosen');document.querySelector('.model-dialog .primary').click();
 await wait(()=>document.querySelector('[data-temporary-job]'),'mock submission');
 const runningSession=active(),runningJob=document.querySelector('[data-temporary-job]').dataset.temporaryJob;
 await nav('Galerie');await nav('Génération');assert(active()===runningSession,'running temporary session preserved');assert(document.querySelector('[data-temporary-job]').dataset.temporaryJob===runningJob,'running job preserved');assert(field('text').value==='edited,  exact\ntext','edited prompts preserved');
 report.checks.push('missing model replaced directly before submission; active run and edits survive navigation');
 assert((await api('jobs')).length===initialJobs+1,'one job');assert(JSON.stringify(await api('workflows'))===original,'saved workflow pollution');assert(JSON.stringify(await api('graph_sessions'))===sessions,'editing saved temp session');
 document.querySelector('[data-image-close]').click();await wait(()=>!active(),'close submitted');
 // An embedded UI workflow is opened as supplied, without preset expansion.
 await invoke('smoke_drop_image',{path:root+'/crates/studio-core/tests/fixtures/restore.png'});await wait(()=>active(),'UI PNG opened');
 const ui=await api('graph_view',{session:active()});assert(ui.sourceFormat==='comfy_ui','embedded UI replaced by API');const source=JSON.parse(ui.sourceDetails);assert(source.nodes.length===2&&!source.extra?.saya_preset,'UI workflow transformed');
 const uiToken=active();await nav('Galerie');await nav('Génération');await wait(()=>active()===uiToken,'temporary editor preserved after navigation');
 assert((await api('graph_view',{session:uiToken})).sourceFormat==='comfy_ui','session remains editable');document.querySelector('[data-image-close]').click();await wait(()=>!active(),'explicit close');
 report.checks.push('edited workflow submitted once; embedded UI preserved; navigation preserves temporary editor; explicit close clears it');assert(!report.errors.length,'JS errors');report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
