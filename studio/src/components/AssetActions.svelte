<script lang="ts">
  import { t, tn } from '../lib/i18n';
 import {call,message,type Asset} from '../lib/api';
 import Icon from './Icon.svelte';
 // variant 'bar': multi-selection bar; 'trash': trash button
 // for a single image (inspector), same plan + confirmation.
 let {assets,onchanged,onclear,variant='bar'}: {assets:Asset[];onchanged:()=>Promise<void>;onclear:()=>void;variant?:'bar'|'trash'}=$props();
 interface TrashFile{id:string;path:string;bytes:string;modified:string;identity:string;sha256:string}
 let busy=$state(false),error=$state(''),note=$state(''),plan=$state<TrashFile[]|null>(null);
 function modal(node:HTMLDialogElement){node.showModal();}
 async function catalog(removed:boolean){if(busy)return;busy=true;error='';try{await call('set_catalog_removed',{ids:assets.map(a=>a.id),removed});await onchanged();onclear();note=removed?$t("Sélection retirée du catalogue. Fichiers conservés."):$t("Sélection restaurée dans le catalogue.");}catch(e){error=message(e);}finally{busy=false;}}
 async function prepare(){if(busy)return;busy=true;error='';try{plan=await call<TrashFile[]>('prepare_asset_trash',{ids:assets.map(a=>a.id)});}catch(e){error=message(e);}finally{busy=false;}}
 async function trash(){if(!plan)return;if(busy)return;busy=true;error='';try{const result=await call<{trashed:string[];errors:{path:string;error:string}[];requested:number}>('trash_assets',{plan:$state.snapshot(plan),confirmed:true});note=$t("{done} / {total} fichiers mis à la corbeille.",{done:result.trashed.length,total:result.requested});error=result.errors.map(e=>`${e.path} : ${e.error}`).join('\n');plan=null;await onchanged();onclear();}catch(e){error=message(e);}finally{busy=false;}}
</script>
{#if variant==='trash'}<button class="icon-btn quiet trash-btn" disabled={busy||!assets.length} onclick={prepare} title={$t("Mettre cette image à la corbeille")} aria-label={$t("Mettre cette image à la corbeille")}><Icon name="trash"/></button>
{:else if assets.length}<section class="selection-bar" data-asset-actions><strong class="num">{$tn("{n} image sélectionnée","{n} images sélectionnées",assets.length)}</strong><button class="quiet" disabled={busy} onclick={()=>catalog(true)} title={$t("Retirer du catalogue conserve les fichiers, annotations et liens historiques. La corbeille agit sur les fichiers après confirmation.")}>{$t("Retirer du catalogue")}</button><button class="quiet" disabled={busy} onclick={()=>catalog(false)}>{$t("Restaurer au catalogue")}</button><button class="danger" data-prepare-trash disabled={busy} onclick={prepare}>{$t("Mettre à la corbeille…")}</button><span class="grow"></span><button class="quiet" disabled={busy} onclick={onclear}>{$t("Vider la sélection")}</button></section>{/if}
{#if note}<p class="msg" class:inline={variant==='trash'} role="status">{note}</p>{/if}{#if error&&!plan}<p class="msg error" class:inline={variant==='trash'} role="alert">{error}</p>{/if}
{#if plan}<dialog class="trash-dialog" use:modal onclose={()=>{if(!busy)plan=null;}} oncancel={e=>{if(busy)e.preventDefault();}} aria-label={$t("Confirmer la mise à la corbeille")}><h2>{$tn("Mettre {n} fichier à la corbeille","Mettre {n} fichiers à la corbeille",plan.length)}</h2><p class="muted">{$t("Les fichiers seront déplacés dans la corbeille du système. L’historique restera dans Saya Studio. Aucun effacement définitif de remplacement.")}</p><ul>{#each plan as file}<li>{file.path} <span class="muted num">· {$t("{n} octets",{n:file.bytes})}</span></li>{/each}</ul><div class="actions"><button disabled={busy} onclick={()=>plan=null}>{$t("Annuler")}</button><button class="danger" data-confirm-trash disabled={busy} onclick={trash}>{$t("Confirmer la mise à la corbeille")}</button></div>{#if error}<p class="error" role="alert">{error}</p>{/if}</dialog>{/if}
<style>
 .selection-bar{display:flex;align-items:center;gap:6px;min-height:36px;padding:0 10px;border-bottom:1px solid var(--line);background:var(--raised);flex-wrap:wrap}
 .selection-bar strong{font-weight:600;margin-inline-end:6px}
 .selection-bar button{min-height:26px;font-size:12px}
 .grow{flex:1}
 .msg{margin:0;padding:6px 10px;border-bottom:1px solid var(--line);background:var(--bg);white-space:pre-wrap;font-size:12px}
 .msg.inline{flex-basis:100%;border:0;background:none;padding:4px 0 0}
 .trash-btn:hover:not(:disabled){color:var(--danger)}
 .trash-dialog{max-width:760px;width:calc(100vw - 48px);max-height:80vh;overflow:auto;background:var(--panel);color:var(--text);border:1px solid var(--line-strong);border-radius:6px;padding:16px}
 .trash-dialog::backdrop{background:#000a}
 .trash-dialog h2{font-size:14px;font-weight:600;margin:0 0 8px}
 .trash-dialog p{margin:0 0 10px;white-space:pre-wrap}
 .trash-dialog ul{margin:0 0 12px;padding-inline-start:18px;display:grid;gap:4px;font-size:12px}
 li{overflow-wrap:anywhere}
 .actions{display:flex;justify-content:flex-end;gap:8px}
</style>
