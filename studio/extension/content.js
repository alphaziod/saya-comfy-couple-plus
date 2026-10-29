(() => {
  const installed = new WeakSet();
  const labels = {queued:'En file', downloading:'Téléchargement', paused:'En pause', done:'Terminé', error:'Erreur', cancelled:'Annulé'};
  function modelIds(anchor) {
    const m = location.pathname.match(/\/models\/(\d+)/);
    const href = anchor?.href || '';
    const v = new URLSearchParams(location.search).get('modelVersionId')
      || location.pathname.match(/\/model-versions\/(\d+)/)?.[1]
      || href.match(/\/api\/download\/models\/(\d+)/)?.[1];
    const fileId = href.match(/[?&]fileId=(\d+)/)?.[1];
    return { modelId: m ? Number(m[1]) : null, versionId: v ? Number(v) : null, fileId: fileId ? Number(fileId) : null };
  }
  function findDownload() {
    return [...document.querySelectorAll('button,a')].find(el => /^\s*Download\b/i.test(el.textContent || '') && el.offsetParent !== null);
  }
  function etaText(seconds) {
    const s = Math.max(0, Math.round(seconds));
    return s >= 60 ? Math.floor(s/60) + ' min ' + String(s%60).padStart(2,'0') + ' s' : s + ' s';
  }
  function render(status) {
    const s = status || {};
    if (s.error) return 'Erreur : ' + s.error;
    const parts = [labels[s.state] || s.state || '…'];
    if (s.total && s.bytes != null) parts.push(Math.min(100, Math.round(s.bytes / s.total * 100)) + ' %');
    if (s.bytes != null) parts.push((s.bytes/1048576).toFixed(1) + ' Mo' + (s.total ? ' / ' + (s.total/1048576).toFixed(1) + ' Mo' : ''));
    if (s.speed) parts.push((s.speed/1048576).toFixed(2) + ' Mo/s');
    if (s.eta != null && s.eta > 0) parts.push('reste ' + etaText(s.eta));
    return parts.join(' · ');
  }
  function addUi(anchor) {
    if (!anchor || installed.has(anchor)) return;
    installed.add(anchor);
    const ids = modelIds(anchor);
    if (!ids.modelId) return;
    let job = null, timer = null, failures = 0;
    const row = document.createElement('div'); row.className = 'saya-civitai-row';
    const primary = document.createElement('button'); primary.className = 'saya-civitai-primary'; primary.textContent = 'Télécharger avec Saya Studio';
    const secondary = document.createElement('button'); secondary.className = 'saya-civitai-secondary'; secondary.textContent = 'Téléchargement normal';
    const pause = document.createElement('button'); pause.className = 'saya-civitai-secondary'; pause.textContent = 'Pause'; pause.hidden = true;
    const cancel = document.createElement('button'); cancel.className = 'saya-civitai-secondary'; cancel.textContent = 'Annuler'; cancel.hidden = true;
    const status = document.createElement('span'); status.className = 'saya-civitai-status';
    const send = (action, payload) => chrome.runtime.sendMessage({type:'saya-bridge', action, ...payload});
    const active = s => ['queued','downloading','paused'].includes(s);
    function apply(s) {
      status.textContent = render(s);
      const state = s?.state;
      if (active(state)) {
        cancel.hidden = false; pause.hidden = false;
        pause.textContent = state === 'paused' ? 'Reprendre' : 'Pause';
        primary.disabled = true;
      } else {
        cancel.hidden = true; pause.hidden = true;
        primary.disabled = state === 'done';
        if (state === 'error') primary.disabled = false;
        if (state === 'cancelled') primary.disabled = false;
      }
      if (['done','error','cancelled'].includes(state) && timer) { clearInterval(timer); timer = null; }
    }
    function poll() {
      timer = setInterval(async () => {
        if (!job) return;
        try {
          const rr = await send('status', {job});
          if (!rr?.ok) { failures += 1; status.textContent = 'Bridge injoignable : ' + (rr?.error || 'réponse invalide'); if (failures > 5 && timer) { clearInterval(timer); timer = null; } return; }
          failures = 0; apply(rr.data);
        } catch (e) { failures += 1; status.textContent = 'Bridge injoignable : ' + (e.message || e); }
      }, 1000);
    }
    primary.onclick = async () => {
      primary.disabled = true; status.textContent = 'Envoi à Saya Studio…';
      try {
        const {dest} = await chrome.storage.local.get(['dest']);
        const reply = await send('download', {payload:{modelId:ids.modelId, versionId:ids.versionId, fileId:ids.fileId, pageUrl:location.href, destination:dest||null}});
        if (!reply?.ok) throw new Error(reply?.error || 'Bridge refusé');
        const j = reply.data; if (!j || !j.job) throw new Error('Réponse bridge invalide');
        job = j.job; apply({state:'queued'}); poll();
      } catch (e) { status.textContent = 'Saya Studio indisponible — utilisez le téléchargement normal. ' + (e.message || e); primary.disabled = false; }
    };
    pause.onclick = async () => {
      try {
        const rr = await send(pause.textContent === 'Pause' ? 'pause' : 'resume', {job});
        if (!rr?.ok) throw new Error(rr?.error || 'refusé');
        apply(rr.data);
      } catch (e) { status.textContent = 'Pause impossible : ' + (e.message || e); }
    };
    cancel.onclick = async () => {
      try {
        const rr = await send('cancel', {job});
        if (!rr?.ok) throw new Error(rr?.error || 'refusé');
        apply(rr.data);
      } catch (e) { status.textContent = 'Annulation impossible : ' + (e.message || e); }
    };
    secondary.onclick = () => anchor.click();
    row.append(primary, secondary, pause, cancel, status);
    anchor.parentElement?.insertBefore(row, anchor.nextSibling);
    // La page ne doit pas garder un second bouton principal concurrent.
    anchor.style.display = 'none';
  }
  function scan() { const d = findDownload(); if (d) addUi(d); }
  const obs = new MutationObserver(scan); obs.observe(document.documentElement, {subtree:true, childList:true});
  scan();
})();
