(async()=>{
 const report={ok:false,checks:[]};window.__SMOKE_STAGE=report;
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+65000;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,120))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
 const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 try{
 await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
 if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language confirmed');}
 if(window.__V12_SMOKE_PHASE__==='initial'){
 await wait(()=>document.querySelector('nav'),'navigation');await nav('Génération');await wait(()=>document.querySelector('[data-saya-source]'),'source');set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
 await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');await nav('Casting');await wait(()=>document.querySelector('[data-casting-source]')?.textContent.includes('Config consommée'),'casting source');
 set('[data-casting-n]','3');await press('[data-casting-lancer]');await wait(async()=>(await api('casting_history',{sync:false})).some(r=>['submitted','queued','running'].includes(r.state)),'first trial submitted');await press('[data-casting-pause]');
 await wait(async()=>{const rows=await api('casting_history',{sync:true});return rows.length===3&&rows.filter(r=>r.asset).length===1},'one image completed while paused');
 const before=await api('casting_history',{sync:false}),done=before.find(r=>r.asset);
 await press('[data-casting-stop]');await wait(()=>!document.querySelector('[data-casting-runbar]'),'batch stopped');
 report.before=before;report.done=done;report.ok=true;report.checks.push('stopped at 1/3 before app restart');await invoke('finish_smoke',{report});return;
 }
 const {before,done}=window.__V12_SMOKE_PREVIOUS__;
 if(!before||!done)throw Error('missing initial batch evidence');
 await wait(()=>document.querySelector('nav'),'restart navigation');await nav('Casting');await wait(()=>document.querySelector('[data-casting-lot]')?.textContent.includes('1/3'),'progress after app restart');
 await wait(()=>{const b=document.querySelector('[data-casting-retry]');return b&&!b.disabled&&b.textContent.includes('Terminer le lot')&&b.textContent.includes('2')},'finish two remaining');
 document.querySelector('[data-casting-retry]').click();
 await wait(async()=>{const rows=await api('casting_history',{sync:true});return rows.length===3&&rows.every(r=>r.asset)},'all three images');
 const after=await api('casting_history',{sync:false}),kept=after.find(r=>r.run===done.run);
 if(kept.asset!==done.asset||kept.phases['1'].job!==done.phases['1'].job)throw Error('completed image regenerated');
 for(const row of after.filter(r=>r.run!==done.run)){const old=before.find(r=>r.run===row.run);if(old.seed!==row.seed||old.phases['1'].job===row.phases['1'].job)throw Error('remaining replay seed/job incorrect');}
 await wait(()=>document.querySelector('[data-casting-lot]')?.textContent.includes('3/3'),'completed progress');
 report.checks.push('1/3 persisted across app restart; finish remaining 2; 3/3; original image/job preserved; replay seeds unchanged');
 // Navigation: the "Générations" tab no longer exists.
 if([...document.querySelectorAll('nav button')].some(b=>b.textContent.trim()==='Générations'))throw Error('Générations tab still visible');
 report.checks.push('Générations tab removed');
 // Preview: image fitted to the window (ratio kept), navigation and closing.
 document.querySelector('[data-casting-preview]').click();
 await wait(()=>document.querySelector('[data-casting-preview-dialog] .casting-large-image img')?.naturalWidth>0,'large preview');
 const img=document.querySelector('[data-casting-preview-dialog] .casting-large-image img'),box=img.getBoundingClientRect(),scale=Math.min(box.width/img.naturalWidth,box.height/img.naturalHeight);
 report.preview={viewport:[innerWidth,innerHeight],box:[Math.round(box.width),Math.round(box.height)],content:[Math.round(img.naturalWidth*scale),Math.round(img.naturalHeight*scale)],fit:getComputedStyle(img).objectFit};
 if(img.naturalHeight*scale<0.75*innerHeight||report.preview.fit!=='contain')throw Error('preview too small: '+JSON.stringify(report.preview));
 const pos=document.querySelector('[data-casting-preview-position]').textContent;document.querySelector('[data-casting-preview-next]').click();
 await wait(()=>document.querySelector('[data-casting-preview-position]').textContent!==pos,'preview next');
 document.querySelector('[data-casting-preview-fermer]').click();await wait(()=>!document.querySelector('[data-casting-preview-dialog]'),'preview closed');
 report.checks.push('preview fills '+Math.round(100*img.naturalHeight*scale/innerHeight)+'% of window height, ratio kept, next/close work');
 // Casting -> gallery: multi-select, explicit confirmation, no duplicates.
 const galleryIds=async()=>(await api('gallery',{filters:{}})).items.map(a=>a.id).sort();
 if((await galleryIds()).length)throw Error('unvalidated casting images already in gallery');
 const boxes=[...document.querySelectorAll('[data-casting-select]')];if(boxes.length<2)throw Error('selection boxes missing');
 for(const b of boxes.slice(0,2)){b.click();}
 await press('[data-casting-publish-selection]');
 await wait(async()=>(await api('casting_history',{sync:false})).filter(r=>r.published).length===2,'two published');
 await wait(()=>document.querySelectorAll('[data-casting-published]').length===2,'published badges');
 const published=await galleryIds();if(published.length!==2)throw Error('gallery count after publish '+published.length);
 const again=await api('casting_publish',{runs:(await api('casting_history',{sync:false})).filter(r=>r.published).map(r=>r.run)});
 if(again.published!==0||(await galleryIds()).join()!==published.join())throw Error('second validation duplicated');
 report.checks.push('2 selected images validated into gallery; second validation adds nothing');
 // Batch deletion: validated ones kept in the gallery, the remaining trial erased.
 window.confirm=()=>true;await press('[data-casting-delete]');
 await wait(async()=>(await api('casting_history',{sync:false})).length===0,'lot deleted');
 await wait(()=>!document.querySelector('[data-casting-lot]'),'lot row gone');
 if((await galleryIds()).join()!==published.join())throw Error('published images lost after lot deletion');
 await nav('Galerie');await wait(()=>/\b2 images/.test(document.querySelector('[data-gallery-count]')?.textContent??''),'gallery page shows the 2 validated images');
 report.checks.push('lot deleted; validated images remain in gallery');report.ok=true;
 }catch(e){report.failure=String(e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
