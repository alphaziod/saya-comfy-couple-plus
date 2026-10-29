<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { stateLabel } from '../lib/state-label';
  import { parseLoras } from '../lib/ui-helpers';
  import { untrack } from 'svelte';
  import { call, message, type PreparedJob } from '../lib/api';
  import Thumbnail from './Thumbnail.svelte';
  import PreparedDraft from './PreparedDraft.svelte';

  interface AbValue { id: string; value: unknown; position: number }
  interface AbParameter { id: string; slot: string; kind: string; label: string; position: number; values: AbValue[] }
  interface AbImage { jobId: string; assetId: string; nodeId: string | null; outputIndex: number | null; kind: string }
  interface AbVerdict { favorite: boolean; best: boolean; rank: number | null; note: string }
  interface AbCell { id: string; position: number; config: Record<string, unknown>; choices: Record<string, string>; images: AbImage[]; verdict: AbVerdict | null }
  interface AbPreset { id: string; name: string; cellId: string | null; config: Record<string, unknown>; createdAt: string }
  interface AbWorkflow { id: string; name: string; promptProfile: string }
  interface AbCampaignSummary { id: string; name: string; workflow: string; createdAt: string; cells: number; parameters: number }
  interface AbCampaign {
    id: string; name: string;
    workflow: { id: string; name: string; sourceSha256: string };
    model: string; loras: unknown[]; prompts: Record<string, string>;
    baseSeed: string; steps: number; cfg: number; sampler: string; gridLimit: number; notes: string; createdAt: string;
    parameters: AbParameter[]; cells: AbCell[]; presets: AbPreset[];
  }
  interface DiffEntry { key: string; a: unknown; b: unknown; differs: boolean }
  interface AbDiff { entries: DiffEntry[]; differing: string[]; attribution: { allowed: boolean; slot: string | null; reason: string }; cellA: string; cellB: string }

  const SLOTS = ['model', 'loras', 'prompt_P1', 'prompt_P2', 'prompt_QUALITY', 'prompt_ACTION', 'prompt_BG', 'prompt_NEG', 'base_seed', 'steps', 'cfg', 'sampler'] as const;
  const LABELS: Record<string, string> = {
    model: 'Modèle', loras: 'LoRA', promptP1: 'Prompt P1', promptP2: 'Prompt P2', promptQUALITY: 'Prompt QUALITY',
    promptACTION: 'Prompt ACTION', promptBG: 'Prompt BG', promptNEG: 'Prompt NEG', baseSeed: 'Seed de base',
    steps: 'Steps', cfg: 'CFG', sampler: 'Sampler',
    // snake_case keys of the settings (SLOTS): same French labels.
    prompt_P1: 'Prompt P1', prompt_P2: 'Prompt P2', prompt_QUALITY: 'Prompt QUALITY', prompt_ACTION: 'Prompt ACTION',
    prompt_BG: 'Prompt BG', prompt_NEG: 'Prompt NEG', base_seed: 'Seed de base'
  };

  let campaigns = $state<AbCampaignSummary[]>([]), workflows = $state<AbWorkflow[]>([]);
  let current = $state<AbCampaign | null>(null);
  let busy = $state(false), error = $state(''), note = $state('');
  let form = $state({ name: '', workflow: '', model: '', baseSeed: '0', steps: 20, cfg: 4.5, sampler: 'euler', gridLimit: 16, notes: '', promptP1: '', promptP2: '', promptQUALITY: '', promptACTION: '', promptBG: '', promptNEG: '', loras: '' });
  let param = $state<{ slot: string; kind: string; label: string; values: string }>({ slot: 'cfg', kind: 'variant', label: '', values: '[4.5, 7]' });
  let drafts = $state<Record<string, { favorite: boolean; best: boolean; rank: number | null; note: string }>>({});
  let diffA = $state(''), diffB = $state(''), diff = $state<AbDiff | null>(null);
  let imageForm = $state<{ cell: string; job: string; asset: string }>({ cell: '', job: '', asset: '' });
  let presetName = $state(''), presetCell = $state('');
  // Runner link: plan per cell, explicit selection, drafts confirmed one by one.
  interface PlanCell { cellId: string; position: number; edits: { nodeId: string; inputName: string; value: string; section: string | null }[]; problems: string[]; jobs: { id: string; state: string }[] }
  interface AbPlan { ready: boolean; problems: string[]; cells: PlanCell[] }
  let plan = $state<AbPlan | null>(null), chosen = $state<string[]>([]), abDrafts = $state<{ cellId: string; position: number; prepared: PreparedJob }[]>([]);
  let recipes = $state<{ id: string; name: string }[]>([]), fromRecipe = $state('');
  const planCell = (id: string) => plan?.cells.find(c => c.cellId === id) ?? null;

  // Only reset the selections if the campaign changes or the chosen cell has disappeared.
  let lastCampaign = '';
  $effect(() => {
    if (!current) { lastCampaign = ''; return; }
    const campaign = current.id, cells = current.cells.map(cell => cell.id);
    untrack(() => {
      const fresh = campaign !== lastCampaign;
      lastCampaign = campaign;
      const valid = (value: string) => cells.includes(value);
      if (fresh || !valid(imageForm.cell)) imageForm.cell = cells[0] ?? '';
      if (fresh || !valid(presetCell)) presetCell = cells[0] ?? '';
      if (fresh || !valid(diffA)) diffA = cells[0] ?? '';
      if (fresh || !valid(diffB)) diffB = cells[1] ?? '';
    });
  });
  // Initial load: campaign list and workflow reference list for the creation form.
  $effect(() => { refresh().catch(e => { error = message(e); }); });

  async function refresh() {
    [campaigns, workflows, recipes] = await Promise.all([call<AbCampaignSummary[]>('ab_campaigns'), call<AbWorkflow[]>('workflows'), call<{ id: string; name: string }[]>('recipes')]);
  }
  async function open(id: string) {
    busy = true; error = ''; note = ''; diff = null;
    try {
      current = await call<AbCampaign>('ab_campaign', { id });
      const next: typeof drafts = {};
      for (const cell of current.cells) next[cell.id] = { favorite: cell.verdict?.favorite ?? false, best: cell.verdict?.best ?? false, rank: cell.verdict?.rank ?? null, note: cell.verdict?.note ?? '' };
      drafts = next;
      if (plan && !current.cells.some(c => c.id === plan!.cells[0]?.cellId)) { plan = null; chosen = []; abDrafts = []; }
      await refresh();
      if (current.cells.length) await loadPlan();
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  function back() { current = null; diff = null; refresh().catch(e => { error = message(e); }); }
  async function createCampaign() {
    if (busy) return; busy = true; error = ''; note = '';
    try {
      const prompts: Record<string, string> = {};
      const texts: Record<string, string> = { P1: form.promptP1, P2: form.promptP2, QUALITY: form.promptQUALITY, ACTION: form.promptACTION, BG: form.promptBG, NEG: form.promptNEG };
      for (const [section, text] of Object.entries(texts)) if (text.trim()) prompts[section] = text;
      const loras = parseLoras(form.loras);
      const id = await call<string>('save_ab_campaign', {
        name: form.name, workflow: form.workflow, model: form.model, loras, prompts,
        base_seed: form.baseSeed.trim(), steps: form.steps, cfg: form.cfg, sampler: form.sampler,
        grid_limit: form.gridLimit, notes: form.notes
      });
      note = $t("Campagne enregistrée. Aucune génération n’a été soumise.");
      await open(id);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function addParameter() {
    if (!current) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      const values: unknown[] = JSON.parse(param.values);
      await call('add_ab_parameter', { campaign: current.id, slot: param.slot, kind: param.kind, label: param.label, values });
      note = $t("Paramètre enregistré.");
      await open(current.id);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function loadPlan() {
    if (!current) return;
    try {
      const value = await call<AbPlan>('ab_plan', { campaign: current.id });
      plan = value;
      chosen = chosen.filter(id => value.cells.some(c => c.cellId === id && !c.problems.length));
    } catch (e) { error = message(e); }
  }
  function toggleCell(id: string) { chosen = chosen.includes(id) ? chosen.filter(c => c !== id) : [...chosen, id]; }
  async function prepareCells() {
    if (!current || !chosen.length) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      const result = await call<{ drafts: { cellId: string; position: number; prepared: PreparedJob }[] }>('prepare_ab_cells', { campaign: current.id, cells: chosen });
      abDrafts = [...abDrafts, ...result.drafts];
      note = $tn("{n} brouillon préparé. Aucun envoi : chaque job exige sa propre confirmation ci-dessous.", "{n} brouillons préparés. Aucun envoi : chaque job exige sa propre confirmation ci-dessous.", result.drafts.length);
      chosen = [];
      await loadPlan();
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function syncResults() {
    if (!current) return;
    if (busy) return; busy = true; error = '';
    try {
      const result = await call<{ registered: number }>('sync_ab_results', { campaign: current.id });
      await open(current.id);
      note = $tn("{n} image de résultat rattachée à sa cellule (provenance job/node, aucune copie).", "{n} images de résultat rattachées à leur cellule (provenance job/node, aucune copie).", result.registered);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function recipeFromJob(job: string, position: number) {
    if (!current) return;
    if (busy) return; busy = true; error = '';
    try {
      await call('create_recipe', { job, name: `${current.name} · cellule ${position + 1}`, description: 'Exportée depuis le laboratoire A/B' });
      note = $t("Recette créée depuis le job de la cellule {n}.", { n: position + 1 });
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function fillFromRecipe() {
    if (!fromRecipe) return;
    error = '';
    try {
      const spec = await call<{ name: string; workflow: string; model: string; loras: { name: string; weightModel: number | null }[]; prompts: Record<string, string>; baseSeed: string; steps: number | null; cfg: number | null; sampler: string | null }>('ab_spec_from_recipe', { recipe: fromRecipe });
      form.name = spec.name; form.workflow = spec.workflow; form.model = spec.model;
      if (spec.baseSeed && spec.baseSeed !== 'null') form.baseSeed = spec.baseSeed;
      if (spec.steps !== null) form.steps = Number(spec.steps);
      if (spec.cfg !== null) form.cfg = Number(spec.cfg);
      if (spec.sampler) form.sampler = String(spec.sampler);
      for (const section of ['P1', 'P2', 'QUALITY', 'ACTION', 'BG', 'NEG'] as const) form[`prompt${section}`] = spec.prompts[section] ?? '';
      form.loras = spec.loras.filter(l => l.weightModel !== null).map(l => `${l.name}:${l.weightModel}`).join('\n');
      note = $t("Formulaire prérempli depuis la recette : vérifiez puis créez la campagne.");
    } catch (e) { error = message(e); }
  }
  async function prepareGrid() {
    if (!current) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      const report = await call<{ cells: number; limit: number }>('prepare_ab_grid', { campaign: current.id });
      note = $t("Grille préparée : {cells} cellule(s) (limite {limit}). Aucune génération n’a été soumise.", { cells: report.cells, limit: report.limit });
      await open(current.id);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function saveVerdict(cellId: string) {
    if (!current) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      const draft = drafts[cellId];
      await call('set_ab_verdict', { campaign: current.id, cell: cellId, favorite: draft.favorite, best: draft.best, rank: draft.rank ?? null, note: draft.note });
      await open(current.id);
      note = $t("Verdict manuel enregistré.");
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function registerImage() {
    if (!current) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      await call('register_ab_cell_image', { campaign: current.id, cell: imageForm.cell, job: imageForm.job.trim(), asset: imageForm.asset.trim() });
      note = $t("Image référencée par provenance ; le fichier original n’a ni bougé ni été copié.");
      await open(current.id);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function runDiff() {
    if (!current) return;
    if (busy) return; busy = true; error = ''; note = ''; diff = null;
    try { diff = await call<AbDiff>('ab_cell_diff', { campaign: current.id, cell_a: diffA, cell_b: diffB }); }
    catch (e) { error = message(e); } finally { busy = false; }
  }
  async function savePreset(cellId: string) {
    if (!current) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      await call('save_ab_winner_preset', { campaign: current.id, cell: cellId, name: presetName });
      presetName = '';
      note = $t("Configuration gagnante mémorisée ; réutilisation via les recettes à venir.");
      await open(current.id);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  function variantValue(cell: AbCell, slot: string): string {
    const parameter = current?.parameters.find(p => p.slot === slot);
    const valueId = cell.choices[slot];
    const found = parameter?.values.find(v => v.id === valueId);
    if (!found) return '—';
    return typeof found.value === 'string' ? found.value : JSON.stringify(found.value);
  }
  function formatValue(value: unknown): string {
    if (value === null || value === undefined) return '—';
    if (typeof value === 'string') return value === '' ? '(vide)' : value;
    return JSON.stringify(value);
  }
</script>
<div class="ab" data-ab-lab>
<aside class="wb-panel left">
 <header class="wb-head"><h2>{$t("Campagnes")}</h2><span class="chip num">{campaigns.length}</span>{#if current}<button disabled={busy} onclick={back}>{$t("Nouvelle campagne")}</button>{/if}</header>
 <div class="wb-scroll">
  {#if !campaigns.length}<p class="note pad">{$t("Aucune campagne. Créez la première ci-dessous.")}</p>{/if}
  {#each campaigns as campaign (campaign.id)}
   <button class="ab-campaign" class:on={current?.id===campaign.id} data-open-campaign={campaign.id} disabled={busy} onclick={() => open(campaign.id)}>
    <strong>{campaign.name}</strong>
    <small class="num">{campaign.workflow} · {$tn("{n} paramètre", "{n} paramètres", campaign.parameters)} · {$tn("{n} cellule", "{n} cellules", campaign.cells)} · {new Date(campaign.createdAt).toLocaleString()}</small>
   </button>
  {/each}
  {#if current}
   <details class="sec ref" open><summary><span class="sec-title">{$t("Référence")}</span></summary>
    <div class="sec-body">
     <p class="note">{$t("Workflow de référence : {name} · révision", { name: current.workflow.name })} <span class="hash">{current.workflow.id}</span> {$t("· empreinte source")} <span class="hash">{current.workflow.sourceSha256}</span></p>
     <p class="note num">{$t("Référence : modèle {model} · seed {seed} · {steps} steps · CFG {cfg} · {sampler} · limite de grille {limit}", { model: current.model || '—', seed: current.baseSeed, steps: current.steps ?? '', cfg: current.cfg ?? '', sampler: current.sampler || 'sampler —', limit: current.gridLimit })}</p>
    </div>
   </details>
   <details class="sec" open><summary><span class="sec-title">{$t("Paramètres")}</span><span class="chip num">{current.parameters.length}</span></summary>
    <div class="sec-body">
     {#if !current.parameters.length}<p class="note">{$t("Aucun paramètre déclaré : la grille serait d’une seule cellule identique à la référence.")}</p>{/if}
     <ul class="ab-params">
      {#each current.parameters as parameter (parameter.id)}
       <li><strong>{parameter.label}</strong> <span class="chip">{parameter.kind === 'constant' ? $t("constant") : $t("variante")}</span><span class="note">{$t("réglage")} {$t(LABELS[parameter.slot] ?? parameter.slot)} — {parameter.values.map(v => formatValue(v.value)).join(' | ')}</span></li>
      {/each}
     </ul>
     <details class="inline"><summary>{$t("Ajouter un paramètre (constant ou variante)")}</summary>
      <div class="form-grid">
       <label>{$t("Réglage")}<select bind:value={param.slot} disabled={busy}>{#each SLOTS as slot}<option value={slot}>{LABELS[slot] ?? slot}</option>{/each}</select></label>
       <label>{$t("Genre")}<select bind:value={param.kind} disabled={busy}><option value="constant">{$t("constant (1 valeur)")}</option><option value="variant">{$t("variante (≥ 2 valeurs)")}</option></select></label>
       <label>{$t("Libellé")}<input bind:value={param.label} disabled={busy} maxlength="300" placeholder={$t("Guidage")} /></label>
       <label title={$t("Les seeds sont des chaînes u64 décimales exactes (pas de zéro de tête). Un même réglage ne peut être déclaré qu’une fois.")}>{$t("Valeurs (tableau JSON, ex. [4.5, 7] ou [\"euler\",\"dpmpp_2m\"] ou [\"portrait A\",\"portrait B\"])")}<textarea bind:value={param.values} disabled={busy} rows="2"></textarea></label>
       <button disabled={busy || !param.label.trim()} onclick={addParameter}>{$t("Ajouter le paramètre")}</button>
      </div>
     </details>
    </div>
   </details>
   {#if current.cells.length}
   <details class="sec" open={!!current.presets.length}><summary><span class="sec-title">{$t("Presets gagnants")}</span><span class="chip num">{current.presets.length}</span></summary>
    <div class="sec-body">
     {#if !current.presets.length}<p class="note">{$t("Aucun preset sauvegardé pour cette campagne.")}</p>{/if}
     <ul class="ab-params">
      {#each current.presets as preset (preset.id)}
       <li title={$t("configuration copiée exactement depuis la cellule.")}><strong>{preset.name}</strong> <span class="note num">{preset.createdAt != null ? new Date(preset.createdAt).toLocaleString() : ''}</span></li>
      {/each}
     </ul>
     <div class="form-grid">
      <label>{$t("Nom du preset pour la configuration gagnante")}<input bind:value={presetName} disabled={busy} maxlength="300" placeholder={$t("Portrait extérieur, CFG 7")} /></label>
      <label>{$t("Cellule source de la configuration")}<select bind:value={presetCell} disabled={busy}>{#each current.cells as cell (cell.id)}<option value={cell.id}>{$t("Cellule {n}", { n: cell.position + 1 })}</option>{/each}</select></label>
      <button disabled={busy || !presetName.trim() || !presetCell} onclick={() => savePreset(presetCell)}>{$t("Mémoriser cette configuration")}</button>
     </div>
    </div>
   </details>
   <details class="sec ab-register"><summary title={$t("L’image doit déjà avoir une provenance enregistrée (sortie d’un job du workflow de référence). Le fichier original n’est jamais copié ni déplacé.")}><span class="sec-title">{$t("Référencer une image existante (provenance job / node / indice)")}</span></summary>
    <div class="sec-body">
     <label>{$t("Cellule")}<select bind:value={imageForm.cell} disabled={busy}>{#each current.cells as cell (cell.id)}<option value={cell.id}>{$t("Cellule {n}", { n: cell.position + 1 })}</option>{/each}</select></label>
     <label>{$t("Identifiant du job")}<input bind:value={imageForm.job} disabled={busy} placeholder={$t("uuid du job")} /></label>
     <label>{$t("Identifiant de l’asset")}<input bind:value={imageForm.asset} disabled={busy} placeholder={$t("uuid de l’asset")} /></label>
     <button disabled={busy || !imageForm.cell || !imageForm.job.trim() || !imageForm.asset.trim()} onclick={registerImage}>{$t("Référencer cette image")}</button>
    </div>
   </details>
   {/if}
  {/if}
 </div>
</aside>

<section class="ab-main">
{#if !current}
 <div class="wb-bar"><h2>{$t("Nouvelle campagne")}</h2><span class="grow"></span>
  <label class="inline-label">{$t("Préremplir depuis une recette")}<select bind:value={fromRecipe} disabled={busy} data-ab-from-recipe><option value="">{$t("Choisir…")}</option>{#each recipes as recipe (recipe.id)}<option value={recipe.id}>{recipe.name}</option>{/each}</select></label><button disabled={busy || !fromRecipe} onclick={fillFromRecipe}>{$t("Préremplir")}</button>
 </div>
 {@render messages()}
 <div class="wb-scroll">
  <div class="create" title={$t("La campagne fige un workflow de référence (nom + empreinte de la source), une configuration de référence et la frontière explicite entre paramètres constants et variantes.")}>
   <div class="cols2">
    <label>{$t("Nom de la campagne")}<input bind:value={form.name} disabled={busy} maxlength="300" /></label>
    <label>{$t("Workflow de référence (révision figée)")}<select bind:value={form.workflow} disabled={busy}><option value="">{$t("Choisir…")}</option>{#each workflows as workflow (workflow.id)}<option value={workflow.id}>{workflow.name} ({workflow.promptProfile})</option>{/each}</select></label>
   </div>
   <label>{$t("Modèle (checkpoint)")}<input bind:value={form.model} disabled={busy} maxlength="500" placeholder={$t("modeles/mon_modele.safetensors")} /></label>
   <div class="cols5">
    <label>{$t("Seed de base (u64 exact, texte décimal)")}<input class="num" bind:value={form.baseSeed} disabled={busy} /></label>
    <label>{$t("Steps")}<input class="num" type="number" min="1" max="1000000" step="1" bind:value={form.steps} disabled={busy} /></label>
    <label>{$t("CFG")}<input class="num" type="number" min="0" step="0.1" bind:value={form.cfg} disabled={busy} /></label>
    <label>{$t("Sampler")}<input bind:value={form.sampler} disabled={busy} maxlength="500" /></label>
    <label>{$t("Limite de grille (1–512 cellules)")}<input class="num" type="number" min="1" max="512" step="1" bind:value={form.gridLimit} disabled={busy} /></label>
   </div>
   <div class="cols2">
    <label>{$t("Prompt P1 de référence")}<textarea bind:value={form.promptP1} disabled={busy} rows="3"></textarea></label>
    <label>{$t("Prompt P2 de référence")}<textarea bind:value={form.promptP2} disabled={busy} rows="3"></textarea></label>
    <label>{$t("Prompt QUALITY de référence")}<textarea bind:value={form.promptQUALITY} disabled={busy} rows="2"></textarea></label>
    <label>{$t("Prompt ACTION de référence")}<textarea bind:value={form.promptACTION} disabled={busy} rows="2"></textarea></label>
    <label>{$t("Prompt BG de référence")}<textarea bind:value={form.promptBG} disabled={busy} rows="2"></textarea></label>
    <label>{$t("Prompt NEG de référence")}<textarea bind:value={form.promptNEG} disabled={busy} rows="2"></textarea></label>
    <label>{$t("LoRA de référence (une par ligne, « nom:poids » ; le poids s’applique au modèle et au CLIP)")}<textarea bind:value={form.loras} disabled={busy} rows="2" placeholder={$t("detail_tweaker:0.6")}></textarea></label>
    <label>{$t("Notes")}<textarea bind:value={form.notes} disabled={busy} rows="2"></textarea></label>
   </div>
   <div class="row end"><button class="primary" data-create-campaign disabled={busy || !form.name.trim() || !form.workflow} onclick={createCampaign}>{$t("Créer la campagne")}</button></div>
  </div>
 </div>
{:else}
 {#if current.cells.length}
 <section class="run" data-ab-run>
  <div class="wb-bar"><h2 title={current.name}>{current.name}</h2><span class="chip num">{$tn("{n} cellule", "{n} cellules", current.cells.length)}</span><span class="grow"></span>
   {#if plan}<button class="primary" disabled={busy || !chosen.length} onclick={prepareCells} data-ab-prepare-cells>{$tn("Préparer {n} brouillon", "Préparer {n} brouillons", chosen.length)}</button>
   <button disabled={busy} onclick={syncResults} data-ab-sync>{$t("Rattacher les résultats")}</button>{/if}
  </div>
  <div class="plan">
   {#if plan}
    {#each plan.problems as problem}<p class="error">{problem}</p>{/each}
    <p class="note num" data-ab-plan-summary title={$t("Verdicts strictement manuels ; aucune attribution automatique.")}>{$t("{ready} sur {total} cellule(s) préparable(s) · {chosen} sélectionnée(s) → {chosen} job(s) brouillon(s). Rien n’est envoyé au GPU sans confirmation individuelle.", { ready: plan.cells.filter(c => !c.problems.length).length, total: plan.cells.length, chosen: chosen.length })}</p>
   {:else}<p class="note">{$t("Calcul du plan…")}</p>{/if}
  </div>
  {#if abDrafts.length}<details class="sec drafts" open><summary><span class="sec-title">{$t("Préparer les générations à comparer")}</span><span class="chip num">{abDrafts.length}</span></summary><div class="sec-body">{#each abDrafts as d (d.prepared.jobId)}<div data-ab-draft={d.cellId}><h4>{$t("Cellule {n}", { n: d.position + 1 })}</h4><PreparedDraft prepared={d.prepared} onsubmitted={() => loadPlan()} /></div>{/each}</div></details>{/if}
 </section>
 {:else}
 <div class="wb-bar"><h2 title={current.name}>{current.name}</h2></div>
 {/if}
 {@render messages()}
 <div class="wb-scroll canvas">
 {#if !current.cells.length}
  <div class="empty">
   <p>{$tn("Le produit cartésien des variantes, borné par la limite explicite : {n} variante déclarée.", "Le produit cartésien des variantes, borné par la limite explicite : {n} variantes déclarées.", current.parameters.filter(p => p.kind === 'variant').length)}</p>
   <button class="primary" data-prepare-grid disabled={busy || !current.parameters.some(p => p.kind === 'variant')} onclick={prepareGrid}>{$t("Préparer la grille (sans exécution)")}</button>
  </div>
 {:else}
  <div class="ab-grid" role="table" aria-label={$t("Grille comparative des cellules")}>
   {#each current.cells as cell (cell.id)}
    {@const draft = drafts[cell.id]}
    {#if draft}
    <article class="ab-cell" data-ab-cell={cell.id} class:favorite={draft.favorite} class:best={draft.best}>
     <div class="media">
      {#if cell.images.length}
       {#each cell.images as image (image.assetId)}<figure><Thumbnail id={image.assetId} alt={$t("Résultat référencé de la cellule {n}", { n: cell.position + 1 })} /><figcaption class="hash">job {image.jobId.slice(0, 8)} · node {image.nodeId ?? '—'} · #{image.outputIndex ?? '—'}</figcaption></figure>{/each}
      {:else}<p class="placeholder">{$t("Aucune image référencée.")}</p>{/if}
     </div>
     <header><span class="cname">{$t("Cellule {n}", { n: cell.position + 1 })}</span>
      {#if draft.best}<span class="ab-badge win">{$t("gagnante")}</span>{/if}
      {#if draft.favorite}<span class="ab-badge">{$t("favorite")}</span>{/if}
      {#if draft.rank}<span class="ab-badge num">n°{draft.rank}</span>{/if}
     </header>
     <dl class="ab-variants">
      {#each current.parameters.filter(p => p.kind === 'variant') as parameter (parameter.id)}
       <div><dt>{parameter.label}</dt><dd>{variantValue(cell, parameter.slot)}</dd></div>
      {/each}
     </dl>
     <div class="verdict">
      <label class="check"><input type="checkbox" bind:checked={draft.favorite} disabled={busy} /> {$t("Favorite")}</label>
      <label class="check"><input type="checkbox" bind:checked={draft.best} disabled={busy} /> {$t("Gagnante (une seule)")}</label>
      <label class="rank">{$t("Rang manuel")}<input class="num" type="number" min="1" step="1" bind:value={draft.rank} disabled={busy} /></label>
     </div>
     <div class="verdict-actions"><button class="small" disabled={busy} onclick={() => saveVerdict(cell.id)}>{$t("Enregistrer le verdict")}</button></div>
     {#if cell.verdict?.best}<div class="win-row"><input bind:value={presetName} disabled={busy} maxlength="300" aria-label={$t("Nom du preset pour la configuration gagnante")} placeholder={$t("Portrait extérieur, CFG 7")} /><button class="small" disabled={busy || !presetName.trim()} onclick={() => savePreset(cell.id)}>{$t("Mémoriser cette configuration")}</button></div>{/if}
     {#if planCell(cell.id)}{@const pc = planCell(cell.id)!}<div class="ab-plan-cell" data-ab-plan-cell={cell.id}>
      {#if pc.problems.length}<p class="error">{pc.problems.join(' ; ')}</p>
      {:else}<label class="check"><input type="checkbox" checked={chosen.includes(cell.id)} disabled={busy} onchange={() => toggleCell(cell.id)} data-ab-choose={cell.position} />{$t("Préparer ({count} entrée(s) modifiée(s) : {names})", { count: pc.edits.length, names: pc.edits.map(e => e.inputName).join(', ') || $t("aucune") })}</label>{/if}
      {#each pc.jobs as job}<p class="note">{$t("Job {id}…", { id: job.id.slice(0, 8) })} · {$t(stateLabel(job.state))} <button class="small quiet" disabled={busy} onclick={() => recipeFromJob(job.id, cell.position)}>{$t("Créer une recette")}</button></p>{/each}
     </div>{/if}
     <details class="inline"><summary>{$t("Configuration exacte et verdict")}</summary>
      <label>{$t("Note")}<textarea bind:value={draft.note} disabled={busy} rows="2"></textarea></label>
      <pre>{JSON.stringify(cell.config, null, 2)}</pre>
     </details>
    </article>
    {/if}
   {/each}
  </div>

  <section class="diff-sec">
   <div class="row diff-row"><h3>{$t("Diff exacte entre deux cellules")}</h3>
    <label class="inline-label">{$t("Cellule A")}<select bind:value={diffA} disabled={busy}>{#each current.cells as cell (cell.id)}<option value={cell.id}>{$t("Cellule {n}", { n: cell.position + 1 })}</option>{/each}</select></label>
    <label class="inline-label">{$t("Cellule B")}<select bind:value={diffB} disabled={busy}>{#each current.cells as cell (cell.id)}<option value={cell.id}>{$t("Cellule {n}", { n: cell.position + 1 })}</option>{/each}</select></label>
    <button data-run-diff disabled={busy || diffA === diffB} onclick={runDiff}>{$t("Comparer champ par champ")}</button>
   </div>
   {#if diff}
    <table class="ab-diff">
     <thead><tr><th>{$t("Champ")}</th><th class="ab-a">A</th><th class="ab-b">B</th><th>{$t("Diffère")}</th></tr></thead>
     <tbody>
      {#each diff.entries as entry (entry.key)}
       <tr class:differs={entry.differs}><th scope="row">{$t(LABELS[entry.key] ?? entry.key)}</th><td class="ab-a">{formatValue(entry.a)}</td><td class="ab-b">{formatValue(entry.b)}</td><td>{entry.differs ? $t("oui") : $t("non")}</td></tr>
      {/each}
     </tbody>
    </table>
    <p class="notice" class:ab-refused={!diff.attribution.allowed} data-attribution>{diff.attribution.reason}</p>
   {/if}
  </section>
 {/if}
 </div>
{/if}
</section>
</div>

{#snippet messages()}
 {#if busy || error || note}<div class="msgs">
  {#if busy}<p class="notice" role="status" aria-live="polite">{$t("Traitement en cours… Les commandes seront disponibles à la fin.")}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if note}<p class="notice" role="status">{note}</p>{/if}
 </div>{/if}
{/snippet}

<style>
  .ab { display: grid; grid-template-columns: 320px minmax(0, 1fr); height: 100%; min-height: 520px; border: 1px solid var(--line); border-radius: var(--radius-lg); overflow: hidden; }
  .ab-main { display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg); }
  h2 { font-size: 13px; font-weight: 600; margin: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  h3 { font-size: 12px; font-weight: 600; margin: 0; }
  h4 { font-size: 11px; font-weight: 600; color: var(--muted); margin: 0 0 4px; text-transform: uppercase; letter-spacing: .04em; }
  label { display: flex; flex-direction: column; gap: 4px; color: var(--muted); font-size: 12px; min-width: 0; }
  .check { flex-direction: row; align-items: center; gap: 6px; color: var(--text); }
  .inline-label { flex-direction: row; align-items: center; gap: 6px; white-space: nowrap; }
  .inline-label select { min-width: 120px; max-width: 220px; }
  .note { color: var(--muted); margin: 0; overflow-wrap: anywhere; font-size: 12px; }
  .note.pad { padding: 12px; }
  .notice { color: var(--muted); background: none; border: 0; padding: 0; margin: 0; overflow-wrap: anywhere; white-space: normal; font-size: 12px; }
  .error { color: var(--danger); margin: 0; overflow-wrap: anywhere; }
  .msgs { display: grid; gap: 4px; padding: 8px 12px; border-bottom: 1px solid var(--line); }
  .grow { flex: 1; }
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .row.end { justify-content: flex-end; }
  .hash { overflow-wrap: anywhere; font-size: 11px; color: var(--faint); font-variant-numeric: tabular-nums; }
  details { margin: 0; }
  details.inline > summary { color: var(--muted); font-size: 12px; cursor: pointer; }
  .small { min-height: 24px; font-size: 11.5px; padding: 0 8px; }
  .ab-campaign { display: grid; gap: 2px; width: 100%; text-align: start; padding: 7px 12px; min-height: 0; border: 0; border-bottom: 1px solid var(--line); border-radius: 0; background: transparent; }
  .ab-campaign:hover:not(:disabled) { background: var(--hover); }
  .ab-campaign.on { background: var(--raised); box-shadow: inset 2px 0 0 var(--accent); }
  .ab-campaign strong { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ab-campaign small { color: var(--muted); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ab-params { list-style: none; margin: 0; padding: 0; }
  .ab-params li { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 6px; padding: 5px 0; border-bottom: 1px solid var(--line); overflow-wrap: anywhere; }
  .ab-params li strong { font-weight: 600; }
  .ab-params li .note { flex-basis: 100%; font-size: 11.5px; }
  .form-grid { display: grid; gap: 8px; padding-top: 8px; }
  .form-grid button { justify-self: start; }
  .create { display: grid; gap: 12px; padding: 16px; max-width: 980px; }
  .cols2 { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 300px), 1fr)); gap: 12px; }
  .cols5 { display: grid; grid-template-columns: 2fr repeat(4, minmax(0, 1fr)); gap: 12px; align-items: end; }
  .run { flex: none; }
  .plan { padding: 6px 12px; border-bottom: 1px solid var(--line); display: grid; gap: 4px; }
  .drafts { max-height: 40vh; overflow: auto; }
  .canvas { background: var(--canvas); }
  .empty { display: grid; justify-items: center; gap: 12px; padding: 64px 16px; color: var(--muted); text-align: center; }
  .empty p { margin: 0; max-width: 520px; }
  .ab-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 16px; padding: 16px; }
  .ab-cell { display: grid; gap: 6px; align-content: start; min-width: 0; padding: 0 0 8px; border-radius: var(--radius-lg); }
  .media { display: grid; gap: 4px; }
  .media figure { margin: 0; }
  .media figure :global(img) { width: 100%; aspect-ratio: 1 / 1; object-fit: contain; display: block; background: var(--bg); border-radius: var(--r-img); }
  .ab-cell.favorite .media :global(img) { outline: 1px solid var(--line-strong); }
  .ab-cell.best .media :global(img) { outline: 2px solid var(--accent); outline-offset: 1px; }
  .media figcaption { font-size: 10px; color: var(--faint); margin-top: 2px; }
  .placeholder { margin: 0; aspect-ratio: 1 / 1; display: grid; place-items: center; background: var(--bg); border: 1px dashed var(--line-strong); border-radius: var(--r-img); color: var(--faint); font-size: 12px; }
  .ab-cell header { display: flex; align-items: center; gap: 6px; }
  .cname { font-weight: 600; flex: 1; }
  .ab-badge { font-size: 10.5px; color: var(--muted); border: 1px solid var(--line-strong); border-radius: 3px; padding: 0 5px; line-height: 16px; }
  .ab-badge.win { color: var(--accent-text); background: var(--accent); border-color: var(--accent); }
  .ab-variants { display: flex; flex-wrap: wrap; gap: 2px 12px; margin: 0; font-size: 12px; }
  .ab-variants dt { display: inline; color: var(--muted); }
  .ab-variants dt::after { content: ' '; }
  .ab-variants dd { display: inline; margin: 0; font-variant-numeric: tabular-nums; }
  .verdict { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 12px; }
  .rank { flex-direction: row; align-items: center; gap: 6px; }
  .rank input { width: 64px; min-height: 24px; }
  .verdict-actions { display: flex; flex-wrap: wrap; gap: 4px; }
  .win-row { display: flex; gap: 4px; }
  .win-row input { flex: 1; min-width: 0; min-height: 24px; }
  .ab-plan-cell { display: grid; gap: 4px; padding-top: 4px; border-top: 1px solid var(--line); font-size: 12px; }
  pre { font-size: 11px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--radius); padding: 6px 8px; margin: 4px 0 0; max-height: 220px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff-sec { padding: 12px 16px 24px; border-top: 1px solid var(--line); display: grid; gap: 8px; }
  .diff-row h3 { margin-inline-end: 8px; }
  .ab-diff { border-collapse: collapse; width: 100%; font-size: 12px; }
  .ab-diff th, .ab-diff td { border-bottom: 1px solid var(--line); padding: 5px 8px; text-align: start; overflow-wrap: anywhere; vertical-align: top; }
  .ab-diff thead th { font-size: 10.5px; text-transform: uppercase; letter-spacing: .04em; color: var(--faint); font-weight: 600; }
  .ab-diff tbody th { color: var(--muted); font-weight: 500; }
  .ab-diff tr.differs td { background: var(--raised); font-weight: 600; }
  .ab-a { border-inline-start: 2px solid var(--sel); }
  .ab-b { border-inline-start: 2px solid var(--accent); }
  .ab-refused { color: var(--danger); }
  @media (max-width: 980px) { .ab { grid-template-columns: minmax(0, 1fr); height: auto; } .cols5 { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
