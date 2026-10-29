<script lang="ts">
  import { t } from '../lib/i18n';
 import {call,message,type Job} from '../lib/api';
 let {job}:{job:Job}=$props();
 interface Checkpoint {manifestJson:string;id:string;status:string;resumeCapability:string;phase:string;attempt:number;manifest:{file:string;kind:string;bytes:number;seed:string;resumeReason:string}}
 let list=$state<Checkpoint[]>([]),busy=$state(false),error=$state(''),note=$state(''),budget=$state(1024),preview=$state(''),selected=$state('');
 async function load(){try{list=await call('checkpoints',{job:job.id});}catch(e){error=message(e);}}
 async function capture(asset:string){if(busy)return;busy=true;error='';note='';try{await call('capture_checkpoint',{job:job.id,asset,max_bytes:budget*1024*1024});await load();note=$t("Copie IMAGE conservée. Aucune reprise de latent ou de sampler activée.");}catch(e){error=message(e);}finally{busy=false;}}
 async function verify(id:string){if(busy)return;busy=true;error='';note='';preview='';selected=id;try{await call('verify_checkpoint',{id});preview=await call<string>('checkpoint_image',{id});note=$t("Fichier conservé et intégrité vérifiés. Compatibilité de reprise non qualifiée.");}catch(e){error=message(e);}finally{busy=false;}}
 async function review(id:string,accept:boolean){if(busy)return;busy=true;error='';try{await call('review_checkpoint',{id,accept,note:''});await load();note=accept?$t("Image validée ; la compatibilité de reprise reste à qualifier."):$t("Image rejetée ; son fichier est conservé.");}catch(e){error=message(e);}finally{busy=false;}}
</script>
<details class="cp" data-job-checkpoints={job.id} ontoggle={e=>{if(e.currentTarget.open)load();}}><summary title={$t("Conserve une copie exacte d’une sortie SaveImage déjà récupérée, avec ses paramètres et dépendances. Un PNG ne contient pas les états latent, conditioning ou sampler. Aucune génération n’est lancée.")}>{$t("Checkpoints de passes · IMAGE")}</summary>
<div class="cp-body">
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}
{#if job.state==='succeeded'&&job.outputs.length}<label class="field">{$t("Budget maximal du stockage des checkpoints (Mio)")}<input class="num" type="number" min="1" max="16777216" step="1" bind:value={budget} disabled={busy} /></label>
<div class="row">{#each job.outputs as output,index}<button data-capture-checkpoint={output.assetId} disabled={busy||!Number.isSafeInteger(budget)||budget<1||budget>16777216} onclick={()=>capture(output.assetId)}>{$t("Conserver le résultat {n}",{n:index+1})}</button>{/each}</div>{/if}
{#each list as item (item.id)}<section class="item" data-checkpoint={item.id}><h4>{item.phase} · {$t("tentative {n}",{n:item.attempt})} · {item.status}</h4><p class="note num" title={item.manifest.resumeReason}>{$t("{n} octets",{n:item.manifest.bytes})} · seed {item.manifest.seed}</p><div class="row"><button class="quiet" disabled={busy} onclick={()=>verify(item.id)}>{$t("Vérifier et prévisualiser la copie")}</button><button class="quiet" disabled={busy||item.status==='validated'} onclick={()=>review(item.id,true)}>{$t("Valider cette image")}</button><button class="quiet" disabled={busy||item.status==='rejected'} onclick={()=>review(item.id,false)}>{$t("Rejeter cette image")}</button></div>
{#if selected===item.id&&preview}<img class="checkpoint-preview" src={preview} alt={$t("Copie du checkpoint vérifiée")} />{/if}
<details><summary>{$t("Manifeste conservé")}</summary><pre>{item.manifestJson}</pre></details></section>{/each}
{#if !list.length}<p class="note">{$t("Aucun checkpoint conservé pour cette génération.")}</p>{/if}
</div>
</details>
<style>
details{margin:0}summary{color:var(--muted);font-size:11.5px;cursor:pointer}
.cp-body{display:grid;gap:6px;padding:6px 0 2px}
.field{display:flex;flex-direction:column;gap:4px;color:var(--muted);font-size:11.5px}
.row{display:flex;flex-wrap:wrap;gap:2px}
.row button{min-height:24px;font-size:11.5px;padding:0 8px}
.item{display:grid;gap:4px;border-top:1px solid var(--line);padding-top:6px}
h4{margin:0;font-size:12px;font-weight:600}
.error{color:var(--danger);margin:0;font-size:11.5px;overflow-wrap:anywhere}.note{color:var(--muted);margin:0;font-size:11.5px;overflow-wrap:anywhere}
pre{font-size:11px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:4px 0 0;max-height:220px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}
.checkpoint-preview{width:100%;max-height:420px;object-fit:contain;background:var(--canvas);border-radius:var(--r-img)}
</style>
