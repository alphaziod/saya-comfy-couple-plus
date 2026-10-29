<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message, native, type LiveObservation } from '../lib/api';
  let { promptId=null,observed=null,onupdate=()=>{} }: {promptId?:string|null;observed?:LiveObservation|null;onupdate?:(value:LiveObservation)=>void}=$props();
  let connection=$state<LiveObservation|null>(null),error=$state('');let pending=false,alive=true;
  const live=$derived(promptId?observed:connection);
  const job=$derived(promptId?live?.jobs[promptId]:null);
  async function poll(){if(!native||pending)return;pending=true;try {const next=await call<LiveObservation|null>('live_state',{since:live?.revision??null});if(alive){if(next){connection=next;onupdate(next);}error='';}}catch(e){if(alive)error=message(e);}finally{pending=false;}}
  onMount(()=>{if(promptId)return;alive=true;poll();const timer=setInterval(poll,1000);return()=>{alive=false;clearInterval(timer);};});
</script>
{#if !promptId}
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if live}<p class="status" role="status" title={$t("Dernière récupération HTTP : {time}. Les aperçus sans identifiant de génération ne sont pas attribués.",{time:live.lastHttpSync?new Date(live.lastHttpSync*1000).toLocaleTimeString():$t("pas encore vérifiée")})}><span class="dot" class:off={!live.connected}></span>{$t("WebSocket : {state}. Reconnexions : {n}.",{state:live.connected?$t("connecté"):$t("déconnecté · nouvelle tentative dans {s} s",{s:live.retryMs/1000}),n:live.reconnects})}</p>{#if live.offlineNote}<p class="note" role="status" data-offline-note>{live.offlineNote}</p>{:else}{#if live.error}<p class="note">{live.error}</p>{/if}{#if live.httpError}<p class="error">{$t("Récupération HTTP : {error}",{error:live.httpError})}</p>{/if}{/if}{/if}
{:else if job}
<p class="note">{$t("Node actuel : {node}",{node:job.node??$t("aucun node en cours annoncé")})}</p>
{#if live?.connected && job.value!==null && job.max!==null}<label class="progress">{$t("Progression du node {node} · {value} / {max}",{node:job.node??'',value:job.value,max:job.max})}<progress value={job.value} max={job.max}></progress></label>{/if}
{#if job.error}<pre class="error">{JSON.stringify(job.error,null,2)}</pre>{/if}
{#if Object.keys(job.nodes).length}<details><summary>{$t("Progression des nodes annoncée par ComfyUI")}</summary>{#each Object.entries(job.nodes) as [id,node]}<p class="note num">{$t("Node {id}",{id})} · {node.state} · {node.value} / {node.max}</p>{/each}</details>{/if}
{#if job.preview}<img class="live-preview" src={job.preview} alt={$t("Aperçu intermédiaire de cette génération")} />{/if}
{/if}
<style>
.status{display:flex;align-items:center;gap:6px;margin:0;font-size:11.5px;color:var(--muted)}
.dot{width:7px;height:7px;border-radius:50%;background:var(--ok);flex:none}.dot.off{background:var(--faint)}
.note{color:var(--muted);margin:0;font-size:11.5px;overflow-wrap:anywhere}
.error{color:var(--danger);margin:0;font-size:11.5px;overflow-wrap:anywhere}
pre.error{background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;white-space:pre-wrap;max-height:200px;overflow:auto}
.progress{display:flex;flex-direction:column;gap:3px;font-size:11px;color:var(--muted)}
progress{width:100%;height:4px;accent-color:var(--accent)}
details{margin:0}summary{color:var(--muted);font-size:11.5px;cursor:pointer}
.live-preview{max-width:100%;max-height:360px;object-fit:contain;display:block;border-radius:var(--r-img);background:var(--canvas)}
</style>
