<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message, native, type Asset } from '../lib/api';
  import Thumbnail from './Thumbnail.svelte';
  let { asset, onclose }: { asset: Asset; onclose: () => void } = $props();
  interface Match { id: string; path: string; score: number; structure: number; color: number; gradient: number; spatial?: number }
  interface Status { running: boolean; cancel_requested: boolean; completed: number; total: number; error: string | null; pending: number }
  interface Report { vectorized: number; failed: number; cancelled: boolean; remaining: number; errors: string[] }
  interface SemanticStatus { available:boolean; model:string; version:number; dim:number; indexed:number; pending:number; python:string; checkpoint:string; script:string }
  interface SemanticMatch { id:string; path:string; score:number }
  interface SemanticReport { embedded:number; failed:number; remaining:number }
  let matches = $state<Match[]>([]), compared = $state(0), loading = $state(true), error = $state('');
  let status = $state<Status>({ running: false, cancel_requested: false, completed: 0, total: 0, error: null, pending: 0 });
  let starting = $state(false);
  let semanticStatus=$state<SemanticStatus|null>(null), semanticMatches=$state<SemanticMatch[]>([]), semanticBusy=$state(false), semanticError=$state(''), semanticDiagnostic=$state<Record<string, unknown>|null>(null);
  function modal(node: HTMLDialogElement) { node.showModal(); }
  const percent = (value: number) => `${Math.round(value * 1000) / 10} %`;
  const name = (path: string) => path.split(/[\\/]/).pop() ?? path;
  async function refreshSemantic() {
    try { semanticStatus = await call<SemanticStatus>('semantic_status', {}); } catch (e) { semanticError = message(e); }
  }
  async function diagnoseSemantic() {
    try { semanticDiagnostic = await call<Record<string, unknown>>('semantic_diagnose', {}); }
    catch (e) { semanticError = message(e); }
  }

  async function indexSemantic() {
    if (semanticBusy) return;
    semanticBusy = true; semanticError = '';
    try {
      const report = await call<SemanticReport>('semantic_index', { budget: 16 });
      await refreshSemantic();
      if (report.failed) semanticError = $tn("{n} image sans embedding sémantique.", "{n} images sans embedding sémantique.", report.failed);
    } catch (e) { semanticError = message(e); } finally { semanticBusy = false; }
  }
  async function semanticSearch() {
    if (semanticBusy) return;
    semanticBusy = true; semanticError = '';
    try {
      const result = await call<{matches:SemanticMatch[]}>('semantic_search', { asset: asset.id, limit: 24 });
      semanticMatches = result.matches;
    } catch (e) { semanticError = message(e); } finally { semanticBusy = false; }
  }
  async function search() {
    loading = true; error = '';
    try {
      const result = await call<{ matches: Match[]; compared: number }>('visual_search', { asset: asset.id });
      matches = result.matches; compared = result.compared;
    } catch (e) { error = message(e); } finally { loading = false; }
  }
  async function refresh() {
    try { status = await call<Status>('visual_status', {}); } catch (e) { error = message(e); }
  }
  async function startIndexing() {
    if (starting || status.running) return;
    starting = true; error = '';
    try {
      const report: Report = await call('visual_index', {});
      status.pending = report.remaining;
      if (report.failed) error = $tn("Analyse visuelle : {n} fichier en échec, les autres restent utilisables.", "Analyse visuelle : {n} fichiers en échec, les autres restent utilisables.", report.failed);
      await search();
    } catch (e) { error = message(e); } finally { starting = false; await refresh(); }
  }
  function cancel() { call('visual_cancel', {}).catch(e => (error = message(e))); }
  onMount(() => {
    search(); refresh(); refreshSemantic();
    let polling = false;
    const timer = setInterval(async () => {
      if ((!status.running && !starting) || polling) return; polling = true;
      try { await refresh(); } finally { polling = false; }
    }, 750);
    return () => clearInterval(timer);
  });
</script>
<dialog class="image-dialog visual-dialog" data-visual-search use:modal onclose={onclose} aria-label={$t("Images visuellement similaires")}>
  <div class="dialog-toolbar"><strong>{$t("Images similaires · {name}",{name:name(asset.path)})}</strong><button class="quiet" onclick={onclose}>{$t("Fermer")}</button></div>
  <div class="visual-results">
    <p class="muted caption" data-visual-caption>{$t("Similarité visuelle approximative calculée localement (structure, couleurs, contours) — jamais une identification de personnage.")}{#if compared} {$tn("{n} image comparée par force brute.","{n} images comparées par force brute.",compared)}{/if}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if status.running}<div class="progress-row"><span role="status" class="num">{$t("Analyse visuelle : {done} / {total}",{done:status.completed,total:status.total || $t("découverte…")})}</span><progress max={status.total || 1} value={status.completed}></progress>
      <button class="quiet" disabled={status.cancel_requested} onclick={cancel}>{status.cancel_requested ? $t("Arrêt après le fichier courant…") : $t("Interrompre")}</button></div>
    {/if}
    {#if loading}<p class="muted" role="status">{$t("Recherche en cours…")}</p>{:else if !matches.length}<div class="empty-state">{$t("Aucune autre image avec vecteur d'analyse.")}{status.pending ? ' ' + $t("Lancez l'analyse des images restantes ci-dessous.") : ''}</div>{/if}
    <div class="image-grid" data-visual-matches>
      {#each matches as match (match.id)}
        <div class="image-card visual-card" data-visual-match={match.path} title={`${$t("structure")} ${percent(match.structure)} · ${$t("couleurs")} ${percent(match.color)} · ${$t("contours")} ${percent(match.gradient)}${match.spatial != null ? ` · ${$t("espace")} ${percent(match.spatial)}` : ''}`}>
          <Thumbnail id={match.id} alt={match.path} />
          <span>{name(match.path)}</span>
          <small class="num">{percent(match.score)} · {$t("structure")} {percent(match.structure)} · {$t("couleurs")} {percent(match.color)} · {$t("contours")} {percent(match.gradient)}{match.spatial != null ? ` · ${$t("espace")} ${percent(match.spatial)}` : ''}</small>
        </div>
      {/each}
    </div>
    {#if !status.running && status.pending > 0}
      <div class="visual-index"><span class="muted">{$tn("{n} image sans analyse visuelle.","{n} images sans analyse visuelle.",status.pending)}</span><button class="primary" disabled={starting || !native} onclick={startIndexing}>{starting ? $t("Analyse en cours…") : $t("Analyser les images restantes")}</button></div>
    {:else if !status.running && !loading && matches.length}
      <p class="muted">{$t("Toutes les images présentes ont un vecteur d'analyse.")}</p>
    {/if}
    <section data-semantic-search>
      <h3 title={$t("Modèle local déjà présent, aucun téléchargement ni GPU. Index séparé et versionné ; la recherche perceptuelle ci-dessus reste indépendante.")}>{$t("Recherche sémantique CLIP (CPU, local)")}</h3>
      {#if semanticError}<p class="error" role="alert">{semanticError}</p>{/if}
      {#if semanticStatus}
        {#if !semanticStatus.available}
          <p class="muted">{$t("Indisponible : venv, checkpoint ou script local absent. La recherche perceptuelle reste utilisable.")}</p>
        {:else}
          <p class="muted num" data-semantic-status>{$t("{model} v{version} · {dim} dims · {indexed} indexée(s) · {pending} en attente",{model:semanticStatus.model,version:semanticStatus.version,dim:semanticStatus.dim,indexed:semanticStatus.indexed,pending:semanticStatus.pending})}</p>
          <div class="row">
            <button data-semantic-index disabled={semanticBusy || semanticStatus.pending === 0} onclick={indexSemantic}>{semanticBusy ? $t("Traitement CPU…") : $t("Indexer 16 images (CPU)")}</button>
            <button data-semantic-find disabled={semanticBusy} onclick={semanticSearch}>{$t("Rechercher sémantiquement")}</button>
          </div>
          {#if semanticMatches.length}<div class="image-grid" data-semantic-matches>{#each semanticMatches as match (match.id)}<div class="image-card"><Thumbnail id={match.id} alt={match.path} /><span>{name(match.path)}</span><small class="num">{percent(match.score)}</small></div>{/each}</div>{/if}
        {/if}
      {/if}
    </section>
  </div>
</dialog>
<style>
  .visual-dialog{background:var(--canvas);border:1px solid var(--line-strong);border-radius:6px;color:var(--text)}
  .visual-dialog::backdrop{background:#000c}
  .dialog-toolbar{display:flex;align-items:center;justify-content:space-between;gap:12px;height:44px;padding:0 12px;border-bottom:1px solid var(--line);background:var(--bg)}
  .dialog-toolbar strong{font-size:13px;font-weight:600}
  .visual-results{padding:14px 16px;height:calc(100% - 44px);overflow:auto;display:grid;gap:12px;align-content:start}
  p{margin:0}
  .caption{font-size:12px}
  .progress-row{display:flex;align-items:center;gap:10px}
  .progress-row progress{flex:1;max-width:320px;height:4px;accent-color:var(--accent)}
  .image-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:10px;margin:0}
  .image-card{display:flex;flex-direction:column;background:transparent;border:0;padding:0;min-width:0;cursor:default}
  .image-card :global(img),.image-card :global(.image-placeholder){width:100%;aspect-ratio:4/5;height:auto;max-height:none;object-fit:contain;background:var(--canvas);border-radius:var(--r-img)}
  .image-card span{padding:4px 2px 0;font-size:11.5px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .image-card small{padding:0 2px;font-size:10.5px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .visual-index{display:flex;align-items:center;gap:12px;flex-wrap:wrap}
  section{display:grid;gap:8px;border-top:1px solid var(--line);padding-top:12px}
  h3{margin:0;font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted)}
  .row{display:flex;gap:6px;flex-wrap:wrap}
  .empty-state{padding:30px 10px;text-align:center;color:var(--muted)}
</style>
