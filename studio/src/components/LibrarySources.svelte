<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { call, message, native } from '../lib/api';
  import Icon from './Icon.svelte';
  let { root=$bindable(''), busy=false, refreshKey=0, onindex, onchanged } = $props<{root?:string;busy?:boolean;refreshKey?:number;onindex:(path:string)=>Promise<boolean>;onchanged:()=>Promise<void>}>();
  interface Source {id:string;path:string;enabled:boolean;state:string;error:string|null;run:number|null}
  let sources=$state<Source[]>([]),working=$state(false),error=$state(''),note=$state('');
  const labels:Record<string,string>={idle:'À indexer',ready:'Indexée',partial:'Terminée avec erreurs',interrupted:'Interrompue',inaccessible:'Source inaccessible'};
  async function refresh(){sources=await call<Source[]>('library_sources');}
  async function add(paths:string[]){
    const errors:string[]=[];
    for(const path of paths) {try{await call('add_library_source',{path});root=path;}catch(e){errors.push(`${path} : ${message(e)}`);}}
    await refresh();await onchanged();error=errors.join('\n');
  }
  async function browse(){working=true;error='';note='';try{const paths=await invoke<string[]>('pick_directories');if(paths.length)await add(paths);else note=$t("Aucun dossier ajouté.");}catch(e){error=message(e);}finally{working=false;}}
  async function manual(){working=true;error='';try{await add([root]);}catch(e){error=message(e);}finally{working=false;}}
  async function enable(source:Source,enabled:boolean){working=true;error='';try{await call('set_library_source',{id:source.id,enabled});await refresh();await onchanged();}catch(e){error=message(e);}finally{working=false;}}
  async function scan(paths:string[]){working=true;error='';try{for(const path of paths){root=path;if(await onindex(path))break;}await refresh();}catch(e){error=message(e);}finally{working=false;}}
  async function open(path:string){try{await call('open_directory',{path});}catch(e){error=message(e);}}
  $effect(()=>{refreshKey;if(native)refresh().catch(e=>error=message(e));});
</script>
<details data-library-sources class="sec source-panel"><summary><span class="sec-title">{$t("Sources et indexation")}</span><span class="chip">{sources.filter(s=>s.enabled).length}</span></summary>
 <div class="sec-body">
  <div class="row"><button data-browse-directories disabled={!native||busy||working} onclick={browse}>{$t("Parcourir…")}</button><button class="quiet" data-index-all disabled={!native||busy||working||!sources.some(s=>s.enabled)} onclick={()=>scan(sources.filter(s=>s.enabled).map(s=>s.path))}>{$t("Réindexer toutes les sources")}</button></div>
  {#each sources as source(source.id)}<div data-source-id={source.id} class="source-row" class:off={!source.enabled}>
   <div class="source-line"><span class="path" title={source.path}>{source.path.split(/[\\/]/).filter(Boolean).pop() ?? source.path}</span><button class="icon-btn quiet" disabled={busy||working||!source.enabled} onclick={()=>scan([source.path])} title={$t("Réindexer")} aria-label={$t("Réindexer")}><Icon name="redo" size={14}/></button><button class="icon-btn quiet" disabled={working} onclick={()=>open(source.path)} title={$t("Ouvrir le dossier")} aria-label={$t("Ouvrir le dossier")}><Icon name="folder" size={14}/></button></div>
   <div class="source-line"><small class="muted">{source.enabled?$t(labels[source.state]??source.state):$t("Hors racines actives")}</small><button class="quiet small" disabled={busy||working} onclick={()=>enable(source,!source.enabled)}>{source.enabled?$t("Retirer cette source"):$t("Réactiver cette source")}</button></div>
   {#if source.error}<p class="error">{source.error}</p>{/if}
   {#if source.run}<details class="inner"><summary class="muted">{$t("Erreurs du dernier scan")}</summary><button class="quiet small" onclick={async()=>{try{const rows=await call<{path:string;message:string}[]>('index_errors',{run:source.run,offset:0});note=rows.length?rows.map(r=>`${r.path} : ${r.message}`).join('\n'):$t("Aucune erreur enregistrée.");}catch(e){error=message(e);}}}>{$t("Afficher les 100 premières erreurs")}</button></details>{/if}
  </div>{/each}
  <details class="inner"><summary class="muted">{$t("Saisir un chemin manuellement")}</summary><label class="root-field"><span class="muted">{$t("Dossier à indexer")}</span><input bind:value={root} placeholder={$t("Chemin absolu du dossier")} disabled={busy||working}/></label><button disabled={!native||busy||working||!root.trim()} onclick={manual}>{$t("Ajouter ce dossier")}</button></details>
  {#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="muted" role="status">{note}</p>{/if}
 </div>
</details>
<style>
 .sec{margin:0}
 .row{display:flex;gap:6px;flex-wrap:wrap}
 .row button{min-height:26px;font-size:12px}
 .source-row{display:grid;gap:2px;padding:6px 0;border-top:1px solid var(--line)}
 .source-row.off .path{color:var(--muted)}
 .source-line{display:flex;align-items:center;gap:4px;min-width:0}
 .path{flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
 .source-line small{flex:1;font-size:11px}
 .icon-btn{min-height:24px;width:24px}
 button.small{min-height:22px;padding:0 6px;font-size:11.5px}
 details.inner{margin:0}
 details.inner>summary{font-size:11.5px;padding:2px 0;cursor:pointer}
 .root-field{display:grid;gap:4px;margin:6px 0;font-size:12px}
 .root-field input{width:100%}
 p{margin:0;white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px}
</style>
