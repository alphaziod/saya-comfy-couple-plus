<script lang="ts">
  import { t } from '../lib/i18n';
 import {onMount} from 'svelte';
 import {call,message,type Character} from '../lib/api';
 import Thumbnail from './Thumbnail.svelte';
 import Icon from './Icon.svelte';
 interface Reference {assetId:string;variantId:string|null;variantName:string|null;position:number;primary:boolean;availability:string;historicalVariant:boolean}
 let {character,variants,onimage,onbrowse}:{character:Character;variants:{id:string;name:string}[];onimage:(asset:string)=>void;onbrowse:()=>void}=$props();
 interface ReferenceEvent {id:number;operation:string;savedAt:string;changes:{before:Reference[];after:Reference[]}}
 const operations:Record<string,string>={migration:'État au début du suivi',add:'Référence ajoutée',remove:'Référence retirée',primary:'Image principale modifiée',order:'Ordre modifié',associate:'Image associée',unlink:'Association retirée',import:'Références importées'};
 let history=$state<ReferenceEvent[]>([]),historyOpen=$state(false),historyBusy=$state(false),historyMore=$state(false),inspected=$state<number|null>(null);
 async function loadHistory(reset=false){historyBusy=true;try{const page=await call<ReferenceEvent[]>('reference_history',{character:character.id,before:reset?null:history.at(-1)?.id??null});history=reset?page:[...history,...page];historyMore=page.length===20;}catch(e){error=message(e);}finally{historyBusy=false;}}
 let references=$state<Reference[]>([]),scope=$state(''),candidate=$state(''),busy=$state(false),error=$state(''),candidateLimit=$state(48);
 const candidates=$derived(character.imageIds.filter(id=>!shown.some(r=>r.assetId===id)));
 const shown=$derived(references.filter(r=>(r.variantId??'')===scope));
 const historical=$derived(references.filter(r=>r.historicalVariant).filter((r,i,list)=>list.findIndex(x=>x.variantId===r.variantId)===i));
 const canAdd=$derived(!scope||variants.some(v=>v.id===scope));
 async function load(){references=await call('references',{character:character.id});if(historyOpen)await loadHistory(true);}
 async function edit(operation:string,asset:string){if(busy)return;busy=true;error='';try{await call('reference_edit',{character:character.id,variant:scope||null,asset,operation});await load();candidate='';}catch(e){error=message(e);}finally{busy=false;}}
 async function move(index:number,delta:number){const assets=shown.map(r=>r.assetId);[assets[index],assets[index+delta]]=[assets[index+delta],assets[index]];if(busy)return;busy=true;error='';try{await call('reference_order',{character:character.id,variant:scope||null,assets});await load();}catch(e){error=message(e);}finally{busy=false;}}
 onMount(()=>{load().catch(e=>error=message(e));});
</script>
<section class="refs" aria-label={$t("Références ordonnées")}>
<div class="refs-bar"><select aria-label={$t("Portée des références")} title={$t("L’image principale est choisie explicitement. Réorganiser ou retirer une référence modifie seulement ses liens ; les fichiers et la provenance des générations restent conservés.")} disabled={busy} bind:value={scope} onchange={()=>{candidate='';candidateLimit=48;}}><option value="">{$t("Fiche principale")}</option>{#each variants as v}<option value={v.id}>{v.name}</option>{/each}{#each historical as r}<option value={r.variantId}>{r.variantName} · {$t("historique")}</option>{/each}</select><span class="grow"></span><button class="quiet" onclick={onbrowse}>{$t("Voir les images de ce personnage")}</button></div>
{#if error}<p class="error" role="alert">{error}</p>{/if}
<div class="ref-grid">{#each shown as ref,index (ref.assetId)}<article class="reference-item" class:primary={ref.primary} data-reference-id={ref.assetId}><button class="cover" onclick={()=>onimage(ref.assetId)} aria-label={$t("Ouvrir cette référence")}><Thumbnail id={ref.assetId} /></button><p class="cap">{ref.primary?$t("Image principale"):$t("Référence secondaire")}{ref.availability!=='present'?' · '+$t("fichier indisponible"):''}</p><div class="ref-acts"><button disabled={busy||character.archived||ref.primary} onclick={()=>edit('primary',ref.assetId)}>{$t("Définir principale")}</button><button class="icon-btn" aria-label={$t("Monter la référence")} title={$t("Monter la référence")} disabled={busy||character.archived||index===0} onclick={()=>move(index,-1)}>↑</button><button class="icon-btn" aria-label={$t("Descendre la référence")} title={$t("Descendre la référence")} disabled={busy||character.archived||index===shown.length-1} onclick={()=>move(index,1)}>↓</button><button class="icon-btn quiet" aria-label={$t("Retirer cette référence")} title={$t("Retirer cette référence")} disabled={busy||character.archived} onclick={()=>edit('remove',ref.assetId)}><Icon name="trash" size={14} /></button></div></article>{/each}</div>
{#if !shown.length}<p class="muted none">{$t("Aucune référence dans cette portée.")}</p>{/if}
<details class="sub"><summary>{$t("Choisir parmi les images associées · {n}",{n:candidates.length})}</summary><div class="ref-grid cands">{#each candidates.slice(0,candidateLimit) as id}<button class="cand" class:chosen={candidate===id} disabled={busy||character.archived||!canAdd} onclick={()=>candidate=id} aria-pressed={candidate===id} data-reference-candidate={id} title={candidate===id?$t("Image sélectionnée"):$t("Choisir cette image")} aria-label={candidate===id?$t("Image sélectionnée"):$t("Choisir cette image")}><Thumbnail {id} /></button>{/each}</div>{#if candidates.length>candidateLimit}<button class="quiet" onclick={()=>candidateLimit+=48}>{$t("Afficher davantage d’images")}</button>{/if}</details>
<div class="row"><button disabled={!candidate||busy||character.archived||!canAdd} onclick={()=>edit('add',candidate)}>{$t("Ajouter à ces références")}</button></div>
<details class="sub" ontoggle={e=>{historyOpen=e.currentTarget.open;if(historyOpen)loadHistory(true);}}><summary title={$t("Changements enregistrés depuis l’activation du suivi. Restaurer une révision de fiche conserve ces associations.")}>{$t("Historique des références")}</summary>
{#each history as entry}<article class="event"><div class="row"><span>{operations[entry.operation]?$t(operations[entry.operation]):entry.operation}</span><span class="muted num">{entry.savedAt}</span><span class="grow"></span><button class="quiet" aria-expanded={inspected===entry.id} onclick={()=>inspected=inspected===entry.id?null:entry.id}>{$t("Voir les changements")}</button></div>
{#if inspected===entry.id}{#each ['before','after'] as side}<h4>{side==='before'?$t("Avant"):$t("Après")}</h4><div class="ref-grid small">{#each entry.changes[side as 'before'|'after'] as ref}<div><button class="cover" onclick={()=>onimage(ref.assetId)} aria-label={$t("Ouvrir cette image historique")}><Thumbnail id={ref.assetId} /></button><p class="cap">{ref.variantName??$t("Fiche principale")} · {$t("position {n}",{n:ref.position+1})} · {ref.primary?$t("principale"):$t("secondaire")}</p></div>{/each}</div>{/each}{/if}</article>{/each}
{#if !history.length&&!historyBusy}<p class="muted">{$t("Aucun changement enregistré.")}</p>{/if}
{#if historyMore}<button class="quiet" disabled={historyBusy} onclick={()=>loadHistory()}>{$t("Changements plus anciens")}</button>{/if}
{#if historyBusy}<p class="muted" role="status">{$t("Chargement de l’historique…")}</p>{/if}</details>
</section>
<style>
 .refs{display:grid;gap:8px;min-width:0}
 .refs-bar{display:flex;align-items:center;gap:8px}.refs-bar select{max-width:260px}
 .grow{flex:1}
 .row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}
 .ref-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(130px,1fr));gap:12px 10px}
 .ref-grid.cands{grid-template-columns:repeat(auto-fill,minmax(84px,1fr));gap:6px;margin-top:6px}
 .ref-grid.small{grid-template-columns:repeat(auto-fill,minmax(84px,1fr));gap:6px}
 .reference-item{min-width:0;display:grid;gap:3px}
 .cover,.cand{display:block;width:100%;aspect-ratio:3/4;padding:0;border:0;border-radius:var(--r-img);overflow:hidden;background:var(--raised)}
 .cover :global(img),.cover :global(.image-placeholder),.cand :global(img),.cand :global(.image-placeholder){width:100%;height:100%;min-height:0;max-height:none;object-fit:cover;object-position:top}
 .reference-item.primary .cover{outline:2px solid var(--accent);outline-offset:2px}
 .cand.chosen{outline:2px solid var(--sel);outline-offset:2px}
 .cap{margin:2px 0 0;font-size:11px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
 .ref-acts{display:flex;gap:3px;align-items:center}.ref-acts button{min-height:24px;font-size:11px}.ref-acts button:first-child{flex:1;min-width:0;padding:0 6px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.ref-acts .icon-btn{width:24px}
 .none{margin:0}
 details.sub{margin:0}details.sub>summary{color:var(--muted);font-size:12px;min-height:24px;display:flex;align-items:center}
 .event{padding:6px 0;border-bottom:1px solid var(--line);font-size:12px}.event button{min-height:22px;font-size:11.5px}
 h4{margin:6px 0 4px;font-size:11px;font-weight:600;color:var(--muted)}
</style>
