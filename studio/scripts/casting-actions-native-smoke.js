(async()=>{
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 // V2 native smoke: action targeting in casting: "Cibler des actions" button →
 // LARGE window (data/actions/** bank read-only via actions_lister) →
 // search → multi-select (the window shows EXACTLY the engine draw pool,
 // converged A/B rule slice D: invalid and pas_solo left out, _flaccide sister
 // clones visible) → confirm → back to casting: Launch sends --actions
 // to the engine (SAYA_ENGINE_DIR stub) and the shots land in pass 1 only.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const texte=s=>{const n=document.querySelector(s);return n?n.textContent.trim():''};
 try{
  await wait(()=>document.querySelector('[data-nav]'),'nav');
  await nav('Génération');
  await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);
  document.querySelector('[data-workflow-open]').click();
  await wait(()=>document.querySelectorAll('[data-prompt-node]').length>0&&!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
  await wait(()=>document.querySelector('[data-nav]'),'casting nav');
  await nav('Casting');
  await wait(()=>document.querySelector('[data-casting-cibler-actions]'),'actions button');
  // 1. Open the LARGE window + list the bank (smoke.actions.json fixture).
  // CONVERGED A/B rule (slice D): the window shows EXACTLY the engine draw pool
  // — x_vieux (valide:false) and x_duo (pas_solo) left out, every
  // solo-drawable action cloned with its _flaccide sister (2 sources + 2 sisters = 4 tiles).
  document.querySelector('[data-casting-cibler-actions]').click();
  await wait(()=>document.querySelector('[data-actions-fenetre]'),'actions window open');
  await wait(()=>document.querySelectorAll('[data-action-item]').length===4,'quatre actions disponibles (2 sources + 2 sœurs flaccide), got '+document.querySelectorAll('[data-action-item]').length);
  assert(!document.querySelector('[data-action-item="x_vieux"]'),'non validated action no longer shown');
  assert(!document.querySelector('[data-action-item="x_duo"]'),'pas_solo action hidden (règle convergée A/B : pas_solo disqualifie comme dans eligible_names)');
  const liste=[...document.querySelectorAll('[data-action-item]')].map(n=>n.getAttribute('data-action-item')).sort();
  assert(JSON.stringify(liste)===JSON.stringify(['x_bridon','x_bridon_flaccide','x_pinces_cheveux','x_pinces_cheveux_flaccide']),'sources + sœurs flaccide listées, got '+JSON.stringify(liste));
  report.checks.push('fenêtre ouverte : pool exact du tirage — x_vieux (valide:false) et x_duo (pas_solo) absentes, x_bridon/x_pinces_cheveux + leurs sœurs _flaccide');
  // 2. Search: "pince" leaves the source and its sister.
  set('[data-actions-recherche]','pince');await pause(100);
  const vues=[...document.querySelectorAll('[data-action-item]')].map(n=>n.getAttribute('data-action-item')).sort();
  assert(JSON.stringify(vues)===JSON.stringify(['x_pinces_cheveux','x_pinces_cheveux_flaccide']),'search filters to x_pinces_cheveux + sister, got '+JSON.stringify(vues));
  report.checks.push('recherche « pince » filtre la banque à x_pinces_cheveux et sa sœur _flaccide');
  // 3. Multi-select + confirm.
  set('[data-actions-recherche]','');await pause(100);
  document.querySelector('[data-action-item="x_pinces_cheveux"]').click();
  document.querySelector('[data-action-item="x_bridon"]').click();
  await pause(100);
  assert(document.querySelector('[data-action-item="x_pinces_cheveux"]').getAttribute('data-selectionne')==='oui','x_pinces_cheveux selected');
  assert(document.querySelector('[data-action-item="x_bridon"]').getAttribute('data-selectionne')==='oui','x_bridon selected');
  assert(document.querySelector('[data-actions-compteur]').textContent.includes('x_pinces_cheveux, x_bridon'),'counter lists both names');
  document.querySelector('[data-actions-valider]').click();
  await wait(()=>!document.querySelector('[data-actions-fenetre]'),'actions window closed');
  const chip=texte('[data-casting-actions-cible]');
  assert(chip.includes('2 action')&&chip.includes('ciblée'),'chip shows 2 targeted actions, got '+chip);
  report.checks.push('multi-sélection validée : fenêtre fermée, chip « 2 actions ciblées » dans le panneau');
  // 4. Back in casting: Launch sends --actions to the engine (stub).
  set('[data-casting-n]','2');await pause(100);
  await wait(()=>!document.querySelector('[data-casting-lancer]').disabled,'casting ready');document.querySelector('[data-casting-lancer]').click();
  await wait(()=>document.querySelectorAll('[data-casting-shot]').length===2,'two targeted shots in casting grid');
  await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'targeted phase 1 succeeded');
  const note=texte('.casting-note');
  assert(note.includes('actions ciblées : x_pinces_cheveux, x_bridon'),'note carries the targeted actions, got '+note);
  const runs=await api('saya_runs');
  const mesRuns=runs.filter(r=>String(r.sourcePath||'').includes('casting/u1-'));
  assert(mesRuns.length===2,'two casting runs started, got '+mesRuns.length);
  const stems=mesRuns.map(r=>String(r.casting?.shot?.stem)).sort().join(' ');
  assert(stems.includes('bny-00001-stub-oc-noobai')&&stems.includes('bny-00002-stub-oc-illustrious'),'run manifests carry the stub stems');
  for(const r of mesRuns){
    assert(Object.keys(r.phases??{}).length===1&&r.phases['1'],'run '+r.id.slice(0,8)+' : phase 1 seule');
    assert(r.phases['1'].state==='succeeded'&&r.phases['1'].asset,'run '+r.id.slice(0,8)+' : image de passe 1');
  }
  report.checks.push('retombée : Lancer → 2 shots passe 1 réussis, note = « actions ciblées : x_pinces_cheveux, x_bridon »');
  report.expected={actions:['x_pinces_cheveux','x_bridon'],runs:mesRuns.map(r=>r.id)};
  report.capture=await invoke('smoke_capture',{name:'casting-actions-v2'});
  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
