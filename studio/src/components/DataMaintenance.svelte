<script lang="ts">
  import { t, tn } from '../lib/i18n';
 import {onMount} from 'svelte';
 import {call,message,native} from '../lib/api';
 let database=$state(''),destination=$state(''),source=$state(''),restoreTarget=$state('');
 let includePassStates=$state(false),estimate=$state<{estimatedBytes:number;files:{file:string;byteSize:number}[];externalReferences:string}|null>(null);
 async function estimateBackup(){if(busy)return;busy=true;error='';estimate=null;try{estimate=await call('backup_estimate',{include_pass_states:includePassStates});}catch(e){error=message(e);}finally{busy=false;}}
 let busy=$state(false),error=$state(''),note=$state(''),report=$state<unknown>(null);
 interface Dependencies{imagesChecked:number;missingImages:string[];missingCheckpoints:string[];missingPipelineStates:{run:string;step:string}[];externalModels:string[];libraries:{root:string;presentOnDisk:boolean;images:number;missingImages:number}[];complete:boolean;note:string}
 let deps=$state<Dependencies|null>(null);
 onMount(()=>{let alive=true;if(native)call<{database:string;suggestedBackup:string}>('data_paths').then(paths=>{if(alive){database=paths.database;destination=paths.suggestedBackup;}}).catch(e=>{if(alive)error=message(e);});return()=>{alive=false;};});
 async function execute(action:'backup_database'|'verify_backup'|'restore_backup'){
  if(busy)return;busy=true;error='';note='';report=null;deps=null;
  try{report=await call(action,action==='backup_database'?{destination,include_pass_states:includePassStates}:action==='verify_backup'?{directory:source}:{source,destination:restoreTarget});
   if(action==='backup_database'){source=destination;note=$t("Sauvegarde créée et vérifiée.");}
   else if(action==='verify_backup')note=$t("Empreinte et intégrité de la sauvegarde vérifiées.");
   else note=$t("Copie restaurée et vérifiée dans le nouveau dossier. La base active reste inchangée.");
   if((report as {version?:number})?.version===1)note+=' '+$t("Ancien format SQLite seul : aucun fichier checkpoint inclus.");
   if(action!=='backup_database')deps=await call<Dependencies>('backup_dependencies',{directory:action==='verify_backup'?source:restoreTarget});
  }catch(e){error=message(e);}finally{busy=false;}
 }
</script>
<div class="data-page">
<header class="data-head">
 <span class="hash muted" title={$t("Les images originales et modèles restent à leur emplacement. Sauvegardez-les séparément. Les miniatures peuvent être recréées. Par défaut, la sauvegarde v3 contient SQLite : personnages, prompts, recettes, pipelines et historique. Les fichiers des états intermédiaires sont optionnels. Cette sauvegarde dépend toujours des images originales et modèles externes. Les formats v1/v2 restent lisibles.")}>{$t("Base active : {path}",{path:database||$t("chargement…")})}</span>
</header>
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="notice" role="status" data-backup-status>{note}</p>{/if}
{#if busy}<p class="muted" role="status">{$t("Vérification et copie en cours…")}</p>{/if}

<fieldset class="dsec" disabled={busy||!native}>
 <legend class="dtitle">{$t("Créer une sauvegarde")}</legend>
 <label class="frow" title={$t("Le dossier parent doit exister. Un dossier déjà présent n’est jamais écrasé. Un dossier incomplet après interruption n’est pas une sauvegarde validée.")}><span>{$t("Nouveau dossier de sauvegarde")}</span><input bind:value={destination} spellcheck="false" /></label>
 <label class="checkbox opt"><input type="checkbox" data-backup-states bind:checked={includePassStates} onchange={()=>{estimate=null;}} />{$t("Inclure les états de passes (checkpoints et fichiers gérés)")}</label>
 <div class="actions"><button class="primary" disabled={!destination.trim()||!database} onclick={()=>execute('backup_database')}>{$t("Créer la sauvegarde vérifiée")}</button><button class="quiet" data-backup-estimate disabled={!database} onclick={estimateBackup}>{$t("Estimer la taille")}</button></div>
 {#if estimate}<div class="estimate muted" data-backup-estimation><p class="num">{$t("Estimation : {size} Mio · {count} fichiers inclus (hors manifeste final).",{size:(estimate.estimatedBytes/1048576).toFixed(2),count:estimate.files.length})}</p><p>{estimate.externalReferences}</p><details><summary>{$t("Fichiers réellement inclus")}</summary><ul>{#each estimate.files as file}<li class="num">{file.file} · {$t("{n} octets",{n:file.byteSize})}</li>{/each}</ul></details></div>{/if}
</fieldset>

<fieldset class="dsec" disabled={busy||!native}>
 <legend class="dtitle">{$t("Vérifier une sauvegarde")}</legend>
 <label class="frow"><span>{$t("Dossier de sauvegarde existant")}</span><input bind:value={source} spellcheck="false" /></label>
 <div class="actions"><button disabled={!source.trim()} onclick={()=>execute('verify_backup')}>{$t("Vérifier la sauvegarde")}</button></div>
</fieldset>

<fieldset class="dsec" disabled={busy||!native}>
 <legend class="dtitle">{$t("Restaurer une sauvegarde")}</legend>
 <p class="frow static"><span>{$t("Sauvegarde source")}</span><code class="hash">{source.trim()||$t("Choisir le dossier dans « Vérifier une sauvegarde ».")}</code></p>
 <label class="frow" title={$t("Restauration dans un dossier neuf uniquement, sans remplacer la base active et sans relancer les générations. Les chemins d’images sont conservés ; la disponibilité des originaux dépend de leur emplacement.")}><span>{$t("Nouveau dossier de restauration")}</span><input bind:value={restoreTarget} spellcheck="false" /></label>
 <div class="actions"><button disabled={!source.trim()||!restoreTarget.trim()} onclick={()=>execute('restore_backup')}>{$t("Restaurer dans le nouveau dossier")}</button></div>
</fieldset>

{#if report}<details class="sec out" open><summary><span class="sec-title">{$t("Rapport d’intégrité")}</span></summary><div class="sec-body"><pre data-backup-report>{JSON.stringify(report,null,2)}</pre></div></details>{/if}
{#if deps}<details class="sec out" open data-backup-dependencies>
<summary title={deps.note}><span class="sec-title">{$t("Dépendances externes de cette copie")}</span><span class="chip">{deps.complete?$t("complètes"):$t("incomplètes")}</span></summary>
<div class="sec-body">
<p>{deps.complete?$t("Toutes les références internes vérifiées sont présentes."):$t("Références manquantes détectées : la copie reste restaurable, mais ces éléments ne sont pas disponibles.")}</p>
<ul class="deps num">
<li>{$t("Images originales vérifiées : {checked} · manquantes sur cette machine : {missing}",{checked:deps.imagesChecked,missing:deps.missingImages.length})}{deps.missingImages.length?` ${$t("(ex. {examples})",{examples:deps.missingImages.slice(0,3).join(', ')})}`:''}</li>
{#each deps.libraries as l}<li>{$t("Bibliothèque {root} : {state} · {images} image(s), {missing} manquante(s)",{root:l.root,state:l.presentOnDisk?$t("présente"):$t("absente"),images:l.images,missing:l.missingImages})}</li>{/each}
<li>{$t("Checkpoints gérés manquants : {n}",{n:deps.missingCheckpoints.length})}</li>
<li>{$t("États de pipeline terminés non inclus : {n}",{n:deps.missingPipelineStates.length})}{deps.missingPipelineStates.length?' '+$t("(sauvegarde sans états de passes : ces passes devront être ré-exécutées)"):''}</li>
<li>{$t("Modèles et LoRA référencés (jamais sauvegardés, à vérifier dans ComfyUI) : {list}",{list:deps.externalModels.length?deps.externalModels.join(' · '):$t("aucun")})}</li>
</ul>
<p class="muted">{deps.note}</p>
</div></details>{/if}
</div>

<style>
 .data-page{max-width:760px;display:grid}
 .data-head{display:flex;align-items:baseline;gap:12px;flex-wrap:wrap;padding-bottom:8px;border-bottom:1px solid var(--line)}
 .data-head .hash{font-size:11.5px;min-width:0}
 p{margin:0}
 .notice{background:none;border:0;padding:6px 0;color:var(--ok)}
 .error{padding:6px 0}
 .dsec{border:0;border-bottom:1px solid var(--line);margin:0;padding:12px 0;display:grid;gap:8px;min-width:0}
 .dtitle{float:left;width:100%;padding:0;margin:0 0 4px;font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted)}
 .dtitle+*{clear:both}
 .frow{display:grid;grid-template-columns:200px minmax(0,1fr);align-items:center;gap:12px;flex-direction:row;color:var(--text);font-size:12.5px;margin:0}
 .frow>span{color:var(--muted)}
 .frow input{width:100%}
 .frow.static code{color:var(--text);font-size:12px}
 .opt{margin-inline-start:212px;color:var(--text);font-size:12.5px}
 .actions{display:flex;gap:8px;margin-inline-start:212px}
 .estimate{margin-inline-start:212px;display:grid;gap:2px;font-size:12px}
 .estimate details{margin:0}.estimate ul{margin:4px 0;padding-inline-start:18px;max-height:200px;overflow:auto}
 .out{margin:0}
 .out>summary{padding:0}.out .sec-body{padding:2px 0 12px}
 .out pre{background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:8px;max-height:320px;margin:0}
 .deps{margin:0;padding:0;list-style:none}.deps li{padding:4px 0;border-top:1px solid var(--line);overflow-wrap:anywhere}
 @media(max-width:700px){.frow{grid-template-columns:1fr;gap:4px}.opt,.actions,.estimate{margin-inline-start:0}}
</style>
