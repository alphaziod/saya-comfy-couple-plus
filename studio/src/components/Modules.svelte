<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message, native } from '../lib/api';
  import {
    parseModuleList, parseModuleDetail, parseModuleDiff, composeModules, buildConnection, instanceId,
    type ModuleSummary, type ModuleDetail, type ModuleVersionDiff, type InputChoice,
  } from '../lib/module-contracts';
  interface WorkflowItem { id: string; name: string; promptProfile: string }
  interface Source { node: string; outputIndex: number; classType: string; kind: string; title: string | null }
  interface Target { node: string; input: string; classType: string; kind: string; linked: boolean; title: string | null }
  interface Candidates {
    inputs: { port: string; kind: string; sources: Source[] }[];
    outputs: { port: string; kind: string; targets: Target[] }[];
    missingModuleClasses: string[]; unknownWorkflowNodes: string[];
  }
  interface InsertPreview {
    instance: string; workflow: { id: string; name: string; nodeCount: number };
    addedNodes: string[]; changedInputs: { node: string; input: string; before: unknown; after: unknown }[];
    warnings: string[]; graph: Record<string, unknown>;
  }
  let list = $state<ModuleSummary[]>([]), workflows = $state<WorkflowItem[]>([]);
  let detail = $state<ModuleDetail | null>(null), diff = $state<ModuleVersionDiff | null>(null);
  let compareA = $state<number | null>(null), compareB = $state<number | null>(null);
  let busy = $state(false), error = $state(''), note = $state('');
  // Composition with another module (type contract only)
  let partnerId = $state(''), partner = $state<ModuleDetail | null>(null);
  let composition = $derived(detail && partner ? [
    { from: detail.manifest.name, to: partner.manifest.name, result: composeModules(detail.manifest, partner.manifest) },
    { from: partner.manifest.name, to: detail.manifest.name, result: composeModules(partner.manifest, detail.manifest) },
  ] : null);
  // Reuse inside a stored workflow
  let targetWorkflow = $state(''), candidates = $state<Candidates | null>(null);
  let inputChoice = $state<Record<string, string>>({}), outputChoice = $state<Record<string, string>>({});
  let instance = $state(''), preview = $state<InsertPreview | null>(null), copyName = $state('');
  const label = (n: { node: string; classType?: string | null; title?: string | null }) => `${n.node} · ${n.title || n.classType || '?'}`;
  const workflowName = (id: string | null) => workflows.find(w => w.id === id)?.name ?? null;

  async function refresh() {
    try {
      list = parseModuleList(await call('modules'));
      workflows = await call<WorkflowItem[]>('workflows');
    } catch (e) { error = message(e); }
  }
  onMount(() => { if (native) refresh(); });
  async function choose(id: string, version: number | null = null) {
    error = ''; diff = null; partner = null; partnerId = ''; candidates = null; preview = null; targetWorkflow = '';
    try {
      detail = parseModuleDetail(await call('module', version === null ? { id } : { id, version }));
      instance = instanceId(detail.manifest.name);
    } catch (e) { error = message(e); }
  }
  // Each selector load carries a ticket: a slower stale answer never
  // overwrites the list for the workflow currently chosen.
  let partnerTicket = 0, targetTicket = 0;
  async function compare() {
    if (!detail || compareA === null || compareB === null || compareA === compareB) return;
    error = '';
    try { diff = parseModuleDiff(await call('compare_module_versions', { id: detail.id, a: compareA, b: compareB })); }
    catch (e) { error = message(e); }
  }
  async function rollback(version: number) {
    if (!detail) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      await call('rollback_module', { id: detail.id, version });
      await refresh();
      await choose(detail.id);
      note = $t("Référence courante ramenée à la version {version} ; toutes les versions restent intactes.", { version });
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function loadPartner() {
    const ticket = ++partnerTicket, id = partnerId;
    partner = null;
    if (!id) return;
    try { const value = parseModuleDetail(await call('module', { id })); if (ticket === partnerTicket) partner = value; }
    catch (e) { if (ticket === partnerTicket) error = message(e); }
  }
  async function loadCandidates() {
    const ticket = ++targetTicket;
    candidates = null; preview = null; inputChoice = {}; outputChoice = {};
    if (!detail || !targetWorkflow) return;
    if (busy) return; busy = true; error = '';
    try {
      const result = await call<Candidates>('module_candidates', { id: detail.id, version: detail.version, workflow: targetWorkflow });
      if (ticket !== targetTicket) return;
      // Preselect only when exactly one typed source exists: never guess between several.
      const choice: Record<string, string> = {};
      for (const port of result.inputs) if (port.sources.length === 1) choice[port.port] = `${port.sources[0].node}:${port.sources[0].outputIndex}`;
      inputChoice = choice; candidates = result;
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  let built = $derived.by(() => {
    if (!candidates) return null;
    const inputs: Partial<Record<string, InputChoice>> = {};
    for (const port of candidates.inputs) {
      const value = inputChoice[port.port];
      if (value) { const [node, index] = value.split(':'); inputs[port.port] = { port: port.port, node, outputIndex: Number(index) }; }
    }
    const outputs = candidates.outputs.flatMap(port => {
      const value = outputChoice[port.port];
      if (!value) return [];
      const [node, input] = value.split(':');
      return [{ port: port.port, node, input }];
    });
    return buildConnection(candidates.inputs.map(p => p.port), inputs, outputs);
  });
  async function previewInsert() {
    if (!detail || !built || built.missing.length || !candidates) return;
    if (busy) return; busy = true; error = ''; note = ''; preview = null;
    try {
      preview = await call<InsertPreview>('module_insert_preview', { id: detail.id, version: detail.version, workflow: targetWorkflow, instance, connection: built.connection });
      copyName = `${preview.workflow.name} + ${detail.manifest.name}`;
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function saveCopy() {
    if (!preview || !copyName.trim()) return;
    if (busy) return; busy = true; error = ''; note = '';
    try {
      await call('import_workflow', { name: copyName, text: JSON.stringify(preview.graph) });
      note = $t("Nouveau workflow « {name} » enregistré. « {original} » reste inchangé.", { name: copyName, original: preview.workflow.name });
      workflows = await call<WorkflowItem[]>('workflows');
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  function openGraphEditor() {
    const tab = document.querySelector<HTMLElement>('[data-nav="graph"]') ?? [...document.querySelectorAll<HTMLElement>('nav button')].find(n => n.textContent?.trim() === $t("Éditeur graphique"));
    tab?.click();
  }
</script>

<div class="wb md">
  <aside class="wb-panel left" data-module-library>
    <div class="wb-head"><h2 title={$t("Conserver un groupe de nodes et le réutiliser dans un autre workflow.")}>{$t("Bibliothèque")}</h2><span class="chip num">{list.length}</span></div>
    <div class="wb-scroll">
      <ul class="md-list">
        {#each list as item (item.id)}
          <li><button class="md-item" class:on={detail?.id === item.id} aria-current={detail?.id === item.id ? 'true' : undefined} data-module-item={item.id} disabled={busy} onclick={() => { note = ''; choose(item.id); }}>
            <span>{item.name}</span><small class="num">v{item.currentVersion} · {$tn("{n} version", "{n} versions", item.versionCount)} · {$tn("{n} entrée", "{n} entrées", item.inputs.length)} → {$tn("{n} sortie", "{n} sorties", item.outputs.length)}</small>
          </button></li>
        {:else}<li class="muted md-empty" title={$t("Aucun module. Importez un workflow API dans Workflows, puis extrayez-en une partie ci-dessous.")}>{$t("Aucun module.")}</li>{/each}
      </ul>
      <details class="sec" data-module-extract>
        <summary><span class="sec-title">{$t("Extraire un module")}</span></summary>
        <div class="sec-body">
          <p class="muted md-p" title="{$t("L'extraction se fait désormais")} {$t("dans l'éditeur graphique")} {$t(": ouvrez un workflow, sélectionnez visuellement les nodes (rectangle ou Maj+clic), puis bouton « Créer un module ». Les entrées/sorties sont déduites des liens traversants — plus de liste de cases à cocher ici.")}">{$t("L'extraction se fait désormais")} {$t("dans l'éditeur graphique")}.</p>
          <button disabled={!native} onclick={openGraphEditor}>{$t("Ouvrir l'éditeur graphique")}</button>
        </div>
      </details>
    </div>
  </aside>

  <div class="wb-center md-center">
    {#if busy || error || note}
      <div class="md-msgs">
        {#if busy}<p class="muted" role="status" aria-live="polite">{$t("Traitement en cours… Les commandes seront disponibles à la fin.")}</p>{/if}
        {#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="notice" role="status">{note}</p>{/if}
      </div>
    {/if}
    {#if !detail}
      <p class="md-empty-state muted">{$t("Choisissez un module pour voir ses ports, sa provenance et le réutiliser.")}</p>
    {:else}
      <section class="md-detail" data-module-detail>
        <header class="wb-bar md-head">
          <h2>{$t("{name} · version {version}", { name: detail.manifest.name, version: detail.version })} {detail.isCurrent ? $t("(courante)") : $t("(archive)")}</h2>
          <span class="grow"></span>
          <span class="chip num">v{detail.version}/{detail.versions.length}</span>
        </header>
        <div class="md-scroll">
          {#if detail.manifest.description}<p class="md-desc">{detail.manifest.description}</p>{/if}
          <p data-module-engine={detail.engineChecked ? (detail.warnings.length ? 'incompatible' : 'ok') : 'unchecked'} class="md-engine {detail.engineChecked && !detail.warnings.length ? 'ok' : 'error'}">
            {#if !detail.engineChecked}{$t("ComfyUI hors ligne : dépendances non vérifiées.")}{:else if detail.warnings.length}{$t("Incompatible avec le moteur actuel :")}{:else}{$t("Dépendances présentes dans le moteur actuel.")}{/if}
          </p>
          {#each detail.warnings as warning}<p class="error md-p">{warning}</p>{/each}

          <div class="md-cols">
            <div>
              <h3 class="sec-title">{$t("Entrées")}</h3>
              <table class="module-table" data-module-inputs><thead><tr><th>{$t("Port")}</th><th>{$t("Type")}</th><th>{$t("Node.entrée")}</th></tr></thead>
                <tbody>{#each detail.manifest.inputs as p}<tr><td>{p.name}</td><td>{p.kind}</td><td class="num">{p.node}.{p.input}</td></tr>{:else}<tr><td colspan="3" class="muted">{$t("aucune")}</td></tr>{/each}</tbody></table>
            </div>
            <div>
              <h3 class="sec-title">{$t("Sorties")}</h3>
              <table class="module-table" data-module-outputs><thead><tr><th>{$t("Port")}</th><th>{$t("Type")}</th><th>{$t("Node[sortie]")}</th></tr></thead>
                <tbody>{#each detail.manifest.outputs as p}<tr><td>{p.name}</td><td>{p.kind}</td><td class="num">{p.node}[{p.outputIndex}]</td></tr>{:else}<tr><td colspan="3" class="muted">{$t("aucune")}</td></tr>{/each}</tbody></table>
            </div>
          </div>

          <h3 class="sec-title">{$t("Paramètres exposés")}</h3>
          <p class="md-p num">{detail.manifest.parameters.map(p => `${p.node}.${p.input} = ${p.value}`).join(' · ') || $t("aucun")}</p>

          <div class="md-cols">
            <div>
              <h3 class="sec-title">{$t("Dépendances")}</h3>
              <p class="md-p" data-module-dependencies>{$t("Nodes : {list}", { list: detail.manifest.dependencies.classes.join(', ') || $t("aucune") })}<br />{$t("Checkpoints : {list}", { list: detail.manifest.dependencies.checkpoints.join(', ') || $t("aucun") })}<br />{$t("LoRA : {list}", { list: detail.manifest.dependencies.loras.join(', ') || $t("aucune") })}</p>
            </div>
            <div>
              <h3 class="sec-title">{$t("Provenance")}</h3>
              <p class="md-p" data-module-provenance>{$t("Workflow source : {name}", { name: workflowName(detail.sourceWorkflowId) ?? (detail.sourceWorkflowId ? $t("{id} (introuvable)", { id: detail.sourceWorkflowId }) : $t("inconnu")) })}<br />{$t("Empreinte source :")} <span class="hash">{detail.sourceSha256}</span><br />{$t("Version créée le {date} · {count} nodes copiés à l’identique", { date: detail.createdAt, count: detail.nodeCount })}</p>
            </div>
          </div>

          <h3 class="sec-title">{$t("Versions")}</h3>
          <ul class="md-versions">{#each detail.versions as v}<li data-module-version={v.version} class:on={v.version === detail.version}><span class="num">v{v.version} · {v.createdAt}</span><span class="grow"></span>
            <button class="quiet" disabled={busy || v.version === detail.version} onclick={() => choose(detail!.id, v.version)}>{$t("Inspecter")}</button>
            {#if v.version !== detail.version && detail.isCurrent}<button class="quiet" disabled={busy} onclick={() => rollback(v.version)}>{$t("Rendre courante")}</button>{/if}</li>{/each}</ul>
          {#if detail.versions.length > 1}
            <div class="md-compare">
              <label class="md-inline">{$t("Version A")}<select bind:value={compareA}>{#each detail.versions as v}<option value={v.version}>v{v.version}</option>{/each}</select></label>
              <label class="md-inline">{$t("Version B")}<select bind:value={compareB}>{#each detail.versions as v}<option value={v.version}>v{v.version}</option>{/each}</select></label>
              <button disabled={busy || compareA === null || compareB === null || compareA === compareB} onclick={compare}>{$t("Comparer")}</button>
            </div>
          {/if}
          {#if diff}
            <section class="md-diff" data-module-diff>
              <p class="md-p num"><strong>{$t("Nodes :")}</strong> +{diff.nodes.added.length} / −{diff.nodes.removed.length} / {$t("modifiés")} {diff.nodes.changed.length} · <strong>{$t("Paramètres :")}</strong> +{diff.parameters.added.length} / −{diff.parameters.removed.length} / {$t("modifiés")} {diff.parameters.changed.length}</p>
              <details open><summary>{$t("Diff exacte")}</summary><pre>{JSON.stringify(diff, null, 2)}</pre></details>
            </section>
          {/if}
        </div>
      </section>
    {/if}
  </div>

  <aside class="wb-panel right md-right">
    {#if detail}
      <div class="wb-scroll">
        <details class="sec" open>
          <summary title={$t("Contrat de types uniquement. L’enchaînement exécuté de plusieurs passes n’existe pas encore dans Saya Studio.")}><span class="sec-title">{$t("Composition avec un autre module")}</span></summary>
          <div class="sec-body">
            <label>{$t("Module partenaire")}<select bind:value={partnerId} onchange={e => { partnerId = e.currentTarget.value; loadPartner(); }} data-module-partner><option value="">{$t("Choisir…")}</option>{#each list.filter(m => m.id !== detail!.id) as m}<option value={m.id}>{m.name}</option>{/each}</select></label>
            {#if composition}
              <div class="md-composition" data-module-composition>
                {#each composition as c}
                  <p data-composable={c.result.composable}><strong>{c.from} → {c.to} :</strong> {#if c.result.composable}{c.result.links.map(l => `${l.from} → ${l.to} (${l.kind})`).join(' · ')}{#if c.result.unsatisfied.length} · {$t("entrées restant à fournir : {list}", { list: c.result.unsatisfied.map(p => `${p.name} (${p.kind})`).join(', ') })}{/if}{:else}<span class="error">{$t("incompatible : aucune sortie ne fournit un type attendu")}</span>{/if}</p>
                {/each}
              </div>
            {/if}
          </div>
        </details>
        <details class="sec" open>
          <summary><span class="sec-title">{$t("Réutiliser dans un workflow")}</span></summary>
          <div class="sec-body">
            <label>{$t("Workflow de destination")}<select bind:value={targetWorkflow} onchange={e => { targetWorkflow = e.currentTarget.value; loadCandidates(); }} data-module-target><option value="">{$t("Choisir…")}</option>{#each workflows as w}<option value={w.id}>{w.name}</option>{/each}</select></label>
            {#if candidates}
              <div class="md-candidates" data-module-candidates>
                {#if candidates.missingModuleClasses.length}<p class="error">{$t("Classes du module absentes du moteur : {list}", { list: candidates.missingModuleClasses.join(', ') })}</p>{/if}
                {#if candidates.unknownWorkflowNodes.length}<p class="muted">{$t("Nodes du workflow inconnus du moteur (ignorés) : {list}", { list: candidates.unknownWorkflowNodes.join(', ') })}</p>{/if}
                {#each candidates.inputs as port}
                  <label>{$t("Entrée {port} ({kind}) — source requise", { port: port.port, kind: port.kind })}
                    <select bind:value={inputChoice[port.port]} data-module-input={port.port}>
                      <option value="">{port.sources.length ? $t("Choisir…") : $t("Aucune sortie compatible dans ce workflow")}</option>
                      {#each port.sources as s}<option value={`${s.node}:${s.outputIndex}`}>{label(s)} [{s.outputIndex}] → {s.kind}</option>{/each}
                    </select></label>
                {/each}
                {#each candidates.outputs as port}
                  <label>{$t("Sortie {port} ({kind}) — destination facultative", { port: port.port, kind: port.kind })}
                    <select bind:value={outputChoice[port.port]} data-module-output={port.port}>
                      <option value="">{$t("Ne pas brancher")}</option>
                      {#each port.targets as tgt}<option value={`${tgt.node}:${tgt.input}`} disabled={tgt.linked}>{label(tgt)}.{tgt.input}{tgt.linked ? ' — ' + $t("déjà alimentée, non remplaçable") : ''}</option>{/each}
                    </select></label>
                {/each}
                <label>{$t("Identifiant d’instance")}<input maxlength="64" bind:value={instance} data-module-instance /></label>
                {#if built?.missing.length}<p class="muted">{$t("À relier : {list}", { list: built.missing.join(', ') })}</p>{/if}
                <button class="primary" disabled={busy || !built || built.missing.length > 0 || !instance} onclick={previewInsert}>{$t("Prévisualiser l’insertion")}</button>
              </div>
            {/if}
            {#if preview}
              <section class="md-preview" data-module-preview>
                <p><strong>{$tn("{n} node ajouté", "{n} nodes ajoutés", preview.addedNodes.length)}</strong> ({preview.addedNodes.join(', ')}) · <strong>{$tn("{n} entrée existante recâblée", "{n} entrées existantes recâblées", preview.changedInputs.length)}</strong>{#each preview.changedInputs as c}<br />{c.node}.{c.input} : {JSON.stringify(c.before)} → {JSON.stringify(c.after)}{/each}</p>
                {#each preview.warnings as warning}<p class="error">{warning}</p>{/each}
                <p class="muted">{$t("Graphe préparé, non soumis. « {name} » n’est pas modifié ; l’enregistrement crée un workflow distinct.", { name: preview.workflow.name })}</p>
                <label>{$t("Nom du nouveau workflow")}<input maxlength="300" bind:value={copyName} data-module-copy-name /></label>
                <button disabled={busy || !copyName.trim()} onclick={saveCopy}>{$t("Enregistrer comme nouveau workflow")}</button>
                <details><summary>{$t("Graphe préparé")}</summary><pre>{JSON.stringify(preview.graph, null, 2)}</pre></details>
              </section>
            {/if}
          </div>
        </details>
      </div>
    {:else}
      <p class="muted md-side-empty">{$t("Choisissez un module pour voir ses ports, sa provenance et le réutiliser.")}</p>
    {/if}
  </aside>
</div>

<style>
  .md{--wb-left:260px;--wb-right:320px}
  .md .wb-head h2{font-size:13px}
  .md-list{list-style:none;margin:0;padding:4px 0;border-bottom:1px solid var(--line)}
  .md-item{display:grid;gap:1px;width:100%;min-height:0;padding:6px 12px;border:0;border-radius:0;background:transparent;text-align:start;color:var(--text)}
  .md-item span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .md-item small{color:var(--muted);font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .md-item:hover:not(:disabled){background:var(--raised)}
  .md-item.on{background:var(--hover);box-shadow:inset 2px 0 0 var(--accent-warm)}
  .md-empty{padding:8px 12px}
  .md-p{margin:0;font-size:12px;overflow-wrap:anywhere}
  .md-center{position:relative}
  .md-msgs{flex:none;border-bottom:1px solid var(--line);background:var(--bg)}
  .md-msgs p{margin:0;padding:4px 16px;font-size:12px;overflow-wrap:anywhere}
  .md-empty-state{margin:auto;padding:24px;text-align:center}
  .md-detail{display:flex;flex-direction:column;min-height:0;flex:1}
  .md-head h2{font-size:13px;font-weight:600;margin:0;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .md-scroll{flex:1;min-height:0;overflow:auto;padding:12px 16px 24px;display:grid;gap:8px;align-content:start}
  .md-scroll h3{margin:10px 0 2px}
  .md-desc{margin:0;color:var(--muted)}
  .md-engine{margin:0;font-size:12px}
  .md-engine.ok{color:var(--ok)}
  .md-cols{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px 24px}
  .md-versions{list-style:none;margin:0;padding:0;border-top:1px solid var(--line)}
  .md-versions li{display:flex;align-items:center;gap:6px;min-height:30px;border-bottom:1px solid var(--line);font-size:12px}
  .md-versions li.on{color:var(--text);box-shadow:inset 2px 0 0 var(--accent-warm);padding-inline-start:8px}
  .md-versions button{min-height:24px;font-size:12px}
  .grow{flex:1}
  .md-compare{display:flex;align-items:end;gap:8px;flex-wrap:wrap}
  .md-inline{gap:3px;color:var(--muted);font-size:12px}
  .md-diff{display:grid;gap:6px}
  .md-diff summary,.md-preview summary{color:var(--muted);font-size:12px;cursor:pointer}
  pre{margin:4px 0 0;max-height:360px;overflow:auto;font-size:11px;white-space:pre-wrap;overflow-wrap:anywhere;background:var(--bg);border:1px solid var(--line);border-radius:var(--radius);padding:8px}
  .md-right .sec-body label{gap:3px;color:var(--muted);font-size:12px}
  .md-right select,.md-right input{width:100%;color:var(--text)}
  .md-composition,.md-candidates,.md-preview{display:grid;gap:8px}
  .md-composition p,.md-candidates p,.md-preview p{margin:0;font-size:12px;overflow-wrap:anywhere}
  .md-side-empty{padding:12px;margin:0;font-size:12px}
  @media(max-width:1100px){.md{--wb-left:220px;--wb-right:280px}.md-cols{grid-template-columns:1fr}}
</style>
