<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { call, message, type FileStatus, type CharacterSheetData } from '../lib/api';
  import Thumbnail from './Thumbnail.svelte';
  let { characterId, onback = () => {}, onimage = () => {} }: { characterId: string; onback?: () => void; onimage?: (character: string, asset: string) => void } = $props();
  let sheet = $state<CharacterSheetData | null>(null);
  let error = $state(''), loading = $state(false);
  let serial = 0;
  $effect(() => {
    const id = characterId;
    const ticket = ++serial;
    loading = true; error = ''; sheet = null;
    call<CharacterSheetData>('character_sheet', { id })
      .then(value => { if (ticket === serial) sheet = value; })
      .catch(e => { if (ticket === serial) error = message(e); })
      .finally(() => { if (ticket === serial) loading = false; });
  });
  let choices=$state<{id:string;path:string;status:FileStatus}[]|null>(null),choiceTotal=$state(0),choiceOffset=$state(0),choosing=$state(false);
  async function candidates(offset=0){choosing=true;error='';try{const r=await call<{items:NonNullable<typeof choices>;total:number}>('portrait_candidates',{character:characterId,offset});choices=r.items;choiceTotal=r.total;choiceOffset=offset;}catch(e){error=message(e)}finally{choosing=false}}
  async function choosePortrait(asset:string){choosing=true;error='';try{await call('choose_portrait',{character:characterId,asset});sheet=await call<CharacterSheetData>('character_sheet',{id:characterId});choices=null;}catch(e){error=message(e)}finally{choosing=false}}
  function shortName(path: string): string { return path.split(/[\\/]/).pop() ?? path; }
  const referenceScopes = $derived.by(() => {
    if (!sheet) return [];
    const scopes: { id: string; label: string; items: CharacterSheetData['references'] }[] = [];
    const main = sheet.references.filter(r => !r.variantId);
    if (main.length) scopes.push({ id: '', label: $t("Fiche"), items: main });
    for (const variant of sheet.variants) {
      const items = sheet.references.filter(r => r.variantId === variant.id);
      if (items.length) scopes.push({ id: variant.id, label: variant.name, items });
    }
    for (const reference of sheet.references) {
      if (!reference.variantId || scopes.some(scope => scope.id === reference.variantId)) continue;
      scopes.push({
        id: reference.variantId,
        label: $t("Variante historique · {name}", { name: reference.variantName || reference.variantId }),
        items: sheet.references.filter(item => item.variantId === reference.variantId),
      });
    }
    return scopes;
  });
</script>
<div class="sheet">
<div class="sheet-titlebar">
  <button class="quiet" onclick={onback}>{$t("← Toutes les fiches")}</button>
  <h1>{sheet ? sheet.name : $t("Fiche personnage")}</h1>{#if sheet}<span class="muted num">{$tn("{n} image associée", "{n} images associées", sheet.imageCount)} · {$tn("{n} visible", "{n} visibles", sheet.visibleImageCount)} · {$t("révision {n}", { n: sheet.revision })}</span>{#if sheet.archived}<span class="chip">{$t("fiche archivée")}</span>{/if}{/if}
</div>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if loading}<p class="muted" role="status">{$t("Chargement de la fiche…")}</p>{/if}
{#if sheet}
  <div class="sheet-layout">
    <aside class="sheet-identity">
      <div class="sheet-portrait">{#if sheet.portrait&&sheet.portraitStatus?.state==='present'}<Thumbnail id={sheet.portrait} preview alt={$t("Portrait principal de {name}", { name: sheet.name })} />{:else}<div class="image-placeholder">{$t("Aucun portrait")}</div>{/if}</div>
      {#if sheet.portraitStatus&&sheet.portraitStatus.state!=='present'}<p class="error" data-portrait-reason>{sheet.portraitStatus.state==='missing'?$t("Portrait manquant"):$t("Portrait inaccessible")} : {sheet.portraitStatus.reason}</p><p class="hash">{sheet.portraitStatus.path}</p>{/if}
      <button data-choose-portrait disabled={choosing||sheet.archived} onclick={()=>candidates()} title={$t("Choix explicite parmi les images associées. L’ancienne référence reste conservée.")}>{$t("Choisir un portrait…")}</button>
      {#if choices}<div class="choices" title={$t("Aucun fichier disponible ? Ajoutez sa source dans la galerie, puis associez explicitement l’image à ce personnage.")}><div class="choice-grid">{#each choices as choice}<button class="choice" data-portrait-option={choice.id} disabled={choosing||choice.status.state!=='present'} onclick={()=>choosePortrait(choice.id)} title={choice.status.reason?`${shortName(choice.path)} · ${choice.status.reason}`:shortName(choice.path)}>{#if choice.status.state==='present'}<Thumbnail id={choice.id} alt={choice.path}/>{:else}<small>{choice.status.state}</small>{/if}</button>{/each}</div><div class="row"><button class="quiet" disabled={!choiceOffset||choosing} onclick={()=>candidates(Math.max(0,choiceOffset-48))} aria-label={$t("Choix précédents")} title={$t("Choix précédents")}>‹</button><span class="muted num">{choiceOffset+1}–{Math.min(choiceOffset+48,choiceTotal)} / {choiceTotal}</span><button class="quiet" disabled={choiceOffset+48>=choiceTotal||choosing} onclick={()=>candidates(choiceOffset+48)} aria-label={$t("Choix suivants")} title={$t("Choix suivants")}>›</button><span class="grow"></span><button class="quiet" onclick={()=>choices=null}>{$t("Annuler le choix")}</button></div></div>{/if}
      {#if sheet.description}<h2>{$t("Description")}</h2><p class="prompt-block">{sheet.description}</p>{/if}
      {#if sheet.traits}<h2>{$t("Caractéristiques permanentes")}</h2><p class="prompt-block">{sheet.traits}</p>{/if}
      <h2>{$t("Prompt P1 complet")}</h2><p class="prompt-block" data-p1>{sheet.identityPrompt || '—'}</p>
      <h2>{$t("Prompt P1 anatomique")}</h2><p class="prompt-block" data-anatomy>{sheet.anatomyPrompt || '—'}</p>
    </aside>
    <div class="sheet-body">
      <h2 title={$t("Classement uniquement par données existantes : favoris, puis validations (validée, à examiner, rejetée), puis plus récentes. Aucun score calculé par l’application.")}>{$t("10 meilleures images")}</h2>
      {#if sheet.topImages.length}
        <div class="reference-grid top" data-top-images>{#each sheet.topImages as image (image.id)}<button class="tile" data-top-image={image.id} onclick={() => onimage(sheet!.id, image.id)} aria-label={$t("Ouvrir {name}", { name: shortName(image.path) })}><span class="cover"><Thumbnail id={image.id} alt={shortName(image.path)} /></span><small>{image.favorite ? '★ ' : ''}{shortName(image.path)} · {image.review}</small></button>{/each}</div>
      {:else}<p class="muted">{$t("Aucune image associée disponible dans les bibliothèques activées.")}</p>{/if}
      <h2>{$t("Variantes ({n})", { n: sheet.variants.length })}</h2>
      <div class="variants">{#each sheet.variants as variant (variant.id)}
        <details data-variant={variant.id}><summary>{variant.name} <span class="muted num">· {$t("révision {n}", { n: variant.revision })}</span></summary>
          {#each ['QUALITY','P1','P2','ACTION','BG','NEG'] as section}{#if variant.promptBlocks[section]}<p class="prompt-block"><strong>{section}</strong> · {variant.promptBlocks[section]}</p>{/if}{/each}
        </details>
      {/each}</div>
      <h2>{$t("Décor (BG) par variante")}</h2>
      {#if sheet.backgrounds.length}{#each sheet.backgrounds as bg (bg.variantId)}<p class="prompt-block"><strong>{bg.variantName}</strong> · {bg.bg}</p>{/each}{:else}<p class="muted">{$t("Aucun prompt BG enregistré dans les variantes de cette fiche.")}</p>{/if}
      <h2>{$t("Images de référence")}</h2>
      {#if referenceScopes.length}
        {#each referenceScopes as scope (scope.id)}
          <h3>{scope.label}</h3>
          <div class="reference-grid">{#each scope.items as reference (reference.assetId)}<button class="tile" onclick={() => onimage(sheet!.id, reference.assetId)} aria-label={$t("Ouvrir la référence {name}", { name: shortName(reference.assetId) })}><span class="cover" class:primary={reference.primary}><Thumbnail id={reference.assetId} alt={$t("Référence")} /></span>{#if reference.primary}<small>{$t("Principale")}{reference.variantName ? ` · ${reference.variantName}` : ''}</small>{/if}{#if reference.availability !== 'present'}<small class="warn">{$t("Indisponible")}</small>{/if}</button>{/each}</div>
        {/each}
      {:else}<p class="muted">{$t("Aucune image de référence définie.")}</p>{/if}
    </div>
  </div>
{/if}
</div>
<style>
  .sheet{display:grid;gap:10px;align-content:start}
  .sheet-titlebar{display:flex;align-items:center;gap:10px;min-height:32px;padding-bottom:10px;border-bottom:1px solid var(--line)}
  .sheet-titlebar h1{font-size:14px;font-weight:600;margin:0;line-height:1.3}
  .sheet-titlebar button{padding:0 8px;margin-inline-start:-8px}
  .grow{flex:1}
  .row{display:flex;align-items:center;gap:4px}
  .sheet .sheet-layout{display:grid;grid-template-columns:280px minmax(0,1fr);gap:24px;align-items:start}
  .sheet-identity{display:grid;gap:6px;min-width:0}
  .sheet-identity>button{justify-self:start}
  .sheet .sheet-portrait :global(img),.sheet .sheet-portrait :global(.image-placeholder){display:flex;align-items:center;justify-content:center;width:100%;aspect-ratio:3/4;height:auto;max-height:none;min-height:0;object-fit:cover;object-position:top;border-radius:var(--r-img);background:var(--raised);color:var(--muted)}
  .error,.hash{margin:0}
  .choices{display:grid;gap:6px;padding:8px;border:1px solid var(--line);border-radius:var(--radius);background:var(--panel)}
  .choice-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(60px,1fr));gap:4px;max-height:300px;overflow:auto}
  .choice{display:block;width:100%;aspect-ratio:3/4;padding:0;border:0;border-radius:var(--r-img);overflow:hidden;background:var(--raised);font-size:10px;color:var(--muted)}
  .choice :global(img),.choice :global(.image-placeholder){width:100%;height:100%;min-height:0;object-fit:cover;object-position:top}
  .choices button.quiet{min-height:24px}
  h2{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);margin:14px 0 4px}
  .sheet-identity h2:first-of-type{margin-top:10px}
  .sheet-body h2:first-child{margin-top:0}
  h3{font-size:12px;font-weight:500;margin:10px 0 6px;color:var(--text)}
  .sheet .prompt-block{margin:0 0 4px;padding:8px 10px;font-size:12px;line-height:1.5;white-space:pre-wrap;overflow-wrap:anywhere;background:var(--panel);border:1px solid var(--line);border-radius:var(--radius)}
  .prompt-block strong{font-weight:600;color:var(--muted)}
  .variants{border-top:1px solid var(--line)}
  .variants details{margin:0;border-bottom:1px solid var(--line)}
  .variants summary{min-height:30px;display:flex;align-items:center;gap:6px;cursor:pointer}
  .variants details[open]{padding-bottom:6px}
  .sheet .reference-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(120px,1fr));gap:12px 10px}
  .tile{display:flex;flex-direction:column;gap:3px;min-width:0;min-height:0;padding:0;border:0;background:transparent;text-align:start}
  .tile:hover:not(:disabled){background:transparent}
  .cover{display:block;width:100%;aspect-ratio:3/4;border-radius:var(--r-img);overflow:hidden;background:var(--raised)}
  .tile:hover .cover{outline:2px solid var(--sel);outline-offset:2px}
  .cover.primary{outline:2px solid var(--accent);outline-offset:2px}
  .cover :global(img),.cover :global(.image-placeholder){width:100%;height:100%;min-height:0;max-height:none;object-fit:cover;object-position:top}
  .tile small{font-size:11px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.tile small.warn{color:var(--warning)}
  @media(max-width:1100px){.sheet .sheet-layout{grid-template-columns:220px minmax(0,1fr)}}
  @media(max-width:760px){.sheet .sheet-layout{grid-template-columns:1fr}}
</style>
