<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message, type LiveObservation, type Job } from '../lib/api';
  import { phaseRows } from '../lib/generation-state';
  let { run, onpreview = (_image:string|null)=>{}, compact = false }: { run:any; onpreview?:(image:string|null)=>void; compact?:boolean } = $props();
  let live=$state<LiveObservation|null>(null),jobs=$state<Job[]>([]),error=$state('');
  let alive=true,pending=false;
  const phases=$derived(phaseRows(run));
  const complete=$derived(phases.filter(p=>p.state==='succeeded').length);
  const current=$derived(phases.find(p=>['submitted','queued','running','submitting'].includes(p.state)));
  const prompt=$derived(run?.promptId??jobs.find(j=>j.id===current?.job)?.promptId);
  const progress=$derived(prompt?live?.jobs[prompt]:null);
  const announced=$derived(Object.values(progress?.nodes??{}));
  const finished=$derived(announced.filter(n=>n.state==='finished').length);
  $effect(()=>{onpreview(progress?.preview??null)});
  async function poll(){if(pending)return;pending=true;try{const [observation,list]=await Promise.all([call<LiveObservation|null>('live_state',{since:live?.revision??null}),run?.promptId?Promise.resolve([] as Job[]):call<Job[]>('jobs')]);if(alive){if(observation)live=observation;jobs=list;error=''}}catch(e){if(alive)error=message(e)}finally{pending=false}}
  onMount(()=>{void poll();const timer=setInterval(poll,1000);return()=>{alive=false;clearInterval(timer)}});
</script>
{#if compact}
<div class="run-progress compact" data-run-progress title={[progress?.node?$t("Node en cours : {node}",{node:progress.node}):'',current&&!live?.connected?$t("Flux temps réel déconnecté · statut HTTP conservé."):''].filter(Boolean).join('\n')||undefined}>
  <progress class="hide" value={complete} max={phases.length} aria-label={$t("Workflow complet")}></progress>
  <small><span>{current?$t("Passe {n} en cours",{n:current.n}):run?.state==='completed'?$t("Terminée"):$t("Au repos")}{#if current&&announced.length}<span class="muted"> · {$t("{done}/{n} nodes annoncés",{done:finished,n:announced.length})}</span>{/if}</span><span class="num">{progress?.value!=null&&progress?.max?`${progress.value} / ${progress.max}`:'—'}</span></small>
  <progress class="hide" value={current?(announced.length?finished:undefined):run?.state==='completed'?1:0} max={announced.length||1}></progress>
  <progress value={progress?.value??0} max={progress?.max||1} aria-label={$t("Sampling")}></progress>
  {#if error}<small class="error">{error}</small>{/if}
</div>
{:else}
<div class="run-progress" data-run-progress>
  <label>{$t("Workflow complet")} <span>{$tn("{done} / {n} passe","{done} / {n} passes",phases.length,{done:complete})}</span><progress value={complete} max={phases.length}></progress></label>
  <label>{current?$t("Passe {n} en cours",{n:current.n}):$t("Passe")} <span>{current?(announced.length?$t("{done}/{n} nodes annoncés",{done:finished,n:announced.length}):$t("En attente des événements")):run?.state==='completed'?$t("Terminée"):$t("Au repos")}</span><progress value={current?(announced.length?finished:undefined):run?.state==='completed'?1:0} max={announced.length||1}></progress></label>
  <label>{$t("Sampling")} <span>{progress?.value!=null&&progress?.max?`${progress.value} / ${progress.max}`:'—'}</span><progress value={progress?.value??0} max={progress?.max||1}></progress></label>
  {#if progress?.node}<small>{$t("Node en cours : {node}",{node:progress.node})}</small>{/if}
  {#if current&&!live?.connected}<small>{$t("Flux temps réel déconnecté · statut HTTP conservé.")}</small>{/if}
  {#if error}<small class="error">{error}</small>{/if}
</div>
{/if}
<style>
.run-progress{display:grid;gap:6px;margin:8px 0}.run-progress label{display:grid;grid-template-columns:1fr auto;gap:3px;font-size:12px}.run-progress span,.run-progress small{color:var(--muted);font-size:11px}.run-progress progress{height:5px;width:100%;margin:0;grid-column:1/-1}
.run-progress.compact{margin:0;gap:3px;min-width:190px;max-width:260px}.compact small{display:flex;justify-content:space-between;gap:8px;white-space:nowrap;overflow:hidden}.compact small span{overflow:hidden;text-overflow:ellipsis}.compact .hide{display:none}
progress{appearance:none;-webkit-appearance:none;border:0;height:3px;background:var(--line-strong);border-radius:2px;overflow:hidden}progress::-webkit-progress-bar{background:var(--line-strong)}progress::-webkit-progress-value{background:var(--accent)}progress::-moz-progress-bar{background:var(--accent)}
</style>
