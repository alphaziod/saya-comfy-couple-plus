<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import GrokImport from "./GrokImport.svelte";
  import { onMount, onDestroy } from 'svelte';
  import { call, message, native, type Character, type CharacterCard } from '../lib/api';
  import CharacterAvatar from './CharacterAvatar.svelte';
  import LoraPicker from './LoraPicker.svelte';
  import { loraChoices } from '../lib/generation-workspace';
  import Thumbnail from './Thumbnail.svelte';
  import References from './References.svelte';
  import CharacterGenerator from './CharacterGenerator.svelte';
  let {initialId=null,onimage=()=>{},onbrowse=()=>{}}:{initialId?:string|null;onimage?:(character:string,asset:string)=>void;onbrowse?:(character:string)=>void}=$props();
  interface Variant { id: string; name: string; promptBlocks: Record<string,string>; constraints: Record<string,unknown>; revision: number }
  interface Revision { revision: number; name: string; identity: Record<string,unknown>; archived: boolean; variants: Variant[]; savedAt: string }
  const sections = ['QUALITY','P1','P2','ACTION','BG','NEG'];
  const sectionLabels: Record<string,string> = { QUALITY:'Qualité', P1:'Personnage 1', P2:'Personnage 2', ACTION:'Action', BG:'Décor', NEG:'Négatif' };
  let list = $state<Character[]>([]), search = $state(''), selected = $state<Character | null>(null), archived = $state(false);
  let name = $state(''), identity = $state(''), anatomy = $state(''), traits = $state(''), description = $state(''), loras = $state('[]');
  let error = $state(''), note = $state(''), busy = $state(false), exchange = $state('');
  let variants = $state<Variant[]>([]), history = $state<Revision[]>([]), restoring = $state<Revision | null>(null);
  let variantId = $state<string | null>(null), variantName = $state(''), blocks = $state<Record<string,string>>({}), constraints = $state('{}');
  let selectionSerial = 0;
  async function useSuggestion(value:{name:string;identity:Record<string,unknown>}) { await choose(null); name = value.name; identity = String(value.identity.identity_prompt); description = String(value.identity.description); generatedIdentity = value.identity; }
  let generatedIdentity = $state<Record<string,unknown>>({});
  // Accent- and case-insensitive search and French alphabetical order ("mèche" sits with "m").
  const fold = (v: string) => v.normalize('NFD').replace(/\p{M}/gu, '').toLocaleLowerCase('fr');
  let order = $state<'name'|'images'|'recent'>('name'), cards = $state<CharacterCard[]>([]), loraNames = $state<string[]|null>(null), loraLabels = $state<Record<string,{label:string;base:string}>>({});
  const cardOf = (id: string) => cards.find(c => c.id === id);
  const filtered = $derived(list.filter(c => (archived || !c.archived) && fold(c.name).includes(fold(search.trim())))
    .sort((a, b) => order === 'images' ? b.imageIds.length - a.imageIds.length || a.name.localeCompare(b.name, 'fr', { sensitivity: 'base' })
      : order === 'recent' ? b.revision - a.revision || a.name.localeCompare(b.name, 'fr', { sensitivity: 'base' })
      : a.name.localeCompare(b.name, 'fr', { sensitivity: 'base' })));
  // LoRA associations edited as rows; the stored value stays the same JSON list.
  const loraRows = $derived.by((): {name:string;trigger:string}[] => { try { const v = JSON.parse(loras); return Array.isArray(v) ? v : []; } catch { return []; } });
  const lorasValid = $derived.by(() => { try { return Array.isArray(JSON.parse(loras)); } catch { return false; } });
  function setLoraRows(rows: {name:string;trigger:string}[]) { loras = JSON.stringify(rows); }
  async function load() { [list, cards] = await Promise.all([call<Character[]>('characters', { include_archived: true }), call<CharacterCard[]>('character_cards').catch(() => [] as CharacterCard[])]); }
  function variant(v: Variant | null) { variantId = v?.id ?? null; variantName = v?.name ?? ''; blocks = v ? { ...v.promptBlocks } : (selected ? { P1: identity } : {}); constraints = JSON.stringify(v?.constraints ?? {}, null, 2); }
  async function choose(c: Character | null) {
    const ticket = ++selectionSerial; generatedIdentity = {}; selected = c; name = c?.name ?? ''; identity = String(c?.identity.identity_prompt ?? c?.identity.identityPrompt ?? ''); anatomy = String(c?.identity.anatomy_prompt ?? c?.identity.anatomyPrompt ?? ''); traits = String(c?.identity.studioTraits ?? c?.identity.traits ?? ''); description = String(c?.identity.description ?? ''); loras = JSON.stringify(c?.identity.loras ?? [], null, 2); error = ''; note = ''; exchange = ''; restoring = null; variants = []; history = []; variant(null);
    if (c) try { const [v,h] = await Promise.all([call<Variant[]>('variants',{id:c.id}),call<Revision[]>('character_history',{id:c.id})]); if (ticket === selectionSerial) { variants = v; history = h; variant(v.find(item => item.name === c.name) ?? null); } } catch(e) { if (ticket === selectionSerial) error = message(e); }
  }
  async function reload(id: string) { await load(); await choose(list.find(c => c.id === id) ?? null); }
  async function save() {
    if (busy) return; busy = true; error = '';
    try {
      const associations: unknown = JSON.parse(loras);
      if (!Array.isArray(associations) || !associations.every(l => l && typeof l === 'object' && typeof l.name === 'string' && typeof l.trigger === 'string')) throw new Error($t("LoRA : liste JSON de {\"name\":\"fichier\",\"trigger\":\"tags exacts\"}."));
      const data = { ...(selected?.identity ?? generatedIdentity), identity_prompt: identity, anatomy_prompt: anatomy, studioTraits: traits, traits: traits, description, loras:associations };
      const id = await call<string>('save_character', { id: selected?.id ?? null, name, identity: data }); await reload(id); note = $t("Fiche enregistrée ; révision conservée.");
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function archive() { if (!selected) return; error = ''; if (busy) return; busy = true; try { const id = selected.id; await call('archive_character', { id, archived: !selected.archived }); await reload(id); } catch(e) { error = message(e); } finally { busy = false; } }
  async function saveVariant() { if (!selected) return; error = ''; if (busy) return; busy = true; try { const id = selected.id; const saved = await call<string>('save_variant', { character:id,id:variantId,name:variantName,blocks:$state.snapshot(blocks),constraints:JSON.parse(constraints) }); await reload(id); variant(variants.find(v => v.id === saved) ?? null); note = $t("Variante et sections enregistrées sans normalisation des tags."); } catch(e) { error = message(e); } finally { busy = false; } }
  async function restore() { if (!selected || !restoring) return; error = ''; if (busy) return; busy = true; try { const id = selected.id; await call('restore_character',{id,revision:restoring.revision}); await reload(id); note = $t("Révision restaurée ; état précédent conservé dans l’historique."); } catch(e) { error = message(e); } finally { busy = false; } }
  async function exportRecord() { if (!selected) return; error = ''; try { exchange = JSON.stringify(await call('export_character',{id:selected.id}),null,2); } catch(e) { error = message(e); } }
  async function importRecord() { error = ''; if (busy) return; busy = true; try { const id = await call<string>('import_character',{text:exchange}); await reload(id); note = $t("Nouvelle fiche importée. Format v2 : ordre, principale et variantes conservés. Format v1 : hashes indexés uniquement."); } catch(e) { error = message(e); } finally { busy = false; } }
  async function importFile(event: Event) { const file = (event.target as HTMLInputElement).files?.[0]; if (!file) return; error = ''; if (file.size > 2_000_000) { error = $t("Fiche supérieure à 2 Mo"); return; } try { exchange = await file.text(); } catch(e) { error = $t("Lecture du fichier impossible : {error}", { error: message(e) }); } }
  async function copyExchange() { error = ''; try { await navigator.clipboard.writeText(exchange); note = $t("JSON copié dans le presse-papiers."); } catch(e) { error = $t("Copie impossible : {error}", { error: message(e) }); } }
  async function unlink(asset: string) { if (!selected) return; error = ''; try { const id = selected.id; await call('associate', { character: id, asset, add: false }); await reload(id); } catch (e) { error = message(e); } }
  let ready=$state(false), draftTimer:ReturnType<typeof setTimeout>|undefined;
  let draftWrite=Promise.resolve();
  function draft(){return {id:selected?.id??null,revision:selected?.revision??null,search,archived,order,name,identity,anatomy,traits,description,loras,generatedIdentity:$state.snapshot(generatedIdentity),variantId,variantName,blocks:$state.snapshot(blocks),constraints};}
  function persist(value:ReturnType<typeof draft>){draftWrite=draftWrite.then(()=>call('set_preference',{key:'characterEditor',value})).then(()=>{}).catch(e=>{error=message(e)});}
  $effect(()=>{const value=draft();if(!ready)return;clearTimeout(draftTimer);draftTimer=setTimeout(()=>persist(value),200)});
  onDestroy(()=>{clearTimeout(draftTimer);if(ready)persist(draft());++selectionSerial;});
  onMount(() => { if(native)void (async()=>{
    await load();const prefs=await call<any>('preferences'),d=prefs.characterEditor;
    const id=initialId??d?.id;const found=list.find(c=>c.id===id)??null;
    if(found){archived=found.archived;await choose(found);}
    if(d&&(!initialId||initialId===d.id)&&(!d.id||found?.revision===d.revision)){
      search=d.search??'';archived=d.archived??false;order=['name','images','recent'].includes(d.order)?d.order:'name';name=d.name??'';identity=d.identity??'';anatomy=d.anatomy??'';traits=d.traits??'';description=d.description??'';loras=d.loras??'[]';generatedIdentity=d.generatedIdentity??{};
      variantId=d.variantId??null;variantName=d.variantName??'';blocks=d.blocks??{};constraints=d.constraints??'{}';
    }
    // Open on a sheet instead of an empty creation form, unless a new sheet was being drafted.
    if(!selected&&!(d&&!d.id&&(d.name||d.identity)))await choose(filtered[0]??null);
    ready=true;
    try{const c=await call<any>('node_catalog');loraNames=loraChoices(c.nodes??[]);const l=await call<{labels:{path:string;label:string;base:string}[]}>('lora_labels');loraLabels=Object.fromEntries(l.labels.map(x=>[x.path,{label:x.label,base:x.base}]))}catch{/* Engine offline: LoRA names typed by hand. */}
  })().catch(e=>error=message(e)); });
</script>
{#snippet portraitOf(c: Character)}{@const card=cardOf(c.id)}{#if card?.portrait&&(!card.portraitStatus||card.portraitStatus.state==='present')}<Thumbnail id={card.portrait} preview alt={$t("Portrait principal de {name}",{name:c.name})} />{:else}<div class="portrait-empty" aria-hidden="true"><CharacterAvatar name={c.name} size={72} /></div>{/if}{/snippet}
<div class="wb chars">
<aside class="wb-panel left">
  <div class="wb-head"><h2>{$t("Fiches")}</h2><span class="chip num" title={list.some(c=>c.archived)?$tn("{n} archivée","{n} archivées",list.filter(c=>c.archived).length):undefined}>{$tn("{n} fiche active","{n} fiches actives",list.filter(c => !c.archived).length)}</span></div>
  <div class="list-filters">
    <input aria-label={$t("Rechercher un personnage")} placeholder={$t("Rechercher une fiche…")} bind:value={search} />
    <div class="list-tools"><select data-character-sort aria-label={$t("Trier")} title={$t("Trier")} bind:value={order}><option value="name">{$t("Nom A → Z")}</option><option value="images">{$t("Plus d’images")}</option><option value="recent">{$t("Plus modifiées")}</option></select><label class="checkbox"><input type="checkbox" bind:checked={archived} />{$t("Archives")}</label></div>
    <button type="button" class="new-btn" onclick={() => choose(null)}>{$t("+ Nouveau personnage")}</button>
  </div>
  <p class="list-count muted num">{$tn("{n} personnage","{n} personnages",filtered.length)}{search.trim()?' '+$t("pour « {query} »",{query:search.trim()}):''}</p>
  <div class="wb-scroll character-list">{#each filtered as c (c.id)}{@const card=cardOf(c.id)}<button class="character-item" class:chosen={selected?.id === c.id} class:archived={c.archived} aria-current={selected?.id === c.id ? 'true' : undefined} onclick={() => choose(c)}><CharacterAvatar id={card?.portraitStatus?.state==='present'||!card?.portraitStatus?card?.portrait:null} name={c.name} size={30} /><span class="item-text"><strong>{c.name}</strong><small class="num">{$tn("{n} image","{n} images",c.imageIds.length)}{card?.variantCount?` · ${$tn("{n} variante","{n} variantes",card.variantCount)}`:''}{c.archived ? ' · '+$t("archivée") : ''}</small></span></button>{:else}<p class="muted empty">{$t("Aucun personnage ne correspond.")}</p>{/each}</div>
  <div class="list-foot"><GrokImport onimported={load}/></div>
</aside>

<section class="wb-center">
  <div class="wb-bar sheet-bar">
    <strong class="sheet-name">{selected ? selected.name : $t("Créer un personnage")}</strong>
    {#if selected}<span class="muted num">{$t("Fiche · révision {n}",{n:selected.revision})}</span>{#if selected.archived}<span class="chip">{$t("archivée")}</span>{/if}{/if}
    <span class="grow"></span>
    {#if selected}<button type="button" class="quiet" disabled={busy} onclick={archive}>{selected.archived ? $t("Réactiver la fiche") : $t("Archiver la fiche")}</button>{/if}
    <button class="primary" type="submit" form="character-sheet-form" disabled={!native || busy || selected?.archived}>{$t("Enregistrer la fiche")}</button>
  </div>
  {#if error || note}<div class="msgline">{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}</div>{/if}
  <div class="wb-scroll character-form">
    <div class="sheet-top">
      <div class="fiche-portrait">
        {#if selected}{@render portraitOf(selected)}{:else}<div class="portrait-empty" aria-hidden="true"><CharacterAvatar name={name || '?'} size={72} /></div>{/if}
        {#if selected}<p class="muted num portrait-meta">{$tn("{n} image","{n} images",selected.imageIds.length)} · {$tn("{n} variante","{n} variantes",variants.length)}</p>{/if}
      </div>
      <form id="character-sheet-form" class="sheet-form" onsubmit={e => { e.preventDefault(); save(); }}>
        <label class="full">{$t("Nom")}<input required maxlength="300" bind:value={name} disabled={selected?.archived} /></label>
        <label class="full">{$t("Description")}<textarea rows="2" bind:value={description} disabled={selected?.archived}></textarea></label>
        <label class="full" title={$t("Prompt d’identité séparé, envoyé tel quel.")}>{$t("Identité / prompt complet")}<textarea rows="5" bind:value={identity} disabled={selected?.archived}></textarea></label>
        <label class="full" title={$t("Prompt anatomique séparé ; les deux prompts restent indépendants.")}>{$t("Anatomie / caractéristiques détaillées")}<textarea rows="3" bind:value={anatomy} disabled={selected?.archived}></textarea></label>
        <label>{$t("Caractéristiques et notes")}<textarea rows="3" bind:value={traits} disabled={selected?.archived}></textarea></label>
        <div class="lora-assoc" data-character-loras><span class="field-title" title={$t("Informatif : ces associations ne modifient aucun workflow.")}>{$t("LoRA et triggers de ce personnage")}</span>{#if lorasValid}{#each loraRows as row,i}<div class="assoc-row"><LoraPicker label="LoRA" value={row.name} names={loraNames} labels={loraLabels} disabled={selected?.archived} onpick={name=>setLoraRows(loraRows.map((r,j)=>j===i?{...r,name}:r))}/><input aria-label={$t("Trigger")} placeholder={$t("tags exacts du trigger")} value={row.trigger} disabled={selected?.archived} oninput={e=>setLoraRows(loraRows.map((r,j)=>j===i?{...r,trigger:e.currentTarget.value}:r))}/><button type="button" class="icon-btn quiet" aria-label={$t("Retirer {name}",{name:row.name})} title={$t("Retirer {name}",{name:row.name})} disabled={selected?.archived} onclick={()=>setLoraRows(loraRows.filter((_,j)=>j!==i))}>×</button></div>{/each}<LoraPicker label={$t("Ajouter une LoRA au personnage")} placeholder={$t("Ajouter une LoRA… (taper pour chercher)")} names={loraNames} labels={loraLabels} disabled={selected?.archived} onpick={name=>setLoraRows([...loraRows,{name,trigger:''}])}/>{:else}<textarea rows="3" bind:value={loras} disabled={selected?.archived}></textarea><small class="error">{$t("Liste illisible : corriger le JSON pour retrouver l’éditeur.")}</small>{/if}</div>
      </form>
    </div>
    <CharacterGenerator character={selected} onuse={useSuggestion} onsaved={async (character,id)=>{await reload(character);variant(variants.find(v=>v.id===id)??null);}} />
    {#if selected}
    <details class="sec" open><summary><span class="sec-title">{$t("Variantes et presets de prompts")}</span><span class="chip num">{variants.length}</span></summary><div class="sec-body">
      <div class="variant-tabs">{#each variants as v}<button type="button" class:on={variantId===v.id} aria-pressed={variantId===v.id} onclick={() => variant(v)}>{v.name}</button>{/each}<button type="button" class="quiet" onclick={() => variant(null)}>{$t("+ Nouvelle variante")}</button></div>
      <form class="variant-form" onsubmit={e => { e.preventDefault(); saveVariant(); }}><label class="full">{$t("Nom de variante")}<input required bind:value={variantName} /></label>{#each sections as section}<label title={section}>{$t(sectionLabels[section])}<textarea rows="2" bind:value={blocks[section]}></textarea></label>{/each}<details class="adv full"><summary>{$t("Contraintes avancées (JSON)")}</summary><textarea rows="2" bind:value={constraints}></textarea></details><div class="form-actions full"><button disabled={busy || selected.archived}>{$t("Enregistrer la variante")}</button></div></form>
    </div></details>
    <details class="sec" open><summary><span class="sec-title">{$t("Images de référence")}</span></summary><div class="sec-body">{#key `${selected.id}:${selected.revision}:${selected.imageIds.join(',')}`}<References character={selected} {variants} onimage={asset=>selected&&onimage(selected.id,asset)} onbrowse={()=>selected&&onbrowse(selected.id)} />{/key}</div></details>
    <details class="sec" open><summary title={$t("Ajoutez des références depuis la galerie ; filtrez la galerie par personnage pour toutes ses images.")}><span class="sec-title">{$t("Images associées · {n}",{n:selected.imageIds.length})}</span></summary><div class="sec-body"><div class="thumb-grid">{#each selected.imageIds as id}<div class="thumb"><button class="cover" onclick={()=>selected&&onimage(selected.id,id)} aria-label={$t("Ouvrir cette image")}><Thumbnail {id} /></button><button class="quiet unlink" disabled={selected.archived} onclick={() => unlink(id)}>{$t("Retirer le lien")}</button></div>{:else}<p class="muted">{$t("Aucune image associée.")}</p>{/each}</div></div></details>
    {/if}
  </div>
</section>

<aside class="wb-panel right">
  <div class="wb-head"><h2>{$t("Historique")}</h2>{#if selected}<span class="chip num">{$tn("{n} révision","{n} révisions",history.length)}</span>{/if}</div>
  <div class="wb-scroll">
    {#if selected}
      {#if restoring}<div class="confirm" role="group"><p>{$t("Restaurer la fiche et ses variantes depuis la révision {n} ? L’état actuel restera dans l’historique. Les associations d’images sont conservées.",{n:restoring.revision})}</p><div class="row"><button disabled={busy} onclick={restore}>{$t("Confirmer la restauration")}</button><button class="quiet" onclick={() => restoring = null}>{$t("Annuler")}</button></div></div>{/if}
      {#each history as revision}<div class="revision"><div class="rev-line"><span class="num">{$t("Révision {n}",{n:revision.revision})}</span><span class="muted num">{revision.savedAt}</span></div><div class="rev-name">{revision.name}</div><details><summary>{$t("Voir la fiche et ses variantes")}</summary><pre>{JSON.stringify(revision,null,2)}</pre></details><button class="quiet" disabled={busy || selected.archived} onclick={() => restoring = revision}>{$t("Préparer la restauration")}</button></div>{:else}<p class="muted pad">—</p>{/each}
    {:else}<p class="muted pad">—</p>{/if}
    <details class="sec exchange"><summary title={$t("Format v2 : variantes, ordre et image principale conservés. Indexez les images sur la destination avant l’import ; une référence absente bloque l’import entier. Le format v1 reste accepté avec ses limites. Les images ne sont jamais copiées.")}><span class="sec-title">{$t("Importer / exporter une fiche")}</span></summary><div class="sec-body">
      <div class="row"><button disabled={!selected || busy} onclick={exportRecord}>{$t("Afficher le JSON à exporter")}</button><button class="quiet" disabled={!exchange} onclick={copyExchange}>{$t("Copier")}</button></div>
      <label>{$t("Charger une fiche JSON")}<input type="file" accept=".json,application/json" onchange={importFile} /></label>
      <label>{$t("Fiche JSON à copier ou importer")}<textarea rows="8" bind:value={exchange}></textarea></label>
      <button disabled={!native || busy || !exchange} onclick={importRecord}>{$t("Importer comme nouvelle fiche")}</button>
    </div></details>
  </div>
</aside>
</div>

<style>
.chars{--wb-left:260px;--wb-right:280px;height:100%;overflow:hidden}
.chars label{display:flex;flex-direction:column;gap:4px;margin:0;color:var(--muted);font-size:12px}
.chars label.checkbox{flex-direction:row;align-items:center;gap:6px;white-space:nowrap}
.chars input[type=checkbox]{width:auto;min-height:0;margin:0}
.chars textarea{resize:vertical;min-height:0;line-height:1.5;color:var(--text)}
.chars details{margin-top:0}
.chars pre{font-size:11.5px;max-height:320px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:8px;margin:6px 0}
.grow{flex:1}
.row{display:flex;flex-wrap:wrap;gap:6px;align-items:center}
.pad{padding:12px;margin:0}
/* Left list */
.wb-head .chip{margin-inline-start:auto}
.list-filters{display:grid;gap:6px;padding:10px 12px;border-bottom:1px solid var(--line)}
.list-filters input:not([type=checkbox]){width:100%}
.list-tools{display:flex;gap:8px;align-items:center}.list-tools select{flex:1;min-width:0;width:auto}
.new-btn{width:100%}
.list-count{margin:0;padding:6px 12px 4px;font-size:11px}
.chars .character-list{position:static;max-height:none;resize:none;width:auto;min-width:0;max-width:none;overflow:auto;padding:0 6px 8px}
.chars .character-list .character-item{display:flex;align-items:center;gap:9px;width:100%;margin:0;padding:5px 6px;min-height:40px;text-align:start;background:transparent;border:0;border-radius:var(--radius)}
.chars .character-list .character-item:hover:not(:disabled){background:var(--hover)}
.chars .character-list .character-item.chosen{background:var(--raised);box-shadow:inset 2px 0 0 var(--sel)}
.character-item.archived{opacity:.55}
.item-text{display:flex;flex-direction:column;min-width:0}
.item-text strong{font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.chars .item-text small{display:block;font-size:11px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.empty{padding:8px 6px;margin:0}
.list-foot{padding:8px 12px;border-top:1px solid var(--line)}.list-foot :global(button[data-grok-open]){width:100%}
/* Center sheet */
.sheet-bar{background:var(--bg)}
.sheet-name{font-size:13.5px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:40%}
.msgline{padding:4px 12px;border-bottom:1px solid var(--line);background:var(--bg);font-size:12px}
.msgline p{margin:2px 0}.note{color:var(--muted);white-space:pre-wrap}
.chars .character-form{padding:0}
.sheet-top{display:grid;grid-template-columns:220px minmax(0,1fr);gap:16px;padding:16px;border-bottom:1px solid var(--line)}
.fiche-portrait{min-width:0}
.fiche-portrait :global(img),.fiche-portrait :global(.image-placeholder),.portrait-empty{display:flex;align-items:center;justify-content:center;width:100%;aspect-ratio:3/4;height:auto;max-height:none;min-height:0;object-fit:cover;object-position:top;border-radius:var(--r-img);background:var(--raised)}
.portrait-meta{margin:6px 0 0;font-size:11.5px}
.chars .sheet-form{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px 14px;align-content:start}
.chars .sheet-form label,.chars .variant-form label{margin:0;grid-column:auto}
.chars .sheet-form .full,.chars .variant-form .full{grid-column:1/-1}
.chars .sheet-form textarea{min-height:0}
.lora-assoc{display:flex;flex-direction:column;gap:6px;min-width:0}.field-title{font-size:12px;color:var(--muted)}
.assoc-row{display:grid;grid-template-columns:minmax(0,1.3fr) minmax(0,1fr) 28px;gap:6px;align-items:end}
.character-form :global(.sec>summary){padding:0 16px}.character-form :global(.sec>.sec-body){padding:2px 16px 14px}
.variant-tabs{display:flex;flex-wrap:wrap;gap:4px}
.variant-tabs button{min-height:26px;font-size:12px}
.variant-tabs button.on{background:var(--hover);border-color:var(--sel)}
.chars .variant-form{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px 14px}
.adv>summary{color:var(--muted);font-size:12px;margin-bottom:4px}.adv textarea{width:100%}
.form-actions{display:flex;justify-content:flex-end}
.thumb-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(110px,1fr));gap:10px}
.thumb{min-width:0;display:flex;flex-direction:column;gap:2px}
.cover{display:block;width:100%;aspect-ratio:3/4;padding:0;border:0;border-radius:var(--r-img);overflow:hidden;background:var(--raised)}
.cover :global(img),.cover :global(.image-placeholder){width:100%;height:100%;min-height:0;max-height:none;object-fit:cover;object-position:top}
.unlink{min-height:22px;font-size:11px;padding:0 4px}
/* Right inspector */
.revision{padding:8px 12px;border-bottom:1px solid var(--line);display:grid;gap:3px;justify-items:start}
.rev-line{display:flex;gap:8px;width:100%;justify-content:space-between;font-size:12px}.rev-line .muted{font-size:11px}
.rev-name{font-size:11.5px;color:var(--muted);max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.revision details>summary{font-size:11.5px;color:var(--muted)}
.revision button{min-height:24px;font-size:11.5px;padding:0 6px;margin-inline-start:-6px}
.confirm{padding:10px 12px;border-bottom:1px solid var(--line);background:var(--raised);font-size:12px}.confirm p{margin:0 0 8px}
.exchange{border-top:1px solid var(--line)}
.exchange input[type=file]{min-height:0;padding:4px;font-size:11.5px}
@media(max-width:1280px){.chars{--wb-left:220px;--wb-right:240px}.sheet-top{grid-template-columns:160px minmax(0,1fr)}}
@media(max-width:1000px){.sheet-top{grid-template-columns:1fr}.fiche-portrait{max-width:200px}.chars .sheet-form,.chars .variant-form{grid-template-columns:1fr}}
</style>
