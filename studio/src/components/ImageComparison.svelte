<script lang="ts">
  import { t } from '../lib/i18n';
  import {onMount} from 'svelte';
  import {call,message,type Asset} from '../lib/api';
  import Thumbnail from './Thumbnail.svelte';
  import Icon from './Icon.svelte';
  let {assets,onclose}:{assets:Asset[];onclose:()=>void}=$props();
  let details=$state<Record<string,unknown>>({}),errors=$state<Record<string,string>>({});
  function modal(node:HTMLDialogElement){node.showModal();}
  onMount(()=>{let alive=true;for(const asset of assets)call('detail',{id:asset.id}).then(value=>{if(alive)details[asset.id]=value;}).catch(e=>{if(alive)errors[asset.id]=message(e);});return()=>{alive=false;};});
</script>
<dialog class="image-dialog comp" data-image-comparison use:modal onclose={onclose} aria-label={$t("Comparaison de deux images")}>
  <header class="comp-bar"><strong>{$t("Comparaison · deux images")}</strong><button class="quiet icon-btn" title={$t("Fermer la comparaison")} aria-label={$t("Fermer la comparaison")} onclick={onclose}><Icon name="close"/></button></header>
  <div class="comp-cols">{#each assets as asset (asset.id)}<section class="comp-item">
    <div class="comp-image"><Thumbnail id={asset.id} preview alt={asset.path} /></div>
    <div class="comp-meta">
      <div class="comp-caption"><h2 title={asset.path}>{asset.path.split(/[\\/]/).pop()}</h2><span class="chip num">{asset.width??'?'} × {asset.height??'?'}</span></div>
      <details><summary>{$t("Métadonnées et provenance")}</summary><p class="hash">{asset.path}</p>{#if errors[asset.id]}<p class="error" role="alert">{errors[asset.id]}</p>{:else if details[asset.id]}<pre>{JSON.stringify(details[asset.id],null,2)}</pre>{:else}<p class="muted" role="status">{$t("Chargement des métadonnées…")}</p>{/if}</details>
    </div>
  </section>{/each}</div>
</dialog>
<style>
  .image-dialog.comp{width:calc(100vw - 24px);height:calc(100vh - 24px);background:var(--canvas);border:1px solid var(--line-strong);border-radius:6px;color:var(--text)}
  .image-dialog.comp::backdrop{background:#000d}
  .comp-bar{display:flex;align-items:center;justify-content:space-between;gap:12px;height:40px;padding:0 6px 0 14px;border-bottom:1px solid var(--line);background:var(--bg);flex:none}
  .comp-bar strong{font-size:13px;font-weight:600}
  .comp-cols{flex:1;min-height:0;display:grid;grid-template-columns:repeat(2,minmax(0,1fr))}
  .comp-item{min-width:0;min-height:0;display:flex;flex-direction:column}
  .comp-item+.comp-item{border-inline-start:1px solid var(--line)}
  .comp-image{flex:1;min-height:0;display:flex;align-items:center;justify-content:center;padding:8px;background:var(--canvas)}
  .comp-image :global(img){max-width:100%;max-height:100%;width:auto;height:auto;object-fit:contain;border-radius:var(--r-img)}
  .comp-meta{flex:none;border-top:1px solid var(--line);background:var(--bg);padding:6px 12px}
  .comp-caption{display:flex;align-items:center;gap:8px;min-width:0}
  .comp-caption h2{margin:0;font-size:12.5px;font-weight:600;flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  details{margin:4px 0 0}summary{color:var(--muted);font-size:12px}
  details p{margin:4px 0}
  pre{max-height:30vh;margin:4px 0 0;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:8px}
  @media(max-width:650px){.comp-cols{grid-template-columns:1fr;overflow:auto}.comp-image{height:45vh;flex:none}.comp-item+.comp-item{border-inline-start:0;border-top:1px solid var(--line)}}
</style>
