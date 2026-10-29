<script lang="ts">
  import { t } from '../lib/i18n';
  import {call,message,type Job} from '../lib/api';
  let {job}:{job:Job}=$props();
  let operation=$state(''),confirmed=$state(false),busy=$state(false),error=$state(''),note=$state('');
  let details=$state<{requestJson:string;events:unknown[]}|null>(null);
  const active=$derived(!!job.promptId&&['queued','running','submission_unknown'].includes(job.state));
  function select(value:string){operation=value;confirmed=false;note='';error='';}
  async function inspect(){if(busy)return;busy=true;error='';try{details=await call('job_details',{id:job.id});}catch(e){error=message(e);}finally{busy=false;}}
  async function control(){if(!confirmed||!operation)return;if(busy)return;busy=true;error='';try{const result=await call<{message:string}>('control_job',{id:job.id,operation,confirmed:true});note=result.message;details=await call('job_details',{id:job.id});}catch(e){error=message(e);}finally{busy=false;confirmed=false;operation='';}}
</script>
<div class="jc"><button class="quiet" disabled={busy} onclick={inspect}>{$t("Requête et journal")}</button>{#if active}<button class="quiet" disabled={busy||job.state==='running'} onclick={()=>select('cancel_queue')}>{$t("Retirer ce job de la file")}</button><button class="quiet" disabled={busy||job.state==='queued'} onclick={()=>select('interrupt')}>{$t("Interrompre ce job actif")}</button>{/if}</div>
{#if operation && active}<div class="confirm"><p>{operation==='interrupt'?$t("Arrêter le calcul de cette génération. Les résultats partiels ne garantissent pas une reprise."):$t("Retirer seulement cette génération en attente. Si elle démarre entre-temps, elle ne sera pas interrompue.")}</p><label class="check"><input type="checkbox" bind:checked={confirmed} />{$t("Je confirme cette action sur {id}.",{id:job.promptId??''})}</label><div class="jc"><button class="danger" disabled={busy||!confirmed} onclick={control}>{$t("Confirmer l’action ciblée")}</button><button class="quiet" disabled={busy} onclick={()=>select('')}>{$t("Fermer")}</button></div></div>{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}
{#if details}<details open><summary>{$t("Requête exacte conservée")}</summary><pre>{details.requestJson}</pre></details><details><summary>{$t("Journal · 100 derniers événements")}</summary><pre>{JSON.stringify(details.events,null,2)}</pre></details>{/if}
<style>
.jc{display:flex;flex-wrap:wrap;gap:2px;margin-inline-start:-8px}
.jc button{min-height:24px;font-size:11.5px;padding:0 8px}
.confirm{display:grid;gap:6px;border-inline-start:2px solid var(--line-strong);padding-inline-start:8px}
.confirm p{margin:0;color:var(--muted);font-size:11.5px}
.check{display:flex;flex-direction:row;align-items:center;gap:8px;font-size:12px;color:var(--text)}
.error{color:var(--danger);margin:0;font-size:11.5px;overflow-wrap:anywhere}.note{color:var(--muted);margin:0;font-size:11.5px}
details{margin:0}summary{color:var(--muted);font-size:11.5px;cursor:pointer}
pre{font-size:11px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:4px 0 0;max-height:220px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}
</style>
