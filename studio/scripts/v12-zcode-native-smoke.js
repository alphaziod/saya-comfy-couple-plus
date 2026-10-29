(async () => {
  const report = { ok: false, phase: window.__V12_SMOKE_PHASE__ || 'initial', checks: [], errors: [] };
  window.__SMOKE_STAGE = report;
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  const api = (action, args = {}) => invoke('studio', { request: { action, ...args } });
  const assert = (ok, message) => { if (!ok) throw Error(message); };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const wait = async (predicate, message, timeout = 20000) => {
    window.__SMOKE_STEP = message + '@' + Date.now();
    const end = Date.now() + timeout;
    while (!await predicate()) { if (Date.now() >= end) throw Error(message); await sleep(50); }
  };
  const button = text => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text);
  const click = async text => { await wait(() => button(text) && !button(text).disabled, 'Button missing or disabled: ' + text); button(text).click(); };
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  const summary = text => [...document.querySelectorAll('summary')].find(s => s.textContent.trim() === text);
  const labeled = text => { const l = [...document.querySelectorAll('label')].find(x => x.textContent.trim().startsWith(text)); if (!l) throw Error('Label manquant : ' + text); return l.querySelector('input,textarea,select'); };
  const set = (el, value) => { el.value = value; el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); };
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    // The fixture database is created at schema v9: the app migrated it on open.
    const bootstrap = await invoke('get_bootstrap');
    assert(bootstrap.schemaVersion >= 16, 'Schema not migrated (>= 16 expected): ' + bootstrap.schemaVersion);
    report.checks.push({ migratedFrom: 9, schemaVersion: bootstrap.schemaVersion });
    await wait(() => document.querySelector('[data-nav="gallery"][aria-current="page"]'), 'Initial gallery missing');
    await click('Indexer / actualiser');
    await wait(async () => (await api('gallery', { filters: {} })).total === 130 && !(await api('index_status')).running, 'Expected 130 indexed fixture images', 90000);

    // Visual search from the gallery dialog: alpha images share one tint.
    await wait(() => document.querySelector('.image-card img'), 'Gallery thumbnails missing');
    document.querySelector('.image-card').click();
    await wait(() => document.querySelector('[data-find-similar]'), 'Inspector missing');
    document.querySelector('[data-find-similar]').click();
    await wait(() => document.querySelector('[data-visual-search]') && document.querySelectorAll('[data-visual-match]').length > 0, 'No visual match shown', 30000);
    const matches = document.querySelectorAll('[data-visual-match]').length;
    assert(document.querySelector('[data-visual-caption]').textContent.includes('approximative'), 'Approximate caption missing');
    report.checks.push({ visualMatches: matches });
    document.querySelector('[data-visual-search]').close();
    await wait(() => !document.querySelector('[data-visual-search]'), 'Visual dialog did not close');

    // Each ZCode page opens without UI error.
    for (const [tab, pageId] of [['Scènes', 'scenes'], ['Modules', 'modules']]) {
      await nav(tab);
      await wait(() => document.querySelector(`.subtabs button.on[data-nav="${pageId}"]`), 'Page missing: ' + tab);
      await sleep(400);
      const errors = [...document.querySelectorAll('.error')].map(n => n.textContent).filter(Boolean);
      assert(!errors.length, tab + ' shows errors: ' + errors.join('; '));
      report.checks.push({ tab, page: pageId });
    }

    // A/B lab: real preparation path, never an execution.
    await api('import_workflow', { name: 'Smoke AB', text: JSON.stringify({ p1: { class_type: 'Text', inputs: { text: 'portrait' } } }) });
    await nav('Labo A/B');
    await wait(() => document.querySelector('.subtabs button.on[data-nav="ablab"]'), 'A/B page missing');
    set(labeled('Nom de la campagne'), 'Smoke campagne');
    const workflowSelect = labeled('Workflow de référence');
    await wait(() => workflowSelect.options.length > 1, 'Workflow option missing');
    set(workflowSelect, workflowSelect.options[1].value);
    set(labeled('Modèle'), 'model.safetensors');
    set(labeled('Seed de base'), '18446744073709551615');
    await click('Créer la campagne');
    await wait(() => [...document.querySelectorAll('h2')].some(h => h.textContent.trim() === 'Smoke campagne'), 'Campaign not created');
    for (const [slot, label, values] of [['cfg', 'Guidage', '[4.5, 7]'], ['sampler', 'Échantillonneur', '["euler","dpmpp_2m"]']]) {
      summary('Ajouter un paramètre (constant ou variante)').click();
      await wait(() => { try { return labeled('Réglage'); } catch { return false; } }, 'Parameter form missing');
      set(labeled('Réglage'), slot); set(labeled('Genre'), 'variant'); set(labeled('Libellé'), label); set(labeled('Valeurs (tableau JSON'), values);
      await click('Ajouter le paramètre');
      await wait(() => [...document.querySelectorAll('.ab-params li strong')].some(s => s.textContent.trim() === label), 'Parameter not added: ' + label);
    }
    await click('Préparer la grille (sans exécution)');
    await wait(() => document.querySelectorAll('.ab-cell').length === 4, 'Grid not prepared');
    await click('Comparer champ par champ');
    await wait(() => document.querySelector('[data-attribution]')?.textContent.includes('admissible'), 'Single-variable attribution missing');
    const diffB = labeled('Cellule B');
    set(diffB, diffB.options[3].value);
    await click('Comparer champ par champ');
    await wait(() => document.querySelector('[data-attribution]')?.textContent.includes('2 paramètres'), 'Multi-variable guard missing');
    assert(document.querySelector('[data-attribution]').textContent.includes('refus'), 'Guard does not refuse');
    const campaigns = await api('ab_campaigns');
    assert(campaigns.length === 1 && campaigns[0].cells === 4 && campaigns[0].parameters === 2, 'Campaign summary inconsistent');
    const bootstrapAfter = await invoke('get_bootstrap');
    assert(bootstrapAfter.counts.jobs === 0, 'A job was created: preparation must not submit');
    report.checks.push({ abGrid: 4, multiVariableGuard: true, jobsCreated: 0, seedU64Max: true });
    assert(document.documentElement.scrollWidth <= innerWidth, 'Horizontal overflow');
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
