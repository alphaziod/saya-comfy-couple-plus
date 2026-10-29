<script lang="ts">
  import { t } from '../lib/i18n';
  import Thumbnail from './Thumbnail.svelte';
  let {before,after,beforeLabel,afterLabel}:{before:string;after:string;beforeLabel:string;afterLabel:string}=$props();
  let split=$state(50);
  function position(e:PointerEvent){
    const node=e.currentTarget as HTMLElement;
    const rect=node.getBoundingClientRect();
    if(rect.width)split=Math.max(0,Math.min(100,(e.clientX-rect.left)*100/rect.width));
  }
  function down(e:PointerEvent){(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);position(e);}
  function move(e:PointerEvent){if((e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId))position(e);}
  function up(e:PointerEvent){(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);}
</script>
<div class="labels"><span>{beforeLabel}</span><span>{afterLabel}</span></div>
<div class="stage" role="group" aria-label={$t("Comparaison avant après")} data-pass-comparison onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}>
  <div class="layer"><Thumbnail id={after} preview alt={afterLabel}/></div>
  <div class="layer" style:clip-path={`inset(0 ${100-split}% 0 0)`}><Thumbnail id={before} preview alt={beforeLabel}/></div>
  <div class="divider" style:left={`${split}%`}><span>↔</span></div>
</div>
<label class="slider"><span class="sr">{$t("Séparateur avant / après")}</span><input aria-label={$t("Séparateur avant après")} data-pass-comparison-slider type="range" min="0" max="100" bind:value={split}/></label>
<style>
.labels{display:flex;justify-content:space-between;gap:12px;color:var(--muted);font-size:11px;margin-bottom:4px}.stage{position:relative;aspect-ratio:1;height:auto;min-height:0;background:var(--canvas);border-radius:var(--r-img);overflow:hidden;touch-action:none;cursor:ew-resize}.layer{position:absolute;inset:0;min-width:0;min-height:0}.layer :global(img){position:absolute;inset:0;display:block;width:100%;height:100%;max-width:100%;max-height:100%;object-fit:contain;pointer-events:none;user-select:none}.divider{position:absolute;top:0;bottom:0;width:1px;background:var(--sel);pointer-events:none}.divider span{position:absolute;top:50%;left:50%;transform:translate(-50%,-50%);background:var(--panel);border:1px solid var(--sel);border-radius:50%;padding:3px 5px;font-size:11px}.slider{display:block;margin:6px 0 0}.slider input{width:100%;min-height:0;padding:0;accent-color:var(--accent)}.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)}
</style>
