<script lang="ts">
  import { t } from '../lib/i18n';
  import { stateLabel } from '../lib/state-label';
  import WorkflowSplit from './WorkflowSplit.svelte';
  import MemoryPanel from './MemoryPanel.svelte';
  import { onMount } from 'svelte';
  import { call, message } from '../lib/api';
  import type { JobProvenance } from '../lib/provenance';
  import Thumbnail from './Thumbnail.svelte';
  import SamplerComparators from './SamplerComparators.svelte';
  import { buildVisualManifest, loadImageNodes, move, newDraftConfig, newDraftStep, pruneBindings, pruneConfigLinks, removeStep, validateVisualDraft, type ConfigLink, type DraftConfig, type DraftStep, type WorkflowNode } from '../lib/pipeline-builder';
  import PipelineEditor from './PipelineEditor.svelte';
  import Icon from './Icon.svelte';

  interface Wf { id: string; name: string; promptProfile: string }
  interface Pipe { id: string; name: string; manifestSha256: string; createdAt: string }
  interface RunItem { run: string; state: string; createdAt: string; updatedAt: string }
  interface RunStep { step: string; position: number; state: string; job: string | null; outputKind: string | null; outputSha256: string | null; outputBytes: number | null; message: string; updatedAt: string }
  interface RunEvent { step: string | null; kind: string; detail: unknown; at: string }
  interface RunDetail { run: string; pipeline: string; state: string; createdAt: string; steps: RunStep[]; events: RunEvent[] }

  let workflows = $state<Wf[]>([]), pipes = $state<Pipe[]>([]), error = $state(''), info = $state('');
  let { embedded = false }: { embedded?: boolean } = $props();
  let name = $state(''), steps = $state<DraftStep[]>([newDraftStep([])]);
  let configs = $state<DraftConfig[]>([{ id: 'config1', label: 'Config globale', fields: [] }]);
  let links = $state<ConfigLink[]>([]);
  let nodes = $state<Record<string, WorkflowNode[]>>({});
  let selected = $state<string | null>(null), runs = $state<RunItem[]>([]);
  let runId = $state<string | null>(null), detail = $state<RunDetail | null>(null), compareJob = $state(''), compareLabel = $state(''), reviewSeed = $state(''), reviewImage = $state(''), reviewChanges = $state<any[]>([]), autoRunning = $state(false);
  let confirmed = $state(false), busy = $state(false), waiting = $state(false), runMsg = $state(''), cancelConfirmed = $state(false), unloadAfterPass = $state(false);
  const wfName = (id:string) => workflows.find(w=>w.id===id)?.name ?? 'workflow non choisi';
  const counts = $derived(detail ? Object.fromEntries(['completed', 'pending', 'submitted', 'failed', 'invalidated'].map(k => [k, detail!.steps.filter(s => s.state === k).length])) : null);
  let thumbs = $state<Record<string, string>>({});

  const problems = $derived(validateVisualDraft(name, steps, configs, links));
  const manifest = $derived(JSON.stringify(buildVisualManifest(name, steps, configs, links, '<uuid généré à l’enregistrement>'), null, 2));
  const pending = $derived(detail?.steps.find(s => s.state !== 'completed') ?? null);
  const totalSteps = $derived(detail?.steps.length ?? 0);
  const completedSteps = $derived(detail?.steps.filter(s => s.state === 'completed').length ?? 0);
  const globalPct = $derived(totalSteps ? Math.round(completedSteps * 100 / totalSteps) : 0);

  async function loadNodes(wf: string) {
    if (!wf || nodes[wf]) return;
    try { nodes[wf] = loadImageNodes(await call<WorkflowNode[]>('workflow_nodes', { id: wf })); } catch (e) { error = message(e); }
  }
  async function refreshPipes() { pipes = await call<Pipe[]>('pipelines'); }
  onMount(async () => { try { workflows = await call<Wf[]>('workflows'); await refreshPipes(); } catch (e) { error = message(e); } });

  function addStep() { steps = [...steps, newDraftStep(steps)]; }
  function drop(i: number) { steps = removeStep(steps, i); links = pruneConfigLinks(configs, steps, links); }
  function shift(i: number, d: number) { steps = pruneBindings(move(steps, i, d)); }
  function setWorkflow(i: number, wf: string) { steps[i].workflowId = wf; steps[i].nodeId = ''; loadNodes(wf); }

  async function save() {
    error = ''; info = '';
    if (problems.length) return;
    try {
      const id = await call<string>('save_pipeline', { text: JSON.stringify(buildVisualManifest(name, steps, configs, links)) });
      info = $t("Pipeline enregistré ({id}).", { id });
      await refreshPipes(); await choose(String(id));
    } catch (e) { error = message(e); }
  }
  async function choose(id: string) {
    if (busy) return;
    selected = id; runId = null; detail = null; confirmed = false; waiting = false; unloadAfterPass = false; runMsg = ''; error = '';
    try { runs = await call<RunItem[]>('pipeline_runs', { pipeline: id }); } catch (e) { error = message(e); }
  }
  async function newRun(reuseModels = false) {
    if (!selected || busy) return;
    try { const r = await call<string>('start_pipeline_run', { pipeline: selected, reuseModels }); runs = await call<RunItem[]>('pipeline_runs', { pipeline: selected }); await openRun(String(r)); } catch (e) { error = message(e); }
  }
  async function openRun(r: string) { if (busy) return; detail = null; runId = r; runMsg = ''; waiting = false; confirmed = false; unloadAfterPass = false; await refreshRun(); }
  async function refreshRun() {
    if (!runId) return;
    try {
      detail = await call<RunDetail>('pipeline_run', { run: runId });
      const first = detail.steps.find(s => s.state === 'completed' && s.job);
      if (first?.job) {
        try {
          const prov = await call<any>('job_provenance', { id: first.job });
          reviewSeed = String(prov.seed ?? '');
          reviewImage = prov.outputs?.[0]?.asset ?? '';
          const raw = prov.requestJson ?? '';
          try { reviewChanges = JSON.parse(raw).changes ?? []; } catch { reviewChanges = []; }
        } catch { reviewSeed = ''; reviewImage = ''; reviewChanges = []; }
      }
      if (selected) runs = await call<RunItem[]>('pipeline_runs', { pipeline: selected });
      for (const s of detail.steps) if (s.state === 'completed' && s.job && !(s.job in thumbs)) {
        thumbs[s.job] = '';
        call<JobProvenance>('job_provenance', { id: s.job }).then(p => { thumbs[s.job as string] = p.outputs[0]?.asset ?? ''; }).catch(() => {});
      }
    } catch (e) { error = message(e); }
  }
  async function cancelRun() {
    if (!runId || !cancelConfirmed || busy) return;
    busy = true; error = ''; runMsg = '';
    try {
      const r = await call<{ stopped?: boolean; engineCalled?: boolean; step?: string; action?: string; engine?: { confirmed: boolean; message: string } }>('cancel_pipeline_run', { run: runId, confirmed: true });
      waiting = false;
      runMsg = r.stopped ? $t("Exécution arrêtée : aucune passe active, moteur non sollicité.")
        : r.engine?.confirmed ? $t("Passe {step} annulée ({action} confirmé par le moteur). Une nouvelle exécution exigera votre confirmation.", { step: r.step ?? '', action: r.action === 'interrupt' ? $t("interruption") : $t("retrait de la file") })
        : $t("Annulation demandée pour la passe {step} : {message}. La passe reste suivie jusqu'à son état réel.", { step: r.step ?? '', message: r.engine?.message ?? $t("en attente de confirmation du moteur") });
    } catch (e) { error = message(e); }
    finally { cancelConfirmed = false; busy = false; await refreshRun(); }
  }
  async function continueAll() {
    if (!runId || autoRunning) return;
    autoRunning = true; error='';
    try {
      for (let i = 0; i < 32; i++) {
        const current = await call<RunDetail>('pipeline_run', { run: runId });
        if (current.state === 'completed') break;
        const nextStep = current.steps.find(s => s.state !== 'completed');
        if (!nextStep) break;
        await call('run_next_pass', { run: runId, confirmed: true, unload_after_pass: unloadAfterPass });
        await refreshRun();
      }
    } catch (e) { error = message(e); } finally { autoRunning = false; await refreshRun(); }
  }
  async function stopRun() {
    if (!runId) return;
    try { await call('cancel_pipeline_run', { run: runId, confirmed: true }); }
    catch (e) { error = message(e); } finally { await refreshRun(); }
  }
  async function next() {
    if (!runId || !confirmed || busy) return;
    busy = true; error = ''; runMsg = '';
    try {
      const r = await call<{ done?: boolean; waiting?: boolean; state?: string; step?: string }>('run_next_pass', { run: runId, confirmed: true, unload_after_pass: unloadAfterPass });
      waiting = !!r.waiting;
      runMsg = r.done && !r.step ? $t("Toutes les passes sont terminées.") : r.waiting ? $t("Passe {step} : en attente du moteur.", { step: r.step ?? '' }) : $t("Passe {step} : {state}.", { step: r.step ?? '', state: $t(stateLabel(r.state)) });
    } catch (e) { error = message(e); }
    finally { confirmed = false; busy = false; await refreshRun(); }
  }

</script>

{#snippet pipeList()}
  <ul class="pl-list">
    {#each pipes as p (p.id)}
      <li><button class="pl-item" class:on={selected === p.id} aria-current={selected === p.id ? 'true' : undefined} data-pipeline-item={p.id} disabled={busy} onclick={() => choose(p.id)}><span>{p.name}</span><small class="num">{p.createdAt}</small></button></li>
    {:else}<li class="muted pl-empty">{$t("Aucun pipeline enregistré.")}</li>{/each}
  </ul>
{/snippet}

{#snippet runPanel()}
  {#if selected}
    <details class="sec" open>
      <summary><span class="sec-title">{$t("Exécutions")}</span><span class="chip num">{runs.length}</span></summary>
      <div class="sec-body">
        <button onclick={() => newRun(false)} disabled={busy}>{$t("Nouvelle exécution")}</button>
        {#if runs.length}<div class="pl-runs">{#each runs as r (r.run)}<button class="pl-item" class:on={runId === r.run} data-pipeline-run={r.run} disabled={busy} onclick={() => openRun(r.run)}><span class="num">{r.run.slice(0, 8)}</span><small>{$t(stateLabel(r.state))}</small></button>{/each}</div>{/if}
      </div>
    </details>
  {/if}
  {#if detail}
    <details class="sec" open>
      <summary><span class="sec-title">{$t("Exécution {id}", { id: detail.run.slice(0, 8) })}</span><span class="chip">{$t(stateLabel(detail.state))}</span></summary>
      <div class="sec-body">
        <div class="pl-progress" data-pass-progress>
          <strong>{$t("Passe {step}", { step: pending?.step ?? completedSteps + '/' + totalSteps })} · {$t(stateLabel(pending?.state ?? detail.state))}</strong>
          {#if pending && ['submitted','running','queued'].includes(pending.state)}<progress aria-label={$t("Passe en cours")}></progress><span class="muted">{$t("Calcul en cours · pourcentage non fourni par le moteur")}</span>{/if}
          {#if pending?.message || pending?.state === 'pending'}<span class="muted">{pending?.message || (pending?.state==='pending'?$t("Prête à lancer après confirmation."):'')}</span>{/if}
        </div>
        <div class="pl-progress" data-pipeline-progress>
          <span class="pl-progress-row"><span>{$t("Pipeline global · {done}/{total} passes", { done: completedSteps, total: totalSteps })}</span><span class="num muted">{globalPct}%</span></span>
          <progress value={globalPct} max="100"></progress>
        </div>
        {#if detail.state !== 'completed' && detail.steps[0]?.state === 'completed'}
          <section class="pass-review" data-pass-review>
            <span class="sec-title">{$t("Passe 1 · revue")}</span>
            {#if reviewImage}<div class="pass-review-image"><Thumbnail id={reviewImage} /></div>{/if}
            <span class="muted">{$t("Master seed")} <code class="num" data-pass-seed>{reviewSeed || '—'}</code></span>
            {#if reviewChanges.length}<details class="pl-sub"><summary>{$t("Prompts et paramètres modifiés ({n})", { n: reviewChanges.length })}</summary><ul>{#each reviewChanges as c}<li><code>{c.node}.{c.input}</code> : {c.before} → {c.after}</li>{/each}</ul></details>{/if}
            <div class="pl-actions">
              <button data-pass-continue disabled={busy||autoRunning} onclick={continueAll}>{autoRunning?$t("Déroulement…"):$t("Continuer tout le pipeline")}</button>
              <button class="quiet" data-pass-restart disabled={busy||autoRunning} onclick={() => newRun(true)}>{$t("Restart (même modèles chargés)")}</button>
              <button class="danger" data-pass-stop disabled={busy||autoRunning} onclick={stopRun}>{$t("Stopper")}</button>
            </div>
          </section>
        {/if}
        {#if counts}<p class="pl-summary muted num" data-run-summary title={$t("Les passes terminées sont récupérables (état épinglé, vérifié à chaque appel) ; une passe en échec ou invalidée ne l'est pas et sera ré-exécutée seulement sur confirmation.")}>{$t("Terminées {completed} · en attente {pending} · soumises {submitted} · en échec {failed} · invalidées {invalidated}", { completed: counts.completed, pending: counts.pending, submitted: counts.submitted, failed: counts.failed, invalidated: counts.invalidated })}</p>{/if}
        <ol class="pl-steps">
          {#each detail.steps as s (s.step)}
            <li data-run-step={s.step}>
              <span class="num muted pl-pos">{s.position + 1}</span>
              <div class="pl-step-main">
                <span class="pl-step-head"><strong>{s.step}</strong><span class="chip">{$t(stateLabel(s.state))}</span></span>
                <small class="muted num" title="{$t("Job")} · {$t("SHA-256")} · {$t("Octets")}">{s.job ?? '—'} · {s.outputSha256?.slice(0, 12) ?? '—'} · {s.outputBytes ?? '—'}</small>
                {#if s.message}<small class="pl-msg">{s.message}</small>{/if}
              </div>
              {#if s.job && thumbs[s.job]}<div class="pl-thumb"><Thumbnail id={thumbs[s.job]} /></div>{/if}
              {#if s.job}<button class="quiet icon-btn" data-run-compare={s.job} title={$t("Comparer les étapes")} aria-label={$t("Comparer les étapes")} onclick={()=>{compareJob=s.job!;compareLabel=$t("Passe {step}", { step: s.step });}}><Icon name="compare" /></button>{/if}
            </li>
          {/each}
        </ol>
        {#if detail.state === 'completed'}
          <p class="notice">{$t("Toutes les passes sont terminées.")}</p>
        {:else}
          <label class="checkbox"><input type="checkbox" data-run-confirm bind:checked={confirmed} />{$t("Je confirme l'exécution de la prochaine passe sur ComfyUI (génération GPU)")}</label>
          <label class="checkbox"><input type="checkbox" data-run-unload bind:checked={unloadAfterPass} />{$t("Décharger les modèles après cette passe (mesure avant/après, sans garantie de baisse)")}</label>
          <button class="primary" data-run-next disabled={!confirmed || busy} onclick={next} title={$t("« Exécuter la passe suivante » lance une passe. Après la première, « Continuer tout le pipeline » demande l’enchaînement des suivantes.")}>{waiting ? $t("Reprendre l'attente") : $t("Exécuter la passe suivante")}</button>
          {#if pending}<span class="muted">{$t("Prochaine passe : {step}.", { step: pending.step })}</span>{/if}
          {#if waiting}<p class="notice">{$t("En attente du moteur. Une passe déjà soumise n'est jamais renvoyée : « Reprendre l'attente » ne fait que reprendre le suivi (garanti par le noyau).")}</p>{/if}
          <details class="pl-sub">
            <summary>{$t("Arrêter / annuler")}</summary>
            <label class="checkbox"><input type="checkbox" data-run-cancel-confirm bind:checked={cancelConfirmed} />{$t("Je confirme l'arrêt de l'exécution (annule la passe en cours si elle est encore chez ComfyUI)")}</label>
            <button class="danger" data-run-cancel disabled={!cancelConfirmed || busy} onclick={cancelRun}>{$t("Arrêter / annuler")}</button>
          </details>
        {/if}
        {#if runMsg}<p class="notice" role="status">{runMsg}</p>{/if}
        <div class="pl-mem">{#key detail.run}<MemoryPanel run={detail.run} />{/key}</div>
      </div>
    </details>
    <details class="sec">
      <summary><span class="sec-title">{$t("Historique")}</span><span class="chip num">{detail.events.length}</span></summary>
      <div class="sec-body"><ul data-run-events class="pl-events">{#each detail.events as ev}<li><span class="num">{ev.at}</span> · {ev.step ?? 'pipeline'} · <strong>{ev.kind}</strong> {JSON.stringify(ev.detail)}</li>{/each}</ul></div>
    </details>
  {/if}
{/snippet}

{#snippet nameField()}
  <label class="pl-name"><span class="pl-sr">{$t("Nom")}</span><input bind:value={name} placeholder={$t("Nom du pipeline")} /></label>
{/snippet}

{#snippet saveButton()}
  <button class="primary" data-pipeline-save disabled={problems.length > 0} onclick={save}>{$t("Enregistrer le pipeline")}</button>
{/snippet}

{#snippet builderSide()}
  <details class="sec" data-pipeline-form>
    <summary><span class="sec-title">{$t("Formulaire simple (passes)")}</span><span class="chip num">{steps.length}</span></summary>
    <div class="sec-body">
      <div class="pipeline-graph" data-pipeline-graph>{#each steps as s,i (i)}<div class="pipeline-node" data-pipeline-node={i}><strong>{$t("Passe {n}", { n: i+1 })}</strong><span>{s.id || $t("sans id")}</span><small>{wfName(s.workflowId)}</small>{#if s.sourceStep}<small>← {s.sourceStep}</small>{/if}</div>{/each}</div>
      {#each steps as s, i (i)}
        <div class="pl-stepform" data-pipeline-step={i}>
          <span class="sec-title">{$t("Passe {n}", { n: i + 1 })}</span>
          <label>{$t("Identifiant")}<input bind:value={s.id} /></label>
          <label>{$t("Workflow")}<select value={s.workflowId} onchange={e => setWorkflow(i, e.currentTarget.value)}><option value="">{$t("— choisir —")}</option>{#each workflows as w}<option value={w.id}>{w.name}</option>{/each}</select></label>
          {#if i > 0}
            <label>{$t("Étape source de l'image")}<select bind:value={s.sourceStep} onchange={() => { s.nodeId = ''; }}><option value="">{$t("Aucune liaison")}</option>{#each steps.slice(0, i) as p}<option value={p.id}>{p.id}</option>{/each}</select></label>
            {#if s.sourceStep && s.workflowId}
              {#if (nodes[s.workflowId] ?? []).length}
                <label>{$t("Node LoadImage cible")}<select bind:value={s.nodeId}><option value="">{$t("— choisir —")}</option>{#each nodes[s.workflowId] as n}<option value={n.id}>{n.id}{n.title ? ` · ${n.title}` : ''}</option>{/each}</select></label>
              {:else if nodes[s.workflowId]}<p class="pl-warn">{$t("Ce workflow n'a pas de LoadImage : il ne peut pas recevoir l'image amont.")}</p>
              {:else}<p class="muted">{$t("Chargement des nodes…")}</p>{/if}
            {/if}
          {/if}
          <div class="pl-actions"><button class="quiet" onclick={() => shift(i, -1)} disabled={i === 0}>{$t("Monter")}</button><button class="quiet" onclick={() => shift(i, 1)} disabled={i === steps.length - 1}>{$t("Descendre")}</button><button class="quiet" onclick={() => drop(i)}>{$t("Retirer")}</button></div>
        </div>
      {/each}
      <button onclick={addStep}>{$t("Ajouter une passe")}</button>
    </div>
  </details>
  <details class="sec">
    <summary><span class="sec-title">{$t("Aperçu du manifeste JSON")}</span></summary>
    <div class="sec-body"><pre class="pl-pre" data-pipeline-manifest>{manifest}</pre></div>
  </details>
  {@render runPanel()}
{/snippet}

<section class="pl" class:embedded data-pipelines>
  {#if embedded}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if info}<p class="notice" role="status">{info}</p>{/if}
    <span class="sec-title">{$t("Pipelines enregistrés")}</span>
    {@render pipeList()}
    {@render runPanel()}
    {#if compareJob}<SamplerComparators job={compareJob} context={compareLabel} onclose={()=>compareJob=''} />{/if}
    <div data-pipeline-builder hidden><PipelineEditor {workflows} bind:steps bind:configs bind:links {nodes} loadWorkflow={loadNodes} {problems} /></div>
  {:else}
    <aside class="wb-panel left pl-left">
      <div class="wb-head"><h2 title={$t("Enchaînez des workflows importés en passes successives. Reprise logique du pipeline : pas une reprise exacte d'un sampler ; un état corrompu ou des entrées modifiées invalident la passe, qui est alors relancée. Passes IMAGE uniquement (LATENT non pris en charge) ; liaison uniquement vers un node LoadImage (entrée image) ; chaque passe doit contenir un SaveImage.")}>{$t("Pipelines enregistrés")}</h2><span class="chip num">{pipes.length}</span></div>
      <div class="wb-scroll">
        {@render pipeList()}
        <WorkflowSplit {workflows} oncreated={async id=>{workflows=await call('workflows');await refreshPipes();await choose(id)}} />
      </div>
    </aside>
    <div class="pl-main" data-pipeline-builder>
      {#if error || info}
        <div class="pl-msgs">
          {#if error}<p class="error" role="alert">{error}</p>{/if}
          {#if info}<p class="notice" role="status">{info}</p>{/if}
        </div>
      {/if}
      <div class="pl-editor">
        <PipelineEditor {workflows} bind:steps bind:configs bind:links {nodes} loadWorkflow={loadNodes} {problems} lead={nameField} trail={saveButton} side={builderSide} />
      </div>
    </div>
    {#if compareJob}<div class="pl-compare"><SamplerComparators job={compareJob} context={compareLabel} onclose={()=>compareJob=''} /></div>{/if}
  {/if}
</section>

<style>
  .pl{position:relative;display:grid;grid-template-columns:260px minmax(0,1fr);height:100%;min-height:0;background:var(--bg)}
  .pl.embedded{display:grid;grid-template-columns:minmax(0,1fr);gap:8px;height:auto;background:transparent}
  .pl-left .wb-head h2{font-size:13px}
  .pl-list{list-style:none;margin:0;padding:4px 0;border-bottom:1px solid var(--line)}
  .pl-item{display:flex;align-items:center;justify-content:space-between;gap:8px;width:100%;min-height:30px;padding:0 12px;border:0;border-radius:0;background:transparent;text-align:start;color:var(--text)}
  .pl-item span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .pl-item small{color:var(--muted);font-size:11px;white-space:nowrap;flex:none}
  .pl-item:hover:not(:disabled){background:var(--raised)}
  .pl-item.on{background:var(--hover);box-shadow:inset 2px 0 0 var(--accent)}
  .pl-empty{padding:8px 12px}
  .pl-runs{display:grid;border:1px solid var(--line);border-radius:var(--radius);overflow:hidden}
  .pl-runs .pl-item{min-height:26px;padding:0 8px;border-bottom:1px solid var(--line)}
  .pl-runs .pl-item:last-child{border-bottom:0}
  .pl-main{position:relative;display:flex;flex-direction:column;min-width:0;min-height:0}
  .pl-editor{flex:1;min-height:0;display:grid}
  .pl-msgs{flex:none;border-bottom:1px solid var(--line);background:var(--bg)}
  .pl-msgs p{margin:0;padding:4px 12px;font-size:12px;white-space:pre-wrap;overflow-wrap:anywhere}
  .pl-name{display:flex;flex-direction:row;margin:0;flex:0 1 200px;min-width:0}
  .pl-name input{width:100%;min-width:100px}
  .pl-sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
  .pl-progress{display:grid;gap:4px;font-size:12px}
  .pl-progress-row{display:flex;justify-content:space-between;gap:8px}
  .pl-progress progress{width:100%;height:4px;accent-color:var(--accent)}
  .pass-review{display:grid;gap:6px;padding:8px 0;border-block:1px solid var(--line);font-size:12px}
  .pass-review-image{border-radius:var(--r-img);overflow:hidden;background:var(--canvas)}
  .pass-review-image :global(img){display:block;width:100%;max-height:240px;object-fit:contain}
  .pl-actions{display:flex;flex-wrap:wrap;gap:6px}
  .pl-actions button{min-height:26px;font-size:12px}
  .pl-summary{margin:0;font-size:11.5px}
  .pl-steps{list-style:none;margin:0;padding:0;border-top:1px solid var(--line)}
  .pl-steps li{display:flex;align-items:center;gap:8px;padding:6px 0;border-bottom:1px solid var(--line);min-width:0}
  .pl-pos{width:14px;flex:none;font-size:11px}
  .pl-step-main{display:grid;gap:1px;flex:1;min-width:0}
  .pl-step-head{display:flex;align-items:center;gap:6px;min-width:0}
  .pl-step-head strong{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .pl-step-main small{font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .pl-msg{color:var(--text)}
  .pl-thumb{width:40px;height:40px;flex:none;border-radius:var(--r-img);overflow:hidden;background:var(--canvas)}
  .pl-thumb :global(img){width:40px;height:40px;object-fit:contain;display:block}
  .pl-sub{margin:0;font-size:12px}
  .pl-sub>summary{color:var(--muted);cursor:pointer;min-height:24px;line-height:24px}
  .pl-sub[open]{display:grid;gap:6px}
  .pl-sub ul{margin:0;padding-inline-start:16px;overflow-wrap:anywhere}
  .pl :global(.checkbox){flex-direction:row;align-items:flex-start;gap:6px;font-size:12px;color:var(--text)}
  .pl .notice{margin:0;font-size:12px}
  .pl-mem :global(details){margin:0;font-size:12px}
  .pl-mem :global(summary){color:var(--muted)}
  .pl-events{margin:0;padding:0;list-style:none;max-height:300px;overflow:auto;font-size:11px;color:var(--muted);overflow-wrap:anywhere}
  .pl-events li{padding:3px 0;border-bottom:1px solid var(--line)}
  .pl-pre{margin:0;max-height:320px;overflow:auto;font-size:11px;white-space:pre-wrap;overflow-wrap:anywhere}
  .pipeline-graph{display:flex;flex-wrap:wrap;gap:4px}
  .pipeline-node{display:grid;gap:1px;padding:4px 8px;border:1px solid var(--line-strong);border-radius:var(--radius);font-size:11.5px;min-width:0;max-width:100%}
  .pipeline-node small{color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .pl-stepform{display:grid;gap:6px;padding-top:8px;border-top:1px solid var(--line)}
  .pl-stepform label{gap:3px;color:var(--muted);font-size:12px}
  .pl-stepform input,.pl-stepform select{color:var(--text);width:100%}
  .pl-warn{margin:0;color:var(--warning);font-size:12px}
  .pl-compare{position:absolute;z-index:30;inset-block:0;inset-inline:260px 320px;overflow:auto;padding:12px 16px;background:var(--bg);border-inline:1px solid var(--line)}
  @media(max-width:1100px){.pl{grid-template-columns:220px minmax(0,1fr)}.pl-compare{inset-inline:220px 280px}}
</style>
