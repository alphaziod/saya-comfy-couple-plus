(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));
 const wait=async(fn)=>{const end=Date.now()+10000;while(!fn()){if(Date.now()>end)throw new Error('Condition native non atteinte');await new Promise(r=>setTimeout(r,100));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const label=text=>[...document.querySelectorAll('label')].find(l=>l.firstChild?.textContent.trim()===text);
 const select=async(text,value)=>{const el=label(text)?.querySelector('select');if(!el)throw new Error('Select absent: '+text);el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await new Promise(r=>setTimeout(r,250));};
 const count=()=>document.querySelectorAll('.image-card').length;
 try{
  await wait(()=>innerWidth===1920&&innerHeight===1080);
  const before=await api('preferences');report.checks.push({restoredFromPriorRun:before.galleryFilters?.job==='pair-job'&&before.galleryFilters?.favorite===true});
  await nav('Galerie');
  await wait(()=>button('Réinitialiser les filtres'));button('Réinitialiser les filtres').click();await wait(()=>count()===3);
  await select('Personnage','alpha');await wait(()=>count()===3);await select('Ajouter un personnage (ET)','beta');await wait(()=>count()===2);
  const summary=[...document.querySelectorAll('summary')].find(x=>x.textContent==='Filtrer par provenance de génération');summary.click();
  await select('Variante historique','beta-variant');await select('Workflow','pair-job-workflow');
  const input=label('Identifiant de génération').querySelector('input');input.value='pair-job';input.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(r=>setTimeout(r,350));await wait(()=>count()===2);
  const favorite=[...document.querySelectorAll('label')].find(l=>l.textContent.trim()==='Favoris').querySelector('input');favorite.click();await wait(()=>count()===1);
  const prefs=await api('preferences');if(prefs.galleryFilters.job!=='pair-job'||prefs.galleryFilters.characters[0]!=='beta'||!prefs.galleryFilters.favorite)throw new Error('Filters not persisted');
  document.querySelector('.image-card').click();await wait(()=>document.querySelector('dialog pre')?.textContent.includes('pair-job'));const detail=await api('detail',{id:'pair-job-0'});if(detail.generations[0].nodeId!=='save')throw new Error('Producer not shown');
  report.checks.push({pairAndVariantFilters:true,workflowAndJobFilters:true,favoriteCombined:true,producerNode:true,viewport:[innerWidth,innerHeight],horizontalOverflow:document.documentElement.scrollWidth>innerWidth,submitted:false});report.ok=!report.errors.length&&!report.checks.at(-1).horizontalOverflow;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
