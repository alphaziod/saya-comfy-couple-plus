<script lang="ts">
  import { t } from '../lib/i18n';
  import SamplerComparators from './SamplerComparators.svelte';
  // Shared last step of every preparation path (recipes, scenes, A/B):
  // shows what the frozen draft changes against the stored workflow, then
  // submits only after an explicit GPU confirmation (and an explicit
  // acceptance when prompt terms are removed). Nothing is sent otherwise.
  import { call, message, native, type PreparedJob } from '../lib/api';
  let { prepared, onsubmitted = () => {} }: { prepared: PreparedJob; onsubmitted?: (jobId: string) => unknown } = $props();
  let confirmed = $state(false), allowRemovals = $state(false), busy = $state(false), error = $state(''), submitted = $state('');
  let recipeName = $state(''), recipeSaved = $state('');
  let forJob = '';
  $effect(() => { if (prepared.jobId !== forJob) { forJob = prepared.jobId; confirmed = false; allowRemovals = false; error = ''; submitted = ''; recipeSaved = ''; recipeName = ''; } });
  // The frozen draft becomes a recipe exactly (prompts, seeds, characters,
  // variants, models): the same capture path as any recipe.
  async function saveRecipe() {
    if (!recipeName.trim() || busy) return;
    busy = true; error = '';
    try { const created = await call<{ id: string }>('create_recipe', { job: prepared.jobId, name: recipeName, description: 'Capturée depuis un brouillon préparé' }); recipeSaved = created.id; }
    catch (e) { error = message(e); } finally { busy = false; }
  }
  async function submit() {
    if (!confirmed || (prepared.removed.length && !allowRemovals) || submitted) return;
    if (busy) return; busy = true; error = '';
    try {
      await call('submit', { job: prepared.jobId, hash: prepared.snapshotHash, confirmed: true, allow_removals: allowRemovals });
      submitted = prepared.jobId; confirmed = false;
      await onsubmitted(prepared.jobId);
    } catch (e) { error = message(e); } finally { busy = false; }
  }
</script>
<section class="prepared-draft" data-prepared-draft={prepared.jobId}>
  <SamplerComparators job={prepared.jobId} />
  <h3>{$t("Brouillon {id}",{id:prepared.jobId})}</h3>
  <p>{$t("{n} nodes · empreinte {hash}… ·",{n:prepared.nodeCount,hash:prepared.snapshotHash.slice(0, 16)})} <strong data-draft-change-count={prepared.changes.length}>{$t("{n} entrée(s) modifiée(s)",{n:prepared.changes.length})}</strong> {$t("par rapport au workflow enregistré ; tout le reste du graphe est identique.")}</p>
  {#if prepared.changes.length}
    <table class="module-table" data-draft-changes><thead><tr><th>{$t("Entrée")}</th><th>{$t("Workflow enregistré")}</th><th>{$t("Graphe préparé")}</th></tr></thead>
      <tbody>{#each prepared.changes as c}<tr><td>{c.node}.{c.input}</td><td><pre class="prompt-block">{c.before}</pre></td><td><pre class="prompt-block">{c.after}</pre></td></tr>{/each}</tbody></table>
  {/if}
  {#if prepared.removed.length}
    <p class="error" data-draft-removed>{$t("Termes retirés des prompts : {terms}",{terms:prepared.removed.join(' · ')})}</p>
    <label class="checkbox"><input type="checkbox" bind:checked={allowRemovals} disabled={!!submitted} data-draft-allow-removals />{$t("J’accepte la suppression de ces termes")}</label>
  {/if}
  <details><summary>{$t("Requête API exacte")}</summary><pre>{prepared.requestJson}</pre></details>
  <div class="button-row"><label>{$t("Enregistrer ce brouillon comme recette")}<input maxlength="300" bind:value={recipeName} placeholder={$t("nom de la recette")} data-draft-recipe-name /></label><button disabled={busy || !native || !recipeName.trim()} onclick={saveRecipe} data-draft-save-recipe>{$t("Enregistrer comme recette")}</button></div>
  {#if recipeSaved}<p class="notice" data-draft-recipe-saved={recipeSaved}>{$t("Recette enregistrée (capture exacte du brouillon).")}</p>{/if}
  {#if submitted}
    <p class="notice" data-draft-submitted={submitted}>{$t("Job {id} transmis à ComfyUI. Suivi dans Workflows.",{id:submitted})}</p>
  {:else}
    <label class="checkbox"><input type="checkbox" bind:checked={confirmed} data-draft-confirm />{$t("Je confirme l’envoi de ce job à ComfyUI (lance une génération sur le GPU)")}</label>
    <button class="danger" disabled={!native || busy || !confirmed || (prepared.removed.length > 0 && !allowRemovals)} onclick={submit}>{$t("Soumettre à ComfyUI")}</button>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
