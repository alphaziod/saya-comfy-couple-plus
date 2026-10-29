<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import GrokImport from "./GrokImport.svelte";
  import { onMount } from 'svelte';
  import { call, message, native, type CharacterCard } from '../lib/api';
  import { peopleView } from '../lib/stream';
  import Thumbnail from './Thumbnail.svelte';
  import CharacterSheet from './CharacterSheet.svelte';
  let { onimage = () => {} }: { onimage?: (character: string, asset: string) => void } = $props();
  let cards = $state<CharacterCard[] | null>(null);
  let error = $state(''), loading = $state(false);
  let selected = $state<string | null>(peopleView.selectedId);
  async function load() {
    loading = true; error = '';
    try { cards = await call<CharacterCard[]>('character_cards'); }
    catch (e) { error = message(e); }
    finally { loading = false; }
  }
  function choose(id: string) { selected = id; peopleView.selectedId = id; }
  function back() { selected = null; peopleView.selectedId = null; load(); }
  onMount(() => { if (native && !cards) load(); });
</script>
{#if selected}
  <CharacterSheet characterId={selected} onback={back} {onimage} />
{:else}
<div class="people">
  <div class="people-head"><span class="muted num">{cards ? $tn("{n} fiche active","{n} fiches actives",cards.length) : $t("Chargement…")}</span><span class="grow"></span><GrokImport onimported={load}/><button class="quiet" onclick={load} disabled={loading}>{$t("Actualiser")}</button></div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if loading && !cards}<p class="muted" role="status">{$t("Chargement des fiches…")}</p>{/if}
  {#if cards && !cards.length}<p class="muted empty">{$t("Aucune fiche active. Créez ou importez un personnage depuis l’onglet Personnages.")}</p>{/if}
  {#if cards}
    <div class="people-grid">
      {#each cards as card (card.id)}
        <button class="person-card" data-character={card.id} onclick={() => choose(card.id)} title={card.portraitStatus&&card.portraitStatus.state!=='present'?`${card.portraitStatus.path} : ${card.portraitStatus.reason}`:undefined}>
          {#if card.portrait&&card.portraitStatus?.state==='present'}<div class="person-portrait"><Thumbnail id={card.portrait} alt={$t("Portrait de {name}",{name:card.name})} /></div>
          {:else}<div class="person-portrait person-portrait-empty" aria-hidden="true"><span>{card.name.slice(0, 1).toUpperCase()}</span></div>{/if}
          <strong>{#if card.portraitStatus&&card.portraitStatus.state!=='present'}<i class="warn-dot" aria-hidden="true"></i>{/if}{card.name}</strong>
          {#if card.portraitStatus&&card.portraitStatus.state!=='present'}<small class="warn">{card.portraitStatus.state==='missing'?$t("Portrait manquant"):$t("Portrait inaccessible")} · {$t("ouvrir la fiche pour le détail")}</small>{/if}
          <small class="num">{card.mainVariant ? `${card.mainVariant} · ` : ''}{$tn("{n} image associée","{n} images associées",card.imageCount)} · {$tn("{n} visible","{n} visibles",card.visibleImageCount)}{card.variantCount > 1 ? ` · ${$tn("{n} variante","{n} variantes",card.variantCount)}` : ''}</small>
        </button>
      {/each}
    </div>
  {/if}
</div>
{/if}
<style>
  .people{display:grid;gap:12px;align-content:start}
  .people-head{display:flex;align-items:center;gap:8px;min-height:32px;padding-bottom:10px;border-bottom:1px solid var(--line)}
  .grow{flex:1}
  .empty{padding:40px 0;text-align:center}
  .people .people-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:14px 12px;margin:0;align-content:start}
  .people .person-card{display:flex;flex-direction:column;min-width:0;min-height:0;padding:0;text-align:start;background:transparent;border:0;border-radius:0;overflow:visible}
  .people .person-card:hover:not(:disabled){background:transparent}
  .people .person-card:hover .person-portrait,.people .person-card:focus-visible .person-portrait{outline:2px solid var(--sel);outline-offset:2px}
  .people .person-portrait{width:100%;aspect-ratio:3/4;border-radius:var(--r-img);overflow:hidden;background:var(--raised)}
  .people .person-portrait :global(img),.people .person-portrait :global(.image-placeholder){width:100%;height:100%;min-height:0;max-height:none;object-fit:cover;object-position:top;background:var(--raised)}
  .people .person-portrait-empty{display:flex;align-items:center;justify-content:center}
  .people .person-portrait-empty span{font-size:40px;color:var(--faint)}
  .people .person-card strong,.people .person-card small{display:block;padding:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .people .person-card strong{margin-top:6px;font-size:12.5px;font-weight:500}
  .people .person-card small{font-size:11px;color:var(--muted)}
  .people .person-card small.warn{color:var(--warning)}
  .warn-dot{display:inline-block;width:6px;height:6px;border-radius:50%;background:var(--warning);margin-inline-end:5px;vertical-align:middle}
</style>
