<script lang="ts">
  import { t } from '../lib/i18n';
  import {onMount} from 'svelte';
  import {call,message,type Character,type Job} from '../lib/api';
  import Thumbnail from './Thumbnail.svelte';
  let {character,variant}:{character:Character;variant:string}=$props();
  interface Reference {assetId:string;variantId:string|null;primary:boolean;availability:string}
  let references=$state<Reference[]>([]),jobs=$state<Job[]>([]),error=$state(''),loading=$state(false),loaded=$state(false),more=$state(false),limit=$state(8),snapshot=$state('');
  let active=true;
  const shown=$derived(references.filter(r=>!r.variantId||r.variantId===variant));
  async function history(reset=false){
    if(loading)return;loading=true;error='';
    try{const page=await call<Job[]>('character_jobs',{character:character.id,before:reset?null:jobs.at(-1)?.id??null});if(active){jobs=reset?page:[...jobs,...page];more=page.length===100;loaded=true;}}
    catch(e){if(active)error=message(e);}finally{if(active)loading=false;}
  }
  async function inspect(job:Job){try{const details=await call<{requestJson:string}>('job_details',{id:job.id});if(active)snapshot=details.requestJson;}catch(e){if(active)error=message(e);}}
  onMount(()=>{call<Reference[]>('references',{character:character.id}).then(v=>{if(active)references=v;}).catch(e=>{if(active)error=message(e);});return()=>{active=false;};});
</script>
<div class="ctx" data-character-context={character.id}>
<details><summary title={$t("Informations de la fiche ; aucun ajout automatique aux prompts ou aux conditionings.")}>{$t("Fiche et références · {name}",{name:character.name})}</summary>
{#each [['description','Description'],['anatomy_prompt','Anatomie'],['anatomy_note','Note sur l’anatomie'],['studioTraits','Caractéristiques permanentes']] as [key,title]}{#if typeof character.identity[key]==='string' && character.identity[key]}<h4>{$t(title)}</h4><pre>{String(character.identity[key])}</pre>{/if}{/each}
<div class="ctx-grid">{#each shown.slice(0,limit) as ref (`${ref.variantId}:${ref.assetId}`)}<div data-context-reference={ref.assetId}><Thumbnail id={ref.assetId} alt={$t("Référence de {name}",{name:character.name})} /><p class="cap">{ref.variantId?$t("Variante sélectionnée"):$t("Fiche principale")} · {ref.primary?$t("principale"):$t("secondaire")}{ref.availability!=='present'?' · '+$t("fichier indisponible"):''}</p></div>{/each}</div>
{#if shown.length>limit}<button class="quiet" onclick={()=>limit+=8}>{$t("Afficher plus de références")}</button>{/if}
{#if !shown.length}<p class="muted">{$t("Aucune référence pour cette fiche ou cette variante.")}</p>{/if}
</details>
<details data-character-history ontoggle={e=>{if(e.currentTarget.open&&!loaded)history(true);}}><summary>{$t("Historique des préparations et générations")}</summary>
<button class="quiet" disabled={loading} onclick={()=>history(true)}>{$t("Actualiser cet historique")}</button>
{#each jobs as job (job.id)}<div class="job" data-context-job={job.id}><p><span>{job.workflow} · {job.state}</span> <span class="muted num">{job.createdAt}</span></p><button class="quiet" onclick={()=>inspect(job)}>{$t("Voir la requête conservée")}</button><div class="ctx-grid">{#each job.outputs as output}<Thumbnail id={output.assetId} />{/each}</div></div>{/each}
{#if loading}<p role="status">{$t("Chargement de l’historique…")}</p>{:else if loaded&&!jobs.length}<p class="muted">{$t("Aucune préparation associée.")}</p>{/if}
{#if more}<button class="quiet" disabled={loading} onclick={()=>history()}>{$t("Préparations plus anciennes")}</button>{/if}
{#if snapshot}<details open><summary>{$t("Requête historique exacte")}</summary><pre>{snapshot}</pre><button class="quiet" onclick={()=>snapshot=''}>{$t("Fermer cette requête")}</button></details>{/if}
</details>
{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<style>
 .ctx{display:grid;gap:4px;min-width:0}
 .ctx details{margin:0}.ctx summary{color:var(--muted);font-size:12px;min-height:26px;display:flex;align-items:center}
 h4{margin:6px 0 2px;font-size:11px;font-weight:600;color:var(--muted)}
 pre{font-size:11.5px;max-height:200px;margin:0;padding:6px 8px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius)}
 .ctx-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(72px,1fr));gap:6px;margin:4px 0}
 .ctx-grid :global(img),.ctx-grid :global(.image-placeholder){width:100%;aspect-ratio:3/4;height:auto;min-height:0;object-fit:cover;object-position:top;border-radius:var(--r-img);background:var(--raised)}
 .cap{margin:2px 0 0;font-size:10.5px;color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
 .job{padding:4px 0;border-bottom:1px solid var(--line)}.job p{margin:0;font-size:12px}
 .ctx button{min-height:24px;font-size:11.5px}
</style>
