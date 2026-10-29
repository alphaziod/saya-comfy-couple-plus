(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke, api = (action, args = {}) => invoke('studio', {request: {action, ...args}});
  const report = {ok: false, checks: []};
  try {
    const saved = window.__GENERATION_EXPECTED__, jobs = await api('jobs');
    if (saved.length !== 5 || jobs.length !== 5) throw Error('Job count changed after restart');
    for (const item of saved) {
      const job = jobs.find(j => j.id === item.job);
      if (job.state !== item.state || (await api('job_details', {id: item.job})).requestJson !== item.requestJson) throw Error('Historical state/snapshot changed');
      if(item.checkpoint){
        const records=await api('checkpoints',{job:job.id});
        if(records.length!==1||records[0].status!=='validated'||records[0].manifestJson!==item.checkpoint.manifestJson||records[0].resumeCapability!=='unverified')throw Error('Checkpoint changed after restart');
        await api('verify_checkpoint',{id:records[0].id});
      }
      if (job.state === 'succeeded') {
        const page=await api('gallery', {filters: {characters: item.people, variant: item.variant, job: job.id}});
        if (page.total !== 1) throw Error('Lost output association: '+JSON.stringify({job:job.id,people:item.people,variant:item.variant,page}));
        const src = await api('image', {id: job.outputs[0].assetId, preview: true});
        const image = new Image(); image.src = src; await image.decode();
        if (image.naturalWidth !== 1600) throw Error('Restored image cannot decode');
      }
    }
    if ((await invoke('smoke_actions')).includes('submit')) throw Error('Restart submitted a job');
    report.checks.push({jobs: 5, outputs: 3, immutableSnapshotsAfterVariantEdit: true, associationsPreserved: true, imagesDecoded: true, noResubmission: true}); report.ok = true;
  } catch (e) {report.failure = String(e?.message ?? e);}
  await invoke('finish_smoke', {report});
})();
