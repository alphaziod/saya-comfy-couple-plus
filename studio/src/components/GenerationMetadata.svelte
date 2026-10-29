<script lang="ts">
  import { t } from '../lib/i18n';
 import {call,message} from '../lib/api';
 let {id}:{id:string}=$props();
 interface Section{node:string|null;input:string|null;name:string;role:string;status:string;content:string|null;activity:string}
 interface Metadata{mode:string;sections:Section[];source:string;width:number|null;height:number|null;fileModifiedUtc:string|null}
 let data=$state<Metadata|null>(null),error=$state(''),copied=$state('');
 $effect(()=>{const asset=id;let alive=true;data=null;error='';copied='';call<Metadata>('generation_metadata',{id:asset}).then(v=>{if(alive)data=v}).catch(e=>{if(alive)error=message(e)});return()=>{alive=false};});
 async function copy(text:string,label:string){error='';try{if(navigator.clipboard){await navigator.clipboard.writeText(text);}else{const input=document.createElement('textarea');input.value=text;document.body.append(input);input.select();const ok=document.execCommand('copy');input.remove();if(!ok)throw Error($t("Presse-papiers indisponible"));}copied=label;}catch(e){error=message(e);}}
 // One card per prompt node of the pass: role as title, final text
 // read-only. Context shrinks to one discreet line; the rest of the
 // generation data stays in the file and the database, not displayed here.
 const roleLabels:Record<string,string>={BG:'BG',ACTION:'Action',P1:'P1','P1 ANATOMY':'P1 anatomie',P2:'P2','P2 ANATOMY':'P2 anatomie',NEG:'Négatif'};
 const shownRoles=['BG','ACTION','P1','P1 ANATOMY','P2','P2 ANATOMY','NEG'];
 // Negative handled elsewhere: empty, embedding placeholder only, or couple mask
 // (mask entries never arrive as prompt sections anyway).
 const maskedNegative=(s:Section)=>!s.content||/^\s*embedding:\S+\s*$/i.test(s.content);
 const blocks=$derived.by(()=>{
  if(!data)return[];
  const byRole=new Map<string,Section>();
  for(const s of data.sections){
   if(!shownRoles.includes(s.role))continue;
   const current=byRole.get(s.role);
   // Node and encoder give the same role: a single card, the final text wins.
   if(!current||(current.content===null||current.content==='')&&s.content)byRole.set(s.role,s);
  }
  const couple=data.mode==='couple';
  return [...byRole.values()].filter(s=>{
   if((s.role==='P2'||s.role==='P2 ANATOMY')&&(!couple||!s.content))return false;
   if(s.role==='NEG'&&maskedNegative(s))return false;
   return true;
  }).sort((a,b)=>shownRoles.indexOf(a.role)-shownRoles.indexOf(b.role));
 });
 const promptsText=$derived(blocks.filter(s=>s.content).map(s=>`${roleLabels[s.role]??s.role} :\n${s.content}`).join('\n\n'));
 const timeOfDay=$derived(data?.fileModifiedUtc?data.fileModifiedUtc.slice(11,16):null);
</script>
<section data-generation-metadata>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if data}<p class="subtle mode num" data-generation-mode>{data.mode==='solo'?'SOLO':data.mode==='couple'?'COUPLE':$t("Mode indéterminé")} · {data.width??'?'} × {data.height??'?'} · {data.source==='job'?$t("job Saya Studio"):$t("métadonnées image")}{timeOfDay?` · ${timeOfDay}`:''}</p>
{#if blocks.length}<div class="prompts-head"><span class="sec-title">{$t("Prompts")}</span><button class="quiet" onclick={()=>copy(promptsText,$t("Prompts"))} disabled={!promptsText}>{$t("Copier les prompts")}</button></div>
<div class="prompts-list" data-prompts-box aria-label={$t("Prompts de génération, lecture seule")}>
{#each blocks as section,i}{#if section.content}<div class="prompt-entry" data-prompt-section={i} data-prompt-role={section.role}><h4>{$t(roleLabels[section.role]??section.role)} :</h4><pre>{section.content}</pre></div>{/if}{/each}
</div>{/if}
{#if copied}<p class="subtle" role="status">{$t("Copié : {label}",{label:copied})}</p>{/if}
{:else if !error}<p class="subtle">{$t("Lecture des métadonnées…")}</p>{/if}
</section>
<style>
 section{display:grid;gap:8px;min-width:0}
 p{margin:0}
 .mode{font-size:11.5px}
 .prompts-head{display:flex;justify-content:space-between;align-items:center;gap:8px;margin:4px 0 0}
 .prompts-head button{min-height:22px;padding:0 6px;font-size:11.5px}
 .prompts-list{display:flex;flex-direction:column;gap:10px;margin:0}
 .prompt-entry{margin:0;display:grid;grid-template-columns:72px minmax(0,1fr);gap:2px 10px;align-items:start}
 .prompt-entry h4{margin:0;font-size:12px;font-weight:400;color:var(--muted);line-height:1.5}
 pre{white-space:pre-wrap;overflow-wrap:anywhere;font:inherit;font-size:12px;line-height:1.5;color:var(--muted);background:none;border:0;padding:0;margin:0;max-height:240px;overflow:auto}
</style>
