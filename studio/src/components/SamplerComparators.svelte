<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message, imageSource } from '../lib/api';
  type Image = { assetId: string; nodeId: string; outputIndex: number; kind: string };
  type Step = { index:number; sampler:string; classType:string; title:string|null; parameters:Record<string,unknown>; models:string[]; loras:string[]; images:Image[]; mode:'single'|'compare'; previous:string|null; status:string };
  type Report = { job:string; state:string; steps:Step[]; errors:string[]; pending:boolean };
  let { job, onclose=()=>{}, context }: { job:string; onclose?:()=>void; context?:string } = $props();
  let report=$state<Report|null>(null), selected=$state(1), error=$state(''), left=$state(''), right=$state(''), previous=$state(''), loading=$state(false);
  let split=$state(50), zoom=$state(1), panX=$state(0), panY=$state(0), dragging=$state(false), drag=null as null|{x:number;y:number;px:number;py:number}, orientation=$state<'vertical'|'horizontal'>('vertical');
  async function load() {
    if (!job) return; loading=true; error='';
    try { report=await call<Report>('sampler_steps',{job}); if (selected>report.steps.length) selected=Math.max(1,report.steps.length); await images(); }
    catch(e){error=message(e);} finally{loading=false;}
  }
  async function source(id:string){ return id ? imageSource(id,true) : ''; }
  async function images(){
    const step=report?.steps.find(s=>s.index===selected); if(!step){left='';right='';previous='';return;}
    const prev=report?.steps.find(s=>s.index===selected-1);
    left=await source(prev?.images[0]?.assetId ?? '');
    previous=step.previous ?? '';
    right=await source(step.images[0]?.assetId ?? '');
  }
  async function choose(index:number){ selected=index; zoom=1;panX=0;panY=0;split=50; await images(); }
  function down(e:PointerEvent){dragging=true;drag={x:e.clientX,y:e.clientY,px:panX,py:panY};(e.currentTarget as HTMLElement | null)?.setPointerCapture?.(e.pointerId);}
  function move(e:PointerEvent){if(!dragging||!drag)return;panX=drag.px+(e.clientX-drag.x);panY=drag.py+(e.clientY-drag.y);}
  function up(){dragging=false;drag=null;}
  const step=$derived(report?.steps.find(s=>s.index===selected) ?? null);
  onMount(load);
</script>
<section class="sampler-comparators" data-sampler-comparators={job}><p class="subtle" data-sampler-context>{context ?? $t("Résultat")}</p>
  <div class="button-row"><h2>{$t("Étapes de sampling")}</h2><button onclick={load} disabled={loading}>{$t("Actualiser")}</button>{#if onclose}<button onclick={onclose}>{$t("Fermer")}</button>{/if}</div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if report}
    <p class="subtle" data-sampler-state>{report.state}{#if report.pending} · {$t("en cours ou incomplet")}{/if} · {$tn("{n} sampler","{n} samplers",report.steps.length)}</p>
    {#if report.errors.length}<ul data-sampler-errors>{#each report.errors as e}<li>{e}</li>{/each}</ul>{/if}
    <div class="chip-row" data-sampler-step-buttons>
      {#each report.steps as s (s.index)}<button class:primary={selected===s.index} data-sampler-step={s.index} onclick={()=>choose(s.index)}>V{s.index} · {s.classType}#{s.sampler}</button>{/each}
    </div>
    {#if step}
      <p data-sampler-current><strong>V{step.index}</strong> · {step.classType}#{step.sampler} · {step.status} · steps {step.parameters.steps ?? '?'} · CFG {step.parameters.cfg ?? '?'} · denoise {step.parameters.denoise ?? '?'} · {step.parameters.sampler ?? '?'} / {step.parameters.scheduler ?? '?'} · seed {step.parameters.seed ?? '?'}</p>
      {#if step.models.length}<p class="subtle">{$t("Modèle : {list}",{list:step.models.join(' · ')})}</p>{/if}
      {#if step.loras.length}<p class="subtle">{$t("LoRA : {list}",{list:step.loras.join(' · ')})}</p>{/if}
      {#if step.images.length === 0}<p class="error" data-sampler-missing>{$t("Aucune image intermédiaire collectée pour cette étape.")}</p>{/if}
      {#if step.index === 1}
        <div class="button-row"><button data-sampler-zoom-out onclick={()=>zoom=Math.max(1,zoom/1.2)}>−</button><button data-sampler-zoom-in onclick={()=>zoom=Math.min(8,zoom*1.2)}>+</button><button onclick={()=>{zoom=1;panX=0;panY=0}}>100 %</button></div>
        <div class="sampler-stage" role="application" aria-label={$t("Rendu du premier sampler")} data-sampler-single onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={up}>
          {#if right}<img src={right} alt={$t("Rendu V1 sampler {sampler}",{sampler:step.sampler})} style="transform:translate({panX}px,{panY}px) scale({zoom})" />{:else}<p>{$t("En attente du rendu V1…")}</p>{/if}
        </div>
      {:else}
        <div class="button-row"><button data-sampler-orientation onclick={()=>orientation=orientation==="vertical"?"horizontal":"vertical"}>{orientation==="vertical"?$t("Séparation verticale · basculer horizontale"):$t("Séparation horizontale · basculer verticale")}</button><label>{$t("Séparateur")} <input type="range" min="0" max="100" bind:value={split} /></label><button data-sampler-zoom-out onclick={()=>zoom=Math.max(1,zoom/1.2)}>−</button><button data-sampler-zoom-in onclick={()=>zoom=Math.min(8,zoom*1.2)}>+</button><button onclick={()=>{zoom=1;panX=0;panY=0;split=50}}>{$t("Réinitialiser")}</button></div>
        <div class="sampler-stage" class:sampler-horizontal={orientation==='horizontal'} role="application" aria-label={$t("Comparateur des samplers")} title={$t("Comparaison V{a} ↔ V{b} · {orientation} · alignement contain centré, aucune déformation ; images de dimensions différentes conservent leur ratio.",{a:step.index-1,b:step.index,orientation:orientation==='vertical'?$t("gauche ↔ droite"):$t("haut ↔ bas")})} data-sampler-compare onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={up}>
          {#if left && right}
            <img src={left} alt={$t("V{n} sampler {sampler}",{n:step.index-1,sampler:previous})} style="transform:translate({panX}px,{panY}px) scale({zoom})" />
            <div class="sampler-clip" class:sampler-clip-horizontal={orientation==='horizontal'}
              style={orientation==='vertical'?`width:${split}%`:`height:${split}%`}>
              <img src={right} alt={$t("V{n} sampler {sampler}",{n:step.index,sampler:step.sampler})}
                style="transform:translate({panX}px,{panY}px) scale({zoom});{orientation==='vertical'?`width:${10000/split}%`:`height:${10000/split}%`}" />
            </div>
            <div class="sampler-handle" class:sampler-handle-horizontal={orientation==='horizontal'}
              style={orientation==='vertical'?`left:${split}%`:`top:${split}%`}></div>
          {:else}<p>{left?$t("Image V{n} chargée,",{n:step.index-1})+' ':$t("Image précédente absente,")+' '}{right?$t("image courante chargée."):$t("image courante absente.")}</p>{/if}
        </div>
      {/if}
    {/if}
  {/if}

</section>
<style>
    .sampler-comparators{display:grid;gap:8px;min-width:0;margin-bottom:16px}
    .sampler-comparators h2{font-size:13px;font-weight:600;margin:0;flex:1}
    .sampler-comparators p{margin:0}
    .subtle{color:var(--muted);font-size:12px}
    .button-row{display:flex;flex-wrap:wrap;gap:6px;align-items:center}
    .chip-row{display:flex;flex-wrap:wrap;gap:4px}
    .chip-row button{font-size:12px}
    .sampler-stage{position:relative;overflow:hidden;aspect-ratio:1;width:min(100%,72vh);background:var(--canvas);border:1px solid var(--line);border-radius:var(--r-img);touch-action:none;display:grid;place-items:center}
    .sampler-stage img{position:absolute;max-width:none;max-height:none;width:100%;height:100%;object-fit:contain;transform-origin:center}
    .sampler-clip{position:absolute;inset:0 auto 0 0;overflow:hidden;border-right:1px solid var(--sel)}
    .sampler-clip img{width:auto;height:100%;max-width:none;object-fit:contain}
    .sampler-clip-horizontal{inset:0 0 auto 0;border-right:0;border-bottom:1px solid var(--sel)}
    .sampler-clip-horizontal img{width:100%;height:auto;object-fit:contain}
    .sampler-handle{position:absolute;top:0;bottom:0;width:1px;background:var(--sel)}
    .sampler-handle-horizontal{left:0;right:0;top:auto;bottom:auto;height:1px;width:auto}
</style>
