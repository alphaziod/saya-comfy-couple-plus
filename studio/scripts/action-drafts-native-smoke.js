(async()=>{
 // Casting "Brouillons": save a draft, test it in the casting grid, YES → real bank, delete another.
 const report={ok:false,checks:[]};const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(150)}};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const click=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const check=(name,ok,detail)=>{report.checks.push({name,ok,detail});if(!ok)throw Error(name+' '+JSON.stringify(detail??''))};
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await pause(500)}
  document.querySelector('[data-nav="generation"]').click();await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await click('[data-workflow-open]');
  await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'session');
  document.querySelector('[data-nav="casting"]').click();
  await wait(()=>document.querySelector('[data-casting-source]')?.textContent.includes('Config consommée'),'casting source');
  const panel=document.querySelector('[data-action-drafts]');panel.closest('details').open=true;
  await wait(()=>document.querySelectorAll('[data-draft-famille] option').length===2,'families');
  check('empty list',!!document.querySelector('[data-draft-empty]'));
  // Refusal shown in the panel, nothing saved: existing real id.
  set('[data-draft-famille]','debout');set('[data-draft-id]','debout_base');set('[data-draft-add]','x');
  await click('[data-draft-save]');await wait(()=>document.querySelector('[data-draft-error]'),'duplicate refused');
  report.duplicateError=document.querySelector('[data-draft-error]').textContent;
  set('[data-draft-id]','bras_leves_test');set('[data-draft-add]','arms up, waving');
  await click('[data-draft-save]');await wait(()=>document.querySelector('[data-draft-check="bras_leves_test"]'),'draft listed');
  const overview=await api('action_drafts');
  check('sandbox ready',overview.sandbox===true);
  check('draft pending',overview.drafts.some(d=>d.id==='bras_leves_test'&&d.state==='draft'));
  // Test it in the casting: two trials drawn from the test copy, pose = the draft text.
  set('[data-draft-n]','2');await click('[data-draft-test]');
  await wait(async()=>(await api('casting_history',{sync:true})).filter(r=>r.asset).length>=2,'two trials',180000);
  const rows=await api('casting_history',{sync:false});
  report.rowKeys=Object.keys(rows[0]||{});
  report.poses=rows.map(r=>r.shot?.pose??r.pose??null);
  check('trials use the draft',report.poses.filter(p=>p==='arms up, waving').length>=2,report.poses);
  report.capture=await invoke('smoke_capture',{name:'action-drafts'});
  // A second draft, rejected.
  set('[data-draft-famille]','assise');set('[data-draft-id]','assise_jetee');set('[data-draft-add]','sitting');
  await click('[data-draft-save]');await wait(()=>document.querySelector('[data-draft-delete="assise_jetee"]'),'second draft');
  await click('[data-draft-delete="assise_jetee"]');await wait(()=>!document.querySelector('[data-draft-check="assise_jetee"]'),'deleted');
  // YES with confirmation.
  await click('[data-draft-yes="bras_leves_test"]');await click('[data-draft-confirm="bras_leves_test"]');
  await wait(()=>document.querySelector('[data-draft-promoted="bras_leves_test"]'),'promoted listed');
  report.promotedRow=document.querySelector('[data-draft-promoted="bras_leves_test"]').textContent;
  report.promotedNote=document.querySelector('[data-draft-note]')?.textContent??null;
  check('rollback command shown',report.promotedRow.includes('solo rollback --backup fake-bras_leves_test'),report.promotedRow);
  const after=await api('action_drafts');
  check('promoted',after.drafts.some(d=>d.id==='bras_leves_test'&&d.state==='promoted'));
  check('rejected gone',!after.drafts.some(d=>d.id==='assise_jetee'));
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;
  report.panelError=document.querySelector('[data-draft-error]')?.textContent??null;
  report.panelNote=document.querySelector('[data-draft-note]')?.textContent??null;
  report.castingError=[...document.querySelectorAll('.error')].map(n=>n.textContent).slice(0,3)}
 await invoke('finish_smoke',{report});
})();
