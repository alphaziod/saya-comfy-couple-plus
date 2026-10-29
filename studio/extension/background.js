const BRIDGE = 'http://127.0.0.1:47821';
const NATIVE_HOST = 'com.saya.studio';
let token = null;
let tokenSource = 'none';

// Le jeton vient d'abord de l'hôte Native Messaging installé par Saya Studio.
// Les options de l'extension restent un secours manuel explicite.
function nativeToken() {
  return new Promise(resolve => {
    try {
      chrome.runtime.sendNativeMessage(NATIVE_HOST, {type: 'get_token'}, reply => {
        const err = chrome.runtime.lastError;
        if (err || !reply || reply.ok !== true || typeof reply.token !== 'string') { resolve(null); return; }
        const value = reply.token.trim();
        resolve(value || null);
      });
    } catch (_e) { resolve(null); }
  });
}

async function ensureToken() {
  if (token) return token;
  const native = await nativeToken();
  if (native) {
    token = native;
    tokenSource = 'native';
    try { await chrome.storage.local.set({token: native}); } catch (_e) {}
    return token;
  }
  const stored = await chrome.storage.local.get(['token']);
  token = String(stored.token || '').trim();
  if (!token) throw new Error('Jeton Saya Studio absent : hôte Native Messaging indisponible — lancer Saya Studio, ou saisir le jeton dans les options de l’extension');
  tokenSource = 'options';
  return token;
}

async function bridge(path, init = {}) {
  const t = await ensureToken();
  const headers = Object.assign({'X-Saya-Token': t}, init.headers || {});
  const r = await fetch(BRIDGE + path, {...init, headers});
  if (!r.ok) { let j={}; try{j=await r.json()}catch{}; if(r.status===403) { token=null; tokenSource='none'; } throw new Error(j.error || ('HTTP '+r.status)); }
  return r.json();
}

function jobPath(action, job) {
  if (!/^[0-9a-f-]+$/i.test(String(job || ''))) throw new Error('Tâche invalide');
  return '/' + action + '/' + job;
}
chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (!msg || msg.type !== 'saya-bridge') return false;
  (async () => {
    try {
      if (msg.action === 'token-status') {
        try { await ensureToken(); sendResponse({ok:true, source:tokenSource}); }
        catch (e) { sendResponse({ok:false, source:'none', error:String(e.message||e)}); }
      }
      else if (msg.action === 'download') sendResponse({ok:true, data:await bridge('/download',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(msg.payload)})});
      else if (msg.action === 'status') sendResponse({ok:true, data:await bridge(jobPath('status', msg.job))});
      else if (msg.action === 'cancel') sendResponse({ok:true, data:await bridge(jobPath('cancel', msg.job),{method:'POST'})});
      else if (msg.action === 'pause') sendResponse({ok:true, data:await bridge(jobPath('pause', msg.job),{method:'POST'})});
      else if (msg.action === 'resume') sendResponse({ok:true, data:await bridge(jobPath('resume', msg.job),{method:'POST'})});
      else if (msg.action === 'jobs') sendResponse({ok:true, data:await bridge('/jobs')});
      else if (msg.action === 'health') sendResponse({ok:true, data:await bridge('/health')});
      else sendResponse({ok:false,error:'action inconnue'});
    } catch (e) { sendResponse({ok:false,error:String(e.message||e)}); }
  })();
  return true;
});
