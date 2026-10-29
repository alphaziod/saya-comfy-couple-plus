(async () => {
  // Requires the comfy-offline-native-smoke.py stub: every HTTP request is
  // answered 500 and every WebSocket upgrade is dropped (true refusal path).
  // Brief point 1: engine closed = ONE short message, no "error
  // sending request" spam, readable local state. Brief points 10/11: tree absent
  // from the navigation, readable workflows library.
  const report = { ok: false, checks: [], errors: [] };
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
const NAV={'Génération':'generation','Créer':'generation','Casting':'casting','Galerie':'gallery','Personnages':'characters','Persos':'characters','LoRA':'lora','Modèles':'lora','Scènes':'scenes','Prompts':'assistant','Workflows':'workflows','Galerie personnages':'people','Labo A/B':'ablab','Modules':'modules','Pipelines':'pipelines','Éditeur graphique':'graph','ComfyUI':'engine','Moteur':'engine','Données':'data','Paramètres':'settings','Réglages':'settings'};
const nav=async t=>{const id=NAV[t]??t;let b=document.querySelector(`[data-nav="${id}"]`);if(!b){const group={workflows:'workflows',graph:'workflows',pipelines:'workflows',modules:'workflows',scenes:'workflows',ablab:'workflows',assistant:'workflows',people:'characters'}[id];document.querySelector(`[data-nav="${group}"]`)?.click();await new Promise(r=>setTimeout(r,300));b=document.querySelector(`[data-nav="${id}"]`)}if(!b)throw Error('nav '+t);b.click()};
  window.addEventListener('error', event => report.errors.push(event.message));
  window.addEventListener('unhandledrejection', event => report.errors.push(String(event.reason)));
  try {
    assert(typeof invoke === 'function', 'Native invoke missing');
    // Local state kept: reading generations answers while the engine is down.
    const jobs = await api('jobs');
    assert(Array.isArray(jobs), 'jobs must stay readable while offline');

    // Workflows library (point 11): name, mapping, last use.
    const graph = `{"1":{"class_type":"Text","inputs":{"text":"portrait"}}}`;
    await api('import_workflow', { name: 'Offline Bibliothèque', text: graph });
    await nav('Workflows');
    await wait(() => document.querySelector('[data-workflow-library]'), 'workflow library table missing');
    const row = document.querySelector('[data-workflow-row]');
    assert(row?.querySelector('[data-workflow-name="Offline Bibliothèque"]'), 'library row name missing');
    assert(document.querySelector('[data-workflow-library]').textContent.includes('À configurer'), 'mapping column missing');
    assert(document.querySelector('[data-workflow-last-use]')?.textContent.trim() === 'jamais', 'last-use column missing');
    report.checks.push({ library: 'nom + mapping + dernière utilisation visibles' });

    // Tree removed from the navigation (point 10).
    assert(![...document.querySelectorAll('[data-nav]')].some(n => n.textContent.trim() === 'Arbre'), 'Arbre nav must be gone');
    assert(!document.body.textContent.includes('Arbre des générations'), 'tree page must be gone');

    // Offline (point 1): ONE short message, never the raw reqwest text.
    await nav('ComfyUI');
    await sleep(500); // let the navigation flush replace the previous page
    await wait(() => document.querySelector('[data-offline-note]'), 'offline note missing', 15000);
    const notes = document.querySelectorAll('[data-offline-note]');
    assert(notes.length === 1, 'expected exactly one offline note, got ' + notes.length);
    const attendu = "ComfyUI hors ligne — les essais restent en file, reprise dès qu'il revient.";
    assert(notes[0].textContent.trim() === attendu, 'offline note text wrong: ' + notes[0].textContent.trim());
    await sleep(6000); // let any spam latency show up
    assert(!document.body.textContent.includes('error sending request'), 'raw reqwest error leaked into UI');
    assert(!document.body.textContent.includes('Récupération HTTP'), 'raw http error banner shown while offline');
    const state = await api('live_state');
    assert(state.connected === false, 'live state must report offline');
    assert(typeof state.offlineNote === 'string' && state.offlineNote.length > 0, 'offlineNote missing in live state');
    assert(state.httpError, 'http diagnostic must stay preserved for diagnosis');
    report.checks.push({ offlineNote: notes[0].textContent.trim(), single: true, httpDiagnosticPreserved: true });
    assert(report.errors.length === 0, 'WebView errors: ' + report.errors.join('; '));
    report.ok = true;
  } catch (error) {
    report.failure = String(error?.message || error) + (error?.stack ? '\n' + error.stack : '');
    report.uiErrors = [...document.querySelectorAll('.error')].map(node => node.textContent);
    try {
      report.debug = {
        notePresent: !!document.querySelector('[data-offline-note]'),
        noteCount: document.querySelectorAll('[data-offline-note]').length,
        h1: document.querySelector('h1')?.textContent,
        liveSection: [...document.querySelectorAll('main p')].map(p => ({ cls: p.className, attrs: [...p.attributes].map(a => a.name).join(','), text: p.textContent.slice(0, 80) })),
        liveState: await api('live_state')
      };
    } catch (e) { report.debugError = String(e); }
  }
  window.__SMOKE_STAGE = report;
  if (typeof invoke === 'function') await invoke('finish_smoke', { report }).catch(() => {});
})();
