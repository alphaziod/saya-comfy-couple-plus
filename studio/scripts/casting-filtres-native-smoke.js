(async()=>{
 // V3 native smoke: casting sorts/filters: "Cibler des personnages" window
 // filterable (family, checkpoint), "Cibler des actions" window filterable (family,
 // scoped LoRA), then filterable casting grid (family, checkpoint, LoRA from the
 // shot's lora_loaders) and reversible arrival sort. SAYA_ENGINE_DIR stub engine
 // (scripts/casting-filtres-stub-engine.py: 4 bny/kmn × noobai/illustrious ids with
 // distinct LoRAs), fake ComfyUI accepting /prompt: no GPU, never the
 // installed app, never the real engine.
 const report={ok:false,phase:window.__V12_SMOKE_PHASE__||'initial',checks:[],errors:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const assert=(v,m)=>{if(!v)throw Error(m)},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m)=>{window.__SMOKE_STEP=m;const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);assert(n,s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const texte=s=>{const n=document.querySelector(s);return n?n.textContent.trim():''};
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
 const tile=id=>document.querySelector(`[data-cibler-perso="${id}"]`);
 try{
  await wait(()=>document.querySelector('[data-nav]'),'nav');
  await nav('Génération');
  await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await pause(100);
  document.querySelector('[data-workflow-open]').click();
  await wait(()=>!document.querySelector('[data-workflow-open]').disabled,'workflow opened');
  await wait(()=>document.querySelector('[data-nav]'),'casting nav');
  await nav('Casting');
  await wait(()=>document.querySelector('[data-casting-cibler]'),'casting menu');

  // 1. Characters window: 4 fixture registry tiles, family + checkpoint filters.
  document.querySelector('[data-casting-cibler]').click();
  await wait(()=>document.querySelector('[data-cibler-fenetre]'),'cibler window opens');
  await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===4,'quatre tuiles du registre fixture');
  set('[data-cibler-filtre-famille]','bny');await pause(120);
  let vues=[...document.querySelectorAll('[data-cibler-perso]')].map(n=>n.getAttribute('data-cibler-perso'));
  assert(JSON.stringify(vues)===JSON.stringify(['CH--9101','CH--9102']),'famille bny → 2 tuiles, got '+JSON.stringify(vues));
  set('[data-cibler-filtre-model]','illustrious');await pause(120);
  vues=[...document.querySelectorAll('[data-cibler-perso]')].map(n=>n.getAttribute('data-cibler-perso'));
  assert(JSON.stringify(vues)===JSON.stringify(['CH--9102']),'bny ∧ illustrious → CH--9102 seul, got '+JSON.stringify(vues));
  set('[data-cibler-filtre-famille]','kmn');await pause(120);
  // kmn ∧ illustrious = CH--9104 (Golf): the intersection exists in the fixture,
  // it must yield THIS tile alone — then the search empties the grid.
  vues=[...document.querySelectorAll('[data-cibler-perso]')].map(n=>n.getAttribute('data-cibler-perso'));
  assert(JSON.stringify(vues)===JSON.stringify(['CH--9104']),'kmn ∧ illustrious → CH--9104 seul, got '+JSON.stringify(vues));
  set('[data-cibler-recherche]','zombie-rien');await pause(120);
  await wait(()=>!!document.querySelector('[data-cibler-grille] .empty-prompts'),'trois filtres sans résultat → ligne vide');
  set('[data-cibler-recherche]','');await pause(120);
  await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===1,'recherche levée : CH--9104 revient');
  set('[data-cibler-filtre-famille]','');set('[data-cibler-filtre-model]','');await pause(120);
  await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===4,'filtres levés : quatre tuiles');
  report.checks.push('fenêtre personnages : filtres famille ∧ checkpoint réels (CH--9104 seule), cas vide, retour complet');
  document.querySelector('[data-cibler-fermer]').click();
  await wait(()=>!document.querySelector('[data-cibler-fenetre]'),'cibler window closed');

  // 2. Actions window: 6 AVAILABLE actions under the CONVERGED A/B rule (slice D):
  // the invalidated y_vieux (valide:false) and x_duo (pas_solo) are removed — exactly
  // like the engine draw (eligible_names) — and every solo-drawable action clones its
  // _flaccide sister in memory (x_a, y_b, y_c → 3 sources + 3 sisters), family filters
  // (file) + scoped LoRA.
  document.querySelector('[data-casting-cibler-actions]').click();
  await wait(()=>document.querySelector('[data-actions-fenetre]'),'actions window open');
  await wait(()=>document.querySelectorAll('[data-action-item]').length===6,'six actions disponibles (3 sources + 3 sœurs flaccide), got '+document.querySelectorAll('[data-action-item]').length);
  assert(!document.querySelector('[data-action-item="y_vieux"]'),'valide:false no longer listed');
  assert(!document.querySelector('[data-action-item="x_duo"]'),'pas_solo no longer listed (règle convergée A/B)');
  set('[data-actions-filtre-famille]','x');await pause(120);
  vues=[...document.querySelectorAll('[data-action-item]')].map(n=>n.getAttribute('data-action-item'));
  assert(JSON.stringify(vues.sort())===JSON.stringify(['x_a','x_a_flaccide']),'famille de fichier x → action + sœur flaccide, got '+JSON.stringify(vues));
  set('[data-actions-filtre-famille]','');
  set('[data-actions-filtre-lora]','pole dancing_illustrious_V2.0');await pause(120);
  vues=[...document.querySelectorAll('[data-action-item]')].map(n=>n.getAttribute('data-action-item'));
  assert(JSON.stringify(vues.sort())===JSON.stringify(['x_a','x_a_flaccide']),'LoRA pole dancing → x_a et sa sœur (la sœur porte la LoRA de sa source), got '+JSON.stringify(vues));
  assert(document.querySelector('[data-action-item="x_a"]').getAttribute('data-lora')==='pole dancing_illustrious_V2.0','tuile porte data-lora');
  set('[data-actions-filtre-lora]','cute_v1.5');await pause(120);
  vues=[...document.querySelectorAll('[data-action-item]')].map(n=>n.getAttribute('data-action-item'));
  assert(JSON.stringify(vues.sort())===JSON.stringify(['y_b','y_b_flaccide']),'LoRA cute_v1.5 → y_b et sa sœur, got '+JSON.stringify(vues));
  set('[data-actions-filtre-lora]','');await pause(120);
  await wait(()=>document.querySelectorAll('[data-action-item]').length===6,'filtres actions levés');
  report.checks.push('fenêtre actions : indisponibles cachées, filtres famille ∧ LoRA portée réels, retour complet');
  document.querySelector('[data-actions-fermer]').click();
  await wait(()=>!document.querySelector('[data-actions-fenetre]'),'actions window closed');

  // 3. Target the 4 characters (window reopened: filters reset) then Launch.
  document.querySelector('[data-casting-cibler]').click();
  await wait(()=>document.querySelector('[data-cibler-fenetre]'),'cibler window reopens');
  for(const id of ['CH--9101','CH--9102','CH--9103','CH--9104']) tile(id).click();
  await wait(()=>document.querySelector('[data-cibler-compteur]').textContent.includes('4 sélectionnés'),'4 sélectionnés');
  document.querySelector('[data-cibler-valider]').click();
  await wait(()=>document.querySelector('[data-casting-ciblage]'),'chip ciblage');
  set('[data-casting-n]','4');await pause(100);
  document.querySelector('[data-casting-lancer]').click();
  await wait(()=>document.querySelectorAll('[data-casting-shot]').length===4,'quatre shots dans la grille');
  await wait(()=>[...document.querySelectorAll('[data-casting-shot]')].every(c=>c.getAttribute('data-state')==='succeeded'),'passe 1 réussie sur les 4');
  report.checks.push('ciblage 4 persos → 4 shots passe 1 réussis (stub moteur)');

  // 4. Grid: the cards carry family/model/loras, the filters separate them.
  const cartes=()=>[...document.querySelectorAll('[data-casting-shot]')];
  const stems=()=>cartes().map(c=>c.getAttribute('data-casting-shot'));
  assert(JSON.stringify(stems())===JSON.stringify(['kmn-00004-stub-v3-illustrious','kmn-00003-stub-v3-noobai','bny-01002-stub-v3-illustrious','bny-01001-stub-v3-noobai']),'ordre par défaut (plus récents d\'abord — createdAt chrono, dernier lancé en tête) : '+JSON.stringify(stems()));
  const delta=cartes().find(c=>c.getAttribute('data-casting-shot')==='bny-01001-stub-v3-noobai');
  assert(delta.getAttribute('data-famille')==='bny'&&delta.getAttribute('data-model')==='noobai','carte porte famille+model');
  assert(delta.getAttribute('data-loras')==='nyalia','carte porte ses LoRA (lora_loaders du shot), got '+delta.getAttribute('data-loras'));
  const fox=cartes().find(c=>c.getAttribute('data-casting-shot')==='kmn-00003-stub-v3-noobai');
  assert(fox.getAttribute('data-loras')==='nyalia,Matte_Skin_Illustrious_v4','fox porte deux LoRA, got '+fox.getAttribute('data-loras'));
  const golf=cartes().find(c=>c.getAttribute('data-casting-shot')==='kmn-00004-stub-v3-illustrious');
  assert(golf.getAttribute('data-loras')==='','golf sans LoRA');

  set('[data-casting-filtre-famille]','kmn');await pause(120);
  assert(stems().length===2&&stems().every(s=>s.startsWith('kmn')),'filtre famille kmn → 2 cartes');
  set('[data-casting-filtre-famille]','');
  set('[data-casting-filtre-model]','illustrious');await pause(120);
  assert(stems().length===2&&stems().every(s=>s.endsWith('illustrious')),'filtre checkpoint illustrious → 2 cartes');
  set('[data-casting-filtre-famille]','bny');await pause(120);
  assert(JSON.stringify(stems())===JSON.stringify(['bny-01002-stub-v3-illustrious']),'bny ∧ illustrious → une carte');
  set('[data-casting-filtre-famille]','');set('[data-casting-filtre-model]','');
  set('[data-casting-filtre-lora]','Matte_Skin_Illustrious_v4');await pause(120);
  assert(JSON.stringify(stems())===JSON.stringify(['kmn-00003-stub-v3-noobai']),'filtre LoRA Matte_Skin → fox seule');
  set('[data-casting-filtre-lora]','nyalia');await pause(120);
  assert(stems().length===2&&stems().includes('bny-01001-stub-v3-noobai')&&stems().includes('kmn-00003-stub-v3-noobai'),'filtre LoRA nyalia → delta+fox');
  assert(texte('[data-casting-filtre-note]').includes('2 affichés sur 4'),'note de filtre comptée, got '+texte('[data-casting-filtre-note]'));
  set('[data-casting-filtre-lora]','');
  await wait(()=>stems().length===4,'filtres grille levés');
  report.checks.push('grille : filtres famille ∧ checkpoint ∧ LoRA réels, note comptée');

  // 5. Reversed arrival sort: oldest first → the order flips.
  set('[data-casting-tri]','anciens');await pause(120);
  assert(JSON.stringify(stems())===JSON.stringify(['bny-01001-stub-v3-noobai','bny-01002-stub-v3-illustrious','kmn-00003-stub-v3-noobai','kmn-00004-stub-v3-illustrious']),'tri anciens : ordre inversé, got '+JSON.stringify(stems()));
  set('[data-casting-tri]','recents');await pause(120);
  assert(stems()[0]==='kmn-00004-stub-v3-illustrious','tri recents : retour au plus récent d\'abord');
  report.checks.push('tri de la grille : plus anciens d\'abord puis retour');

  assert(!report.errors.length,report.errors.join(';'));report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
