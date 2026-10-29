(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,checks:[],errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 const wait=async fn=>{const end=Date.now()+20000;while(!fn()){if(Date.now()>end)throw new Error('Condition not reached '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
 const cards=()=>[...document.querySelectorAll('.image-card')];
 const open=async name=>{cards().find(c=>c.textContent.includes(name)).click();await wait(()=>document.querySelector('[data-inspector-path]')?.dataset.inspectorPath.endsWith('/'+name));};
 const close=async()=>{};
 try{
  await wait(()=>button('Indexer / actualiser')&&!button('Indexer / actualiser').disabled);if(!document.querySelector('.root-field input').value.endsWith('/test-results/comparison-native/images'))throw new Error('Unexpected library');
  button('Réinitialiser les filtres').click();button('Indexer / actualiser').click();await wait(()=>cards().length===5&&document.querySelector('.image-grid').getAttribute('aria-busy')==='false');
  const before=JSON.stringify(await api('gallery',{filters:{}}));
  await open('a.png');button('Ajouter à la comparaison').click();await wait(()=>button('Ajouter à la comparaison').disabled);await close();if(!button('Comparer les deux images').disabled)throw new Error('Single image compare enabled');
  const search=document.querySelector('input[placeholder="Rechercher…"]');search.value='ab.png';search.dispatchEvent(new Event('input',{bubbles:true}));await wait(()=>cards().length===1);
  await open('ab.png');button('Ajouter à la comparaison').click();await wait(()=>button('Ajouter à la comparaison').disabled);await close();await wait(()=>!button('Comparer les deux images').disabled);
  button('Réinitialiser les filtres').click();await wait(()=>cards().length===5);await open('abc.png');if(!button('Ajouter à la comparaison').disabled)throw new Error('Third selection permitted');await close();
  const auditStart=(await invoke('smoke_actions')).length;button('Comparer les deux images').click();await wait(()=>document.querySelectorAll('[data-image-comparison] img').length===2&&[...document.querySelectorAll('[data-image-comparison] img')].every(img=>img.complete&&img.naturalWidth>0));
  const dialog=document.querySelector('[data-image-comparison]');await wait(()=>dialog.querySelectorAll('pre').length===2);const headings=[...dialog.querySelectorAll('h2')].map(n=>n.textContent);if(headings.join(',')!=='a.png,ab.png')throw new Error('Wrong images');
  if(!dialog.contains(document.activeElement))throw new Error('Dialog did not acquire focus');
  if(dialog.scrollWidth>dialog.clientWidth)throw new Error('Horizontal dialog overflow');
  dialog.close();await wait(()=>!document.querySelector('[data-image-comparison]'));if((await invoke('smoke_actions')).length!==auditStart)throw new Error('Comparison wrote data');
  if(JSON.stringify(await api('gallery',{filters:{}}))!==before)throw new Error('Comparison changed assets');
  button('Retirer a.png').click();await wait(()=>button('Comparer les deux images').disabled);button('Retirer ab.png').click();await wait(()=>!document.querySelector('[data-comparison-selection]'));
  report.checks.push({twoDecodedPreviews:true,metadataLoaded:true,crossFilterSelection:true,duplicateAndThirdBlocked:true,modalFocus:true,nativeClose:true,readOnly:true,explicitRemoval:true,viewport:[innerWidth,innerHeight]});report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);report.uiErrors=[...document.querySelectorAll('.error')].map(e=>e.textContent);}
 await invoke('finish_smoke',{report});
})();
