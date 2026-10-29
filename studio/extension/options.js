const token=document.getElementById('token'),dest=document.getElementById('dest'),msg=document.getElementById('msg'),native=document.getElementById('native');
chrome.storage.local.get(['dest']).then(v=>{dest.value=v.dest||'';});
const labels={native:'fourni par Saya Studio (Native Messaging)',options:'jeton manuel enregistré (secours)'};
async function status(){
  try{
    const r=await chrome.runtime.sendMessage({type:'saya-bridge',action:'token-status'});
    if(r?.ok&&r.source==='native') native.textContent='actif — '+labels.native;
    else if(r?.ok) native.textContent='hôte indisponible — '+labels.options;
    else native.textContent='hôte indisponible — aucun jeton (saisir le jeton de secours)';
  }catch(e){native.textContent='hôte indisponible — '+(e.message||e);}
}
status();
document.getElementById('save').onclick=async()=>{
  const patch={dest:dest.value.trim()};
  const value=token.value.trim();
  if(value) patch.token=value;
  await chrome.storage.local.set(patch);
  token.value='';
  msg.textContent=value?'Enregistré (jeton de secours).':'Enregistré.';
  await status();
};
document.getElementById('clear').onclick=async()=>{
  await chrome.storage.local.remove('token');
  token.value='';
  msg.textContent='Jeton enregistré supprimé.';
  await status();
};
document.getElementById('test').onclick=async()=>{
  msg.textContent='Test en cours…';
  try{
    const r=await chrome.runtime.sendMessage({type:'saya-bridge',action:'health'});
    msg.textContent=r?.ok?'Bridge joignable, jeton accepté.':'Bridge refusé : '+(r?.error||'réponse invalide');
  }catch(e){msg.textContent='Bridge injoignable : '+(e.message||e);}
  await status();
};
