<script lang="ts">
  import { t } from '../lib/i18n';
  import { loraMatches, loraResolved } from '../lib/generation-workspace';
  type Labels=Record<string,{label:string;base:string}>;
  // Typing only filters the engine's real list; a name is committed only by picking a result.
  let { value='', names=null, labels={}, disabled=false, label='LoRA', placeholder, onpick }:
    { value?:string; names?:string[]|null; labels?:Labels; disabled?:boolean; label?:string; placeholder?:string; onpick:(name:string)=>void } = $props();
  let query=$state(''), open=$state(false), active=$state(0), list=$state<HTMLUListElement>();
  const results=$derived(names&&open?loraMatches(names,query,labels):[]);
  const resolved=$derived(names&&value?loraResolved(names,value):null);
  const shown=$derived(resolved&&labels[resolved]?`${labels[resolved].label} · ${resolved}`:value);
  function pick(name:string){open=false;query='';onpick(name)}
  function move(to:number){active=Math.max(0,Math.min(to,results.length-1));list?.querySelectorAll('li button')[active]?.scrollIntoView({block:'nearest'})}
  function key(e:KeyboardEvent){
    if(e.key==='ArrowDown'){open=true;move(active+1);e.preventDefault()}
    else if(e.key==='ArrowUp'){move(active-1);e.preventDefault()}
    else if(e.key==='Enter'&&open&&results[active]){pick(results[active]);e.preventDefault()}
    else if(e.key==='Escape'){open=false;query=''}
  }
</script>
{#if names}
  <div class="lora-picker" data-lora-picker>
    <input aria-label={label} data-value={value} value={open?query:shown} placeholder={placeholder ?? $t("Rechercher une LoRA…")} {disabled} title={resolved??value}
      class:missing={!!value&&!resolved} onfocus={()=>{open=true;query='';active=0}} oninput={e=>{query=e.currentTarget.value;open=true;active=0}}
      onkeydown={key} onblur={()=>setTimeout(()=>{open=false;query=''},150)}/>
    {#if open&&!disabled}<ul role="listbox" bind:this={list}><li class="count">{$t("{shown} / {total} LoRA",{shown:results.length,total:names.length})}</li>{#each results as name,i (name)}<li role="option" aria-selected={i===active}><button type="button" data-lora-name={name} class:active={i===active} onmousedown={e=>{e.preventDefault();pick(name)}}>{#if labels[name]}<strong>{labels[name].label}</strong>{/if}<small>{name}{labels[name]?.base?` · ${labels[name].base}`:''}</small></button></li>{:else}<li class="none">{$t("Aucune LoRA ne correspond")}</li>{/each}</ul>{/if}
  </div>
{:else}
  <input aria-label={label} {value} {disabled} placeholder={$t("Nom de la LoRA (moteur ComfyUI éteint : saisie libre)")} title={$t("Liste du moteur indisponible")} onchange={e=>{const typed=e.currentTarget.value.trim();e.currentTarget.value=value;if(typed)onpick(typed)}}/>
{/if}
<style>
  .lora-picker{position:relative;min-width:0}.lora-picker input{width:100%;min-width:0;margin:0;padding:0 8px;min-height:28px;font-size:12px}.missing{border-color:var(--warning)}
  ul{position:absolute;z-index:20;inset-inline:0;top:100%;margin:2px 0 0;padding:4px;list-style:none;max-height:360px;overflow:auto;background:var(--raised);border:1px solid var(--line-strong);border-radius:var(--radius);box-shadow:0 8px 24px #0008}
  li button{display:flex;flex-direction:column;align-items:stretch;width:100%;min-height:0;text-align:start;padding:4px 8px;margin:0;border:0;border-radius:3px;background:none;min-width:0}
  li strong,li small{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}li strong{font-size:12px;font-weight:500}li small{font-size:11px;color:var(--muted)}
  li button.active,li button:hover{background:var(--hover)}.none,.count{font-size:11px;color:var(--muted);padding:4px 8px}.count{position:sticky;top:-4px;background:var(--raised)}
</style>
