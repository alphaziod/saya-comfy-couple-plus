<script lang="ts">
  import { t } from '../lib/i18n';
 import {onMount} from 'svelte';
 import {invoke} from '@tauri-apps/api/core';
 import {call,message} from '../lib/api';
 let {onuse}:{onuse:(id:string,name:string,index:number,blocks:Record<string,string>)=>void}=$props();
 interface Entry{path:string;sha256:string;key:string;name:string;variants:{name:string;blocks:Record<string,string>;characterSourceId?:string}[];warnings:string[];duplicate:boolean}
 interface Summary{id:string;name:string;key:string;importedAt:string}
 let library=$state<Summary[]>([]),selected=$state(''),record=$state<Entry|null>(null),variant=$state(0);
 let opened=$state(false),paths=$state(''),entries=$state<Entry[]>([]),warnings=$state<string[]>([]),chosen=$state<Record<string,boolean>>({}),page=$state(0),busy=$state(false),error=$state(''),report=$state('');
 const key=(e:Entry)=>e.path+'|'+e.key;
 const selection=$derived(entries.filter(e=>chosen[key(e)]&&!e.duplicate));
 async function reload(){try{library=await call<Summary[]>('grok_scenes')}catch(e){error=message(e)}}
 async function choose(){record=null;variant=0;error='';if(!selected)return;try{const id=selected;const value=await call<Entry>('grok_scene',{id});if(selected===id)record=value}catch(e){error=message(e)}}
 async function browse(){try{const files=await invoke<string[]>('pick_data_files');if(files.length)paths=[...new Set([...paths.split('\n').filter(Boolean),...files])].join('\n')}catch(e){error=message(e)}}
 async function preview(){if(busy)return;busy=true;error='';report='';try{const r=await call<{entries:Entry[];warnings:string[]}>('grok_scene_preview',{paths:paths.split('\n').map(p=>p.trim()).filter(Boolean)});entries=r.entries;warnings=r.warnings;chosen={};page=0;}catch(e){error=message(e)}finally{busy=false}}
 async function apply(){if(busy)return;busy=true;error='';try{const ids=await call<string[]>('grok_scene_import',{choices:selection.map(e=>({path:e.path,sha256:e.sha256,key:e.key})),confirmed:true});report=$t("{n} scènes enregistrées",{n:ids.length});for(const e of selection)e.duplicate=true;chosen={};await reload();if(ids.length){selected=ids[0];await choose();}}catch(e){error=message(e)}finally{busy=false}}
 function dialog(node:HTMLDialogElement){node.showModal()}
 onMount(reload);
</script>
<section class="grok" data-grok-scenes>
 <header class="ghead"><span class="sec-title">{$t("Scènes Importer importées")}</span><span class="chip num">{library.length}</span><button class="small" data-grok-scenes-open onclick={()=>opened=true}>{$t("Importer des scènes…")}</button></header>
 <div class="gbody">
 {#if error}<p class="error" role="alert">{error}</p>{/if}
 <label class="stack">{$t("Scène et version source")}<select data-grok-scene-select bind:value={selected} onchange={choose}><option value="">{$t("Choisir…")}</option>{#each library as scene}<option value={scene.id}>{scene.name} · {scene.importedAt}</option>{/each}</select></label>
 {#if record}<p class="note ell" title={`${record.path} · ${record.key}`}>{record.path} · {record.key}</p>
 <label class="stack">{$t("Variante")}<select data-grok-scene-variant bind:value={variant}>{#each record.variants as v,i}<option value={i}>{v.name}</option>{/each}</select></label>
 <button data-grok-scene-use title={$t("Choisissez ensuite le workflow, P1 et P2 explicitement. Les prompts de personnage et l’anatomie ci-dessous servent de référence ; importez leur fiche/variante pour les utiliser.")} onclick={()=>onuse(selected,record!.name,variant,record!.variants[variant].blocks)}>{$t("Utiliser dans une nouvelle scène")}</button>
 <details><summary>{$t("Comparer les {n} variantes et consulter les prompts",{n:record.variants.length})}</summary><div class="variants">{#each record.variants as v}<article><h3>{v.name}</h3>{#if v.characterSourceId}<p class="note">{$t("Personnage source : {id}",{id:v.characterSourceId})}</p>{/if}{#each Object.entries(v.blocks) as [role,text]}<strong>{role}</strong><pre>{text}</pre>{/each}</article>{/each}</div></details>{#each record.warnings as warning}<p class="note">{warning}</p>{/each}{/if}
 </div>
</section>
{#if opened}<dialog class="grok-scene-dialog" use:dialog onclose={()=>opened=false} aria-label={$t("Importer des scènes Importer")}><div class="dhead"><strong>{$t("Importer des scènes Importer")}</strong><button class="quiet" disabled={busy} onclick={()=>opened=false}>{$t("Fermer")}</button></div>
<div class="dbody">
<div class="row" title={$t("JSON exportés et tables Python POLE_PERSO / POLE_STAGES / POLE_INTIMES. Analyse AST de données littérales uniquement ; scripts et générateurs jamais exécutés.")}><button disabled={busy} onclick={browse}>{$t("Choisir des fichiers…")}</button></div>
<label class="stack">{$t("Fichiers, un chemin par ligne")}<textarea data-grok-scene-paths rows="3" bind:value={paths}></textarea></label>
<div class="row"><button data-grok-scene-analyze disabled={busy||!paths.trim()} onclick={preview}>{$t("Analyser les scènes")}</button></div>
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if report}<p class="note" data-grok-scene-report>{report}</p>{/if}
{#if warnings.length}<details><summary>{$t("{n} diagnostics",{n:warnings.length})}</summary>{#each warnings as warning}<p class="note">{warning}</p>{/each}</details>{/if}
{#if entries.length}<div class="row"><span class="note num">{$t("{total} scènes · {chosen} sélectionnées",{total:entries.length,chosen:selection.length})}</span><span class="grow"></span><button class="quiet" onclick={()=>chosen=Object.fromEntries(entries.filter(e=>!e.duplicate).map(e=>[key(e),true]))}>{$t("Sélectionner toutes les nouvelles")}</button></div>
<div class="entries">{#each entries.slice(page*50,page*50+50) as e}<section class="entry"><label class="check"><input data-grok-scene-entry type="checkbox" disabled={e.duplicate||busy} bind:checked={chosen[key(e)]}/>{e.name}{e.duplicate?' · '+$t("déjà importée"):''}</label><details><summary>{$t("{n} variantes · {key}",{n:e.variants.length,key:e.key})}</summary>{#each e.variants as v}<h3>{v.name}</h3>{#each Object.entries(v.blocks) as [role,text]}<strong>{role}</strong><pre>{text}</pre>{/each}{/each}</details></section>{/each}</div>
<div class="row"><button class="quiet" disabled={!page||busy} onclick={()=>page--}>{$t("Page précédente")}</button><span class="num note">{page+1} / {Math.ceil(entries.length/50)}</span><button class="quiet" disabled={(page+1)*50>=entries.length||busy} onclick={()=>page++}>{$t("Page suivante")}</button><span class="grow"></span>
<button class="primary" data-grok-scene-confirm disabled={busy||!selection.length} title={$t("Chaque version source est conservée ; aucun fichier ni scène existante n’est écrasé.")} onclick={apply}>{$t("Confirmer les scènes ({n})",{n:selection.length})}</button></div>{/if}
</div>
</dialog>{/if}
<style>
.grok{border-top:1px solid var(--line)}
.ghead{display:flex;align-items:center;gap:6px;min-height:36px;padding:0 8px 0 12px}
.sec-title{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.gbody{display:grid;gap:8px;padding:2px 12px 12px}
.small{min-height:24px;font-size:11.5px;padding:0 8px}
label{color:var(--muted);font-size:12px}
.stack{display:flex;flex-direction:column;gap:4px}
.check{display:flex;flex-direction:row;align-items:center;gap:8px;color:var(--text)}
.note{color:var(--muted);margin:0;font-size:11.5px;overflow-wrap:anywhere}.ell{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.error{color:var(--danger);margin:0;overflow-wrap:anywhere}
details{margin:0}summary{color:var(--muted);font-size:12px;cursor:pointer}
h3{font-size:12px;font-weight:600;margin:8px 0 4px}
strong{font-size:11px;color:var(--muted);font-weight:600}
pre{font-size:11.5px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:2px 0 6px;max-height:none;white-space:pre-wrap;overflow-wrap:anywhere}
.variants{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px}
.row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}.grow{flex:1}
.grok-scene-dialog{width:min(1000px,95vw);max-height:90vh;padding:0;overflow:auto;background:var(--panel);color:var(--text);border:1px solid var(--line-strong);border-radius:var(--radius-lg)}
.grok-scene-dialog::backdrop{background:#000a}
.dhead{display:flex;align-items:center;justify-content:space-between;gap:12px;min-height:44px;padding:0 12px;border-bottom:1px solid var(--line);position:sticky;top:0;background:var(--panel);z-index:1}
.dhead strong{font-size:13px;color:var(--text)}
.dbody{display:grid;gap:10px;padding:12px}
.entries{border-top:1px solid var(--line)}
.entry{border-bottom:1px solid var(--line);padding:6px 0;display:grid;gap:4px}
</style>
