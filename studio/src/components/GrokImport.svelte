<script lang="ts">
  import { t } from '../lib/i18n';
 import {invoke} from '@tauri-apps/api/core';
 import {call,message,type Character} from '../lib/api';
 import Icon from './Icon.svelte';
 let {onimported=()=>{}}:{onimported?:()=>void}=$props();
 interface Entry{path:string;sha256:string;key:string;name:string;sourceId:string;identity:Record<string,unknown>;blocks:Record<string,string>;variants:{name:string;blocks:Record<string,string>}[];warnings:string[];duplicate:boolean;suggested:string|null}
 let opened=$state(false),paths=$state(''),entries=$state<Entry[]>([]),warnings=$state<string[]>([]),characters=$state<Character[]>([]),selected=$state<Record<string,boolean>>({}),targets=$state<Record<string,string>>({}),busy=$state(false),error=$state(''),report=$state(''),page=$state(0);
 const key=(e:Entry)=>e.path+'|'+e.key;
 const chosen=$derived(entries.filter(e=>selected[key(e)]&&!e.duplicate));
 function dialog(node:HTMLDialogElement){node.showModal()}
 async function browse(){error='';try{const files=await invoke<string[]>('pick_data_files');if(files.length)paths=[...new Set([...paths.split('\n').filter(Boolean),...files])].join('\n');}catch(e){error=message(e)}}
 async function directory(){error='';try{const dirs=await invoke<string[]>('pick_directories');const files=await call<string[]>('grok_directory_files',{paths:dirs});if(files.length)paths=[...new Set([...paths.split('\n').filter(Boolean),...files])].join('\n');}catch(e){error=message(e)}}
 async function preview(){if(busy)return;busy=true;error='';report='';try{const result=await call<{entries:Entry[];warnings:string[]}>('grok_preview',{paths:paths.split('\n').map(p=>p.trim()).filter(Boolean)});entries=result.entries;warnings=result.warnings;characters=await call<Character[]>('characters');selected={};targets={};page=0;}catch(e){error=message(e)}finally{busy=false}}
 async function apply(){if(busy)return;busy=true;error='';try{const choices=chosen.map(e=>({path:e.path,sha256:e.sha256,key:e.key,mode:targets[key(e)]?'variant':'create',character:targets[key(e)]||null}));const r=await call<{imported:unknown[];skipped:number}>('grok_import',{choices,confirmed:true});report=$t("{imported} importées · {skipped} doublons ignorés",{imported:r.imported.length,skipped:r.skipped});for(const e of chosen)e.duplicate=true;selected={};onimported();}catch(e){error=message(e)}finally{busy=false}}
</script>
<button data-grok-open onclick={()=>opened=true}>{$t("Importer des personnages")}</button>
{#if opened}<dialog class="grok-dialog" use:dialog onclose={()=>opened=false} aria-label={$t("Importer des personnages")}><div class="grok-head"><strong>{$t("Importer des personnages")}</strong><span class="grow"></span><button class="icon-btn quiet" disabled={busy} aria-label={$t("Fermer")} title={$t("Fermer")} onclick={()=>opened=false}><Icon name="close" /></button></div>
<div class="grok-body">
<div class="row" title={$t("Exports JSON observés : characters_ready, lots p1/anatomy, valides et persos. Les scripts ne sont jamais exécutés. Les noms identiques ne sont pas fusionnés.")}><button disabled={busy} onclick={browse}>{$t("Choisir des fichiers…")}</button><button disabled={busy} onclick={directory}>{$t("JSON d’un dossier…")}</button></div>
<label>{$t("Fichiers (un chemin par ligne)")}<textarea data-grok-paths rows="3" bind:value={paths}></textarea></label><div class="row"><button data-grok-analyze disabled={busy||!paths.trim()} onclick={preview}>{$t("Analyser les fichiers")}</button></div>
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if report}<p class="report" data-grok-report>{report}</p>{/if}
{#if warnings.length}<details class="sub"><summary>{$t("{n} diagnostics / éléments ignorés",{n:warnings.length})}</summary>{#each warnings as warning}<p>{warning}</p>{/each}</details>{/if}
{#if entries.length}<div class="row list-bar"><span class="num">{$t("{total} définitions · {chosen} sélectionnées",{total:entries.length,chosen:chosen.length})}</span><span class="grow"></span><button class="quiet" disabled={busy} onclick={()=>selected=Object.fromEntries(entries.filter(e=>!e.duplicate).map(e=>[key(e),true]))}>{$t("Sélectionner toutes les nouvelles")}</button><button class="quiet" onclick={()=>selected={}}>{$t("Tout désélectionner")}</button></div>
<div class="entries">{#each entries.slice(page*50,page*50+50) as entry}<section class="grok-entry"><div class="entry-line"><label class="checkbox"><input data-grok-entry type="checkbox" disabled={entry.duplicate||busy} bind:checked={selected[key(entry)]}/>{entry.name} · {entry.sourceId}{entry.duplicate?' · '+$t("déjà importé"):''}</label><select data-grok-target aria-label={$t("Destination")} title={$t("Destination")} disabled={entry.duplicate||busy} bind:value={targets[key(entry)]}><option value="">{$t("Créer une nouvelle fiche")}</option>{#each characters as character}<option value={character.id}>{$t("Nouvelle variante de {name}",{name:character.name})}{entry.suggested===character.id?' · '+$t("même source"):''}</option>{/each}</select></div><small class="path">{entry.path} · {entry.key}</small>
<details class="sub"><summary>{$t("Prompts, anatomie et {n} variantes de décor",{n:entry.variants.length})}</summary>{#each Object.entries(entry.blocks) as [section,text]}<strong>{section}</strong><pre>{text}</pre>{/each}<strong>{$t("Anatomie")}</strong><pre>{String(entry.identity.anatomy_prompt??'')}</pre>{#each entry.variants as variant}<details class="sub"><summary>{variant.name}</summary>{#each Object.entries(variant.blocks) as [role,text]}<strong>{role}</strong><pre>{text}</pre>{/each}</details>{/each}{#each entry.warnings as warning}<p class="muted">{warning}</p>{/each}</details></section>{/each}</div>
<div class="row pager"><button class="quiet" disabled={page===0||busy} onclick={()=>page--}>{$t("Page précédente")}</button><span class="num muted">{page+1} / {Math.ceil(entries.length/50)}</span><button class="quiet" disabled={(page+1)*50>=entries.length||busy} onclick={()=>page++}>{$t("Page suivante")}</button></div>{/if}
</div>
{#if entries.length}<div class="grok-foot"><span class="muted foot-note">{$t("Confirmer crée des fiches ou ajoute des variantes selon chaque destination. Aucune fiche existante ni aucun fichier source n’est écrasé.")}</span><button data-grok-confirm class="primary" disabled={busy||!chosen.length} onclick={apply}>{$t("Confirmer l’import ({n})",{n:chosen.length})}</button></div>{/if}
</dialog>{/if}
<style>
 .grok-dialog{width:min(960px,95vw);max-height:88vh;padding:0;display:flex;flex-direction:column;background:var(--panel);color:var(--text);border:1px solid var(--line-strong);border-radius:var(--radius-lg)}
 .grok-dialog::backdrop{background:#000a}
 .grok-head{display:flex;align-items:center;gap:8px;min-height:44px;padding:0 8px 0 14px;border-bottom:1px solid var(--line);flex:none}.grok-head strong{font-size:13px;font-weight:600}
 .grok-body{flex:1;min-height:0;overflow:auto;padding:12px 14px;display:grid;gap:8px;align-content:start}
 .grok-foot{display:flex;align-items:center;gap:12px;padding:10px 14px;border-top:1px solid var(--line);flex:none}.foot-note{flex:1;font-size:11.5px}
 .grow{flex:1}
 .row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}
 label{display:flex;flex-direction:column;gap:4px;margin:0;color:var(--muted);font-size:12px}
 label.checkbox{flex-direction:row;align-items:center;gap:6px;color:var(--text);min-width:0}
 input[type=checkbox]{width:auto;min-height:0;margin:0}
 textarea{width:100%}
 .report{margin:0;color:var(--ok)}
 .entries{border-top:1px solid var(--line)}
 .grok-entry{padding:6px 0;border-bottom:1px solid var(--line);display:grid;gap:2px}
 .entry-line{display:flex;align-items:center;gap:10px}.entry-line label{flex:1}.entry-line select{width:240px;flex:none}
 .path{font-size:11px;color:var(--faint);overflow-wrap:anywhere}
 details{margin:0}.sub>summary{color:var(--muted);font-size:12px}
 strong{font-size:11px;font-weight:600;color:var(--muted)}
 pre{font-size:11.5px;max-height:none;margin:2px 0 6px;padding:6px 8px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius)}
 .pager{justify-content:center}
</style>
