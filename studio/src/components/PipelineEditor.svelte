<script lang="ts">
  import { t, tn } from '../lib/i18n';
  // Visual pipeline editor (brief point 13): big blocks [ pass 1 ] →
  // [ pass 2 ] linked ComfyUI-style, plus GLOBAL CONFIG blocks (prompts
  // P1/P2, negative, seed, explicit node.input field for model/VAE/LoRA…)
  // linkable to the passes that need them. A single system:
  // - the logic lives in src/lib/pipeline-builder.ts (single v2 manifest,
  //   validations mirroring the core);
  // - rendering and gestures reuse the graph library of the workflow
  //   editor (src/lib/graph/view.ts);
  // - execution stays the existing orchestrator: each pass keeps its
  //   own API workflow, never a graph merge here.
  import {
    CONFIG_SECTIONS, configFieldType, newDraftConfig, newDraftStep, pruneConfigLinks, removeStep,
    type ConfigLink, type DraftConfig, type DraftStep, type WorkflowNode,
  } from '../lib/pipeline-builder';
  import { clampZoom, edgePath, fitTransform, roundPos, HEAD_H, NODE_W, ROW_H, typeColor } from '../lib/graph/view';
  import type { Snippet } from 'svelte';

  interface Wf { id: string; name: string }
  interface Port { name: string; label: string; type: string; index: number; x: number; y: number; linked: boolean }
  interface Block { key: string; kind: 'phase' | 'config'; refId: string; title: string; sub: string; x: number; y: number; w: number; h: number; inputs: Port[]; outputs: Port[] }
  interface Edge { id: string; kind: 'image' | 'config'; d: string; color: string }

  let {
    workflows, steps = $bindable(), configs = $bindable(), links = $bindable(),
    nodes, loadWorkflow, problems = [], lead, trail, side,
  }: {
    workflows: Wf[];
    steps: DraftStep[];
    configs: DraftConfig[];
    links: ConfigLink[];
    nodes: Record<string, WorkflowNode[]>;
    loadWorkflow: (wf: string) => void;
    problems?: string[];
    /** Page-level controls rendered at the start / end of the editor bar, and below the selection inspector. */
    lead?: Snippet; trail?: Snippet; side?: Snippet;
  } = $props();

  const CONFIG_W = 210;
  const keyOf = (kind: 'step' | 'config', id: string) => `${kind}:${id}`;

  let canvas = $state<HTMLDivElement | null>(null);
  let zoom = $state(1), panX = $state(0), panY = $state(0);
  let positions = $state<Record<string, { x: number; y: number }>>({});
  let offsets = $state<Record<string, { x: number; y: number }>>({});
  let selected = $state<string | null>(null), selectedEdge = $state<string | null>(null);
  let mode = $state<null | { kind: 'pan' | 'drag' | 'connect'; key?: string; sx: number; sy: number; moved: boolean }>(null);
  let link = $state<null | { block: string; port: string; color: string; x: number; y: number; ax: number; ay: number }>(null);
  let note = $state(''), newPhaseWorkflow = $state('');

  const blocks: Block[] = $derived.by(() => {
    const list: Block[] = [];
    configs.forEach((c, i) => {
      const pos = positions[keyOf('config', c.id)];
      const x = pos?.x ?? 40 + i * (CONFIG_W + 60), y = pos?.y ?? 60;
      const outputs: Port[] = c.fields.map((f, k) => ({
        name: f.key, label: f.key, type: configFieldType(f.key), index: k,
        x: x + CONFIG_W, y: y + HEAD_H + k * ROW_H + ROW_H / 2,
        linked: links.some(l => l.config === c.id && l.field === f.key),
      }));
      list.push({
        key: keyOf('config', c.id), kind: 'config', refId: c.id, title: c.label,
        sub: `${c.fields.length} champ(s)`, x, y, w: CONFIG_W,
        h: HEAD_H + Math.max(c.fields.length, 1) * ROW_H + 8, inputs: [], outputs,
      });
    });
    steps.forEach((s, i) => {
      const pos = positions[keyOf('step', s.id)];
      const x = pos?.x ?? 40 + (i % 4) * (NODE_W + 70);
      const y = pos?.y ?? 340 + Math.floor(i / 4) * 220;
      const inputs: Port[] = [];
      if (i > 0) inputs.push({ name: 'image', label: 'image', type: 'IMAGE', index: 0, x, y: y + HEAD_H + ROW_H / 2, linked: !!s.sourceStep });
      inputs.push({ name: 'config', label: 'config', type: '*', index: inputs.length, x, y: y + HEAD_H + inputs.length * ROW_H + ROW_H / 2, linked: links.some(l => l.step === s.id) });
      const outputs: Port[] = [{ name: 'image', label: 'image', type: 'IMAGE', index: 0, x: x + NODE_W, y: y + HEAD_H + ROW_H / 2, linked: steps.some(o => o.sourceStep === s.id) }];
      list.push({
        key: keyOf('step', s.id), kind: 'phase', refId: s.id, title: s.id,
        sub: workflows.find(w => w.id === s.workflowId)?.name ?? $t("workflow non choisi"),
        x, y, w: NODE_W, h: HEAD_H + Math.max(inputs.length, outputs.length, 1) * ROW_H + 8, inputs, outputs,
      });
    });
    return list;
  });

  const edges: Edge[] = $derived.by(() => {
    const list: Edge[] = [];
    const byKey = new Map(blocks.map(b => [b.key, b]));
    for (const s of steps) {
      if (!s.sourceStep) continue;
      const a = byKey.get(keyOf('step', s.sourceStep)), b = byKey.get(keyOf('step', s.id));
      if (!a || !b) continue;
      list.push({
        id: `img:${s.id}`, kind: 'image',
        d: edgePath({ x: a.x + a.w, y: a.y + HEAD_H + ROW_H / 2 }, { x: b.x, y: b.y + HEAD_H + ROW_H / 2 }),
        color: typeColor('IMAGE'),
      });
    }
    for (const l of links) {
      const a = byKey.get(keyOf('config', l.config)), b = byKey.get(keyOf('step', l.step));
      const out = a?.outputs.find(p => p.name === l.field), input = b?.inputs.find(p => p.name === 'config');
      if (!a || !b || !out || !input) continue;
      list.push({ id: `cfg:${l.config}:${l.step}:${l.field}`, kind: 'config', d: edgePath(out, input), color: typeColor(out.type) });
    }
    return list;
  });

  const single = $derived(selected ? blocks.find(b => b.key === selected) ?? null : null);
  const selectedStep = $derived(single?.kind === 'phase' ? steps.find(s => s.id === single.refId) ?? null : null);
  const selectedConfig = $derived(single?.kind === 'config' ? configs.find(c => c.id === single.refId) ?? null : null);
  const linkCount = $derived(links.length + steps.filter(s => s.sourceStep).length);

  function viewport() { const r = canvas?.getBoundingClientRect(); return { w: r?.width ?? 800, h: r?.height ?? 500 }; }
  function fit() {
    const { w, h } = viewport();
    // Only the boxes count for fitTransform (lib/graph/view.ts);
    // the rest of the Block (ports, labels) is not a SceneNode.
    const boxes = { nodes: blocks.map(b => ({ x: b.x, y: b.y, w: b.w, h: b.h })) } as Parameters<typeof fitTransform>[0];
    const t = fitTransform(boxes, w, h);
    zoom = t.zoom; panX = t.panX; panY = t.panY;
  }
  // Refits the scene on each block add/remove (never during a
  // drag or a pan): new blocks must stay visible.
  let fittedFor = -1;
  $effect(() => {
    const count = blocks.length;
    if (count > 0 && count !== fittedFor) {
      fittedFor = count;
      queueMicrotask(fit);
    }
  });

  function toWorld(cx: number, cy: number) {
    const r = canvas!.getBoundingClientRect();
    return { x: (cx - r.left - panX) / zoom, y: (cy - r.top - panY) / zoom };
  }
  function wheel(node: HTMLElement) {
    const h = (e: WheelEvent) => {
      e.preventDefault();
      const r = node.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top;
      const nz = clampZoom(zoom * Math.exp(-e.deltaY * 0.0015));
      panX = mx - ((mx - panX) / zoom) * nz; panY = my - ((my - panY) / zoom) * nz; zoom = nz;
    };
    node.addEventListener('wheel', h, { passive: false });
    return { destroy() { node.removeEventListener('wheel', h); } };
  }

  function bgDown(e: PointerEvent) {
    if (e.button !== 0 || !canvas) return;
    selectedEdge = null;
    mode = { kind: 'pan', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function blockDown(e: PointerEvent, block: Block) {
    if (e.button !== 0) return;
    e.stopPropagation();
    selectedEdge = null; selected = block.key;
    mode = { kind: 'drag', key: block.key, sx: e.clientX, sy: e.clientY, moved: false };
  }
  function portDown(e: PointerEvent, block: Block, port: Port) {
    if (e.button !== 0 || !canvas) return;
    e.stopPropagation();
    const w = toWorld(e.clientX, e.clientY);
    link = { block: block.key, port: port.name, color: typeColor(port.type), x: w.x, y: w.y, ax: port.x, ay: port.y };
    mode = { kind: 'connect', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function edgeDown(e: PointerEvent, id: string) {
    e.stopPropagation();
    selected = null; selectedEdge = id;
  }
  function move(e: PointerEvent) {
    if (!mode) return;
    const dx = e.clientX - mode.sx, dy = e.clientY - mode.sy;
    if (Math.abs(dx) + Math.abs(dy) > 3) mode.moved = true;
    if (mode.kind === 'pan') { panX += e.movementX; panY += e.movementY; }
    else if (mode.kind === 'drag' && mode.moved && mode.key) {
      offsets = { [mode.key]: { x: dx / zoom, y: dy / zoom } };
    } else if (mode.kind === 'connect' && link) {
      const w = toWorld(e.clientX, e.clientY);
      link = { ...link, x: w.x, y: w.y };
    }
  }
  function up(e: PointerEvent) {
    const m = mode; mode = null;
    if (!m) return;
    if (m.kind === 'pan') { if (!m.moved) { selected = null; selectedEdge = null; } return; }
    if (m.kind === 'drag' && m.moved && m.key) {
      const block = blocks.find(b => b.key === m.key), o = offsets[m.key!];
      if (block && o) positions = { ...positions, [m.key!]: { x: roundPos(block.x + o.x), y: roundPos(block.y + o.y) } };
      offsets = {};
      return;
    }
    if (m.kind === 'connect' && link) resolveConnect(e);
  }

  function resolveConnect(e: PointerEvent) {
    const l = link; link = null;
    if (!l) return;
    const hit = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>('[data-pe-port="in"]');
    if (!hit) return;
    const target = blocks.find(b => b.key === hit.dataset.peBlock);
    const source = blocks.find(b => b.key === l.block);
    if (!target || target.kind !== 'phase' || !source) { note = $t("Relier vers le port d’une passe."); return; }
    if (l.port === 'image' && source.kind === 'phase') {
      const from = steps.findIndex(s => s.id === source.refId), to = steps.findIndex(s => s.id === target.refId);
      if (to <= 0) { note = $t("La première passe ne reçoit pas d’image amont."); return; }
      if (from >= to) { note = $t("La source doit précéder la cible (le noyau refuse un cycle)."); return; }
      const step = steps[to];
      if (step.sourceStep) { note = $t("Entrée image déjà reliée : retirez le lien d’abord."); return; }
      step.sourceStep = source.refId; step.nodeId = '';
      if (step.workflowId) loadWorkflow(step.workflowId);
      note = $t("Passe {source} → {target} reliée (choisissez le node LoadImage cible).", { source: source.refId, target: target.refId });
      return;
    }
    if (source.kind === 'config') {
      const config = configs.find(c => c.id === source.refId);
      if (!config?.fields.some(f => f.key === l.port)) { note = $t("Champ de config introuvable."); return; }
      if (links.some(x => x.config === source.refId && x.step === target.refId && x.field === l.port)) { note = $t("Liaison déjà présente."); return; }
      links = [...links, { config: source.refId, step: target.refId, field: l.port }];
      note = $t("{config} · {port} → passe {target}.", { config: config.label, port: l.port, target: target.refId });
      return;
    }
    note = $t("Relier une sortie d’image de passe, ou un champ de config, vers une passe.");
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return;
    if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); removeSelected(); }
  }

  function removeSelected() {
    if (selectedEdge) {
      if (selectedEdge.startsWith('img:')) {
        const step = steps.find(s => s.id === selectedEdge!.slice(4));
        if (step) { step.sourceStep = ''; step.nodeId = ''; }
      } else {
        const [_, config, step, field] = selectedEdge.split(':');
        links = links.filter(x => !(x.config === config && x.step === step && x.field === field));
      }
      selectedEdge = null;
      return;
    }
    if (!selected) return;
    const block = blocks.find(b => b.key === selected);
    if (!block) return;
    if (block.kind === 'phase') {
      const i = steps.findIndex(s => s.id === block.refId);
      if (i >= 0) steps = removeStep(steps, i);
      links = pruneConfigLinks(configs, steps, links);
    } else {
      configs = configs.filter(c => c.id !== block.refId);
      links = links.filter(l => l.config !== block.refId);
    }
    selected = null;
  }

  function addPhase() {
    if (!newPhaseWorkflow) return;
    const step = newDraftStep(steps);
    step.workflowId = newPhaseWorkflow;
    steps = [...steps, step];
    loadWorkflow(newPhaseWorkflow);
    note = $t("Passe {id} ajoutée.", { id: step.id });
  }
  function addConfig() { configs = [...configs, newDraftConfig(configs)]; }

  function renameStep(step: DraftStep, id: string) {
    const old = step.id;
    if (!id.trim() || old === id) return;
    step.id = id;
    for (const s of steps) if (s.sourceStep === old) s.sourceStep = id;
    links = links.map(l => l.step === old ? { ...l, step: id } : l);
  }
  function setWorkflow(step: DraftStep, wf: string) {
    step.workflowId = wf; step.nodeId = '';
    if (wf) loadWorkflow(wf);
  }
</script>


<svelte:window onpointermove={move} onpointerup={up} onkeydown={onKey} />

<section class="pe" data-pipeline-editor>
  <div class="pe-main">
    <div class="wb-bar pe-bar">
      {@render lead?.()}
      <select data-pe-phase-workflow bind:value={newPhaseWorkflow} aria-label={$t("Passe : workflow")} title={$t("Passe : workflow")}>
        <option value="">{$t("Passe : workflow")}</option>
        {#each workflows as w (w.id)}<option value={w.id}>{w.name}</option>{/each}
      </select>
      <button data-pe-add-phase disabled={!newPhaseWorkflow} onclick={addPhase}>{$t("+ Passe")}</button>
      <button data-pe-add-config onclick={addConfig}>{$t("+ Config globale")}</button>
      <span class="pe-sep" aria-hidden="true"></span>
      <button class="quiet" data-pe-fit onclick={fit}>{$t("Ajuster")}</button>
      <button class="quiet icon-btn" onclick={() => { zoom = clampZoom(zoom / 1.2); }} aria-label={$t("Zoom arrière")} title={$t("Zoom arrière")}>−</button>
      <span class="num muted pe-zoom">{Math.round(zoom * 100)}%</span>
      <button class="quiet icon-btn" onclick={() => { zoom = clampZoom(zoom * 1.2); }} aria-label={$t("Zoom avant")} title={$t("Zoom avant")}>+</button>
      <button class="quiet" data-pe-delete disabled={!selected && !selectedEdge} onclick={removeSelected}>{$t("Supprimer la sélection")}</button>
      <span class="grow"></span>
      {@render trail?.()}
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="pe-canvas" data-pe-canvas bind:this={canvas} use:wheel onpointerdown={bgDown} role="application" aria-label={$t("Canevas du pipeline")} title={$t("Cliquez un bloc pour l'éditer. Glissez la sortie « image » d'une passe vers l'entrée « image » d'une passe suivante ; glissez un champ de config vers le port « config » d'une passe. Suppr : supprimer la sélection.")}>
      <div class="pe-world" style="transform:translate({panX}px,{panY}px) scale({zoom})">
        <svg class="pe-edges" width="1" height="1" style="overflow:visible">
          {#each edges as e (e.id)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <g data-pe-edge={e.id} class:pe-edge-sel={selectedEdge === e.id} onpointerdown={ev => edgeDown(ev, e.id)}>
              <path d={e.d} stroke="transparent" stroke-width="12" fill="none" />
              <path d={e.d} stroke={e.color} stroke-width={selectedEdge === e.id ? 4 : 2} fill="none" />
            </g>
          {/each}
          {#if link}<path d={`M${link.ax},${link.ay} L${link.x},${link.y}`} stroke={link.color} stroke-width="2" stroke-dasharray="5 4" fill="none" />{/if}
        </svg>
        {#each blocks as b (b.key)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="pe-block pe-{b.kind}" class:pe-sel={selected === b.key}
            data-pe-block={b.key}
            data-pe-phase={b.kind === 'phase' ? b.refId : undefined}
            data-pe-config={b.kind === 'config' ? b.refId : undefined}
            style="left:{b.x}px;top:{b.y}px;width:{b.w}px;height:{b.h}px"
            onpointerdown={e => blockDown(e, b)}>
            <div class="pe-head" title={b.title}>{b.kind === 'phase' ? `[ ${b.title} ]` : b.title}<small>{b.sub}</small></div>
            {#each b.inputs as p, pi (pi)}
              <span class="pe-port pe-in" data-pe-port="in" data-pe-block={b.key} data-pe-name={p.name}
                title={p.type + (p.linked ? ' ' + $t("· relié") : '')}
                style="top:{HEAD_H + p.index * ROW_H}px;height:{ROW_H}px"
                onpointerdown={e => portDown(e, b, p)}>
                <i style="background:{typeColor(p.type)}"></i>{p.label}{#if p.linked}<small>{$t("· relié")}</small>{/if}
              </span>
            {/each}
            {#each b.outputs as p, po (po)}
              <span class="pe-port pe-out" data-pe-port="out" data-pe-block={b.key} data-pe-name={p.name}
                title={$t("glisser vers une passe : {type}", { type: p.type })}
                style="top:{HEAD_H + p.index * ROW_H}px;height:{ROW_H}px"
                onpointerdown={e => portDown(e, b, p)}>
                <small>{p.type}</small>{p.label}<i style="background:{typeColor(p.type)}"></i>
              </span>
            {/each}
          </div>
        {/each}
      </div>
    </div>
    <footer class="pe-foot">
      {#if note}<p class="pe-note" role="status">{note}</p>{/if}
      <span class="pe-status num" data-pe-status>{$tn("{n} passe", "{n} passes", steps.length)} · {$tn("{n} config", "{n} configs", configs.length)} · {$tn("{n} liaison", "{n} liaisons", linkCount)}{problems.length ? ' · ' + $tn("{n} problème", "{n} problèmes", problems.length) : ''}</span>
    </footer>
  </div>
  <aside class="wb-panel right pe-props" data-pe-props>
    <div class="wb-head">
      <h2>{#if selectedStep && single}{$t("Passe [ {title} ]", { title: single.title })}{:else if selectedConfig && single}{single.title}{:else}{$t("Sélection")}{/if}</h2>
    </div>
    <div class="wb-scroll">
      {#if selectedStep && single}
        <div class="pe-sel-body">
          <label>{$t("Identifiant")}<input data-pe-step-id value={selectedStep.id} onchange={e => renameStep(selectedStep, e.currentTarget.value)} /></label>
          <label>{$t("Workflow (fichier API de la passe)")}
            <select data-pe-step-workflow value={selectedStep.workflowId} onchange={e => setWorkflow(selectedStep, e.currentTarget.value)}>
              <option value="">{$t("— choisir —")}</option>
              {#each workflows as w (w.id)}<option value={w.id}>{w.name}</option>{/each}
            </select>
          </label>
          {#if selectedStep.sourceStep}
            <p class="pe-hint">{$t("Image amont : passe {step}", { step: selectedStep.sourceStep })}</p>
            {#if selectedStep.workflowId && (nodes[selectedStep.workflowId] ?? []).length}
              <label>{$t("Node LoadImage cible")}
                <select data-pe-step-node bind:value={selectedStep.nodeId}>
                  <option value="">{$t("— choisir —")}</option>
                  {#each nodes[selectedStep.workflowId] as n (n.id)}<option value={n.id}>{n.id}{n.title ? ` · ${n.title}` : ''}</option>{/each}
                </select>
              </label>
            {:else if nodes[selectedStep.workflowId]}
              <p class="pe-hint">{$t("Ce workflow n'a pas de LoadImage : il ne peut pas recevoir l'image amont.")}</p>
            {/if}
          {/if}
        </div>
      {:else if selectedConfig && single}
        <div class="pe-sel-body">
          <label>{$t("Nom")}<input data-pe-config-label bind:value={selectedConfig.label} /></label>
          {#each selectedConfig.fields as f, k (k)}
            <div class="pe-field">
              <label title={$t("Champs : section ({sections}), « seed » (u64), ou « node.entrée » explicite (ex. 1.ckpt_name pour le model, le VAE ou une LoRA). Reliez chaque champ au port « config » des passes concernées.", { sections: CONFIG_SECTIONS.join(', ') })}>{$t("Clé")}<input data-pe-field-key={k} bind:value={f.key} list="pe-config-keys" onchange={() => { links = pruneConfigLinks(configs, steps, links); }} /></label>
              <label>{$t("Valeur")}<textarea data-pe-field-value={k} rows="3" bind:value={f.value}></textarea></label>
              <button class="quiet" data-pe-field-remove={k} onclick={() => { selectedConfig.fields.splice(k, 1); links = pruneConfigLinks(configs, steps, links); }}>{$t("Retirer le champ")}</button>
            </div>
          {/each}
          <button data-pe-add-field onclick={() => selectedConfig.fields.push({ key: 'P1', value: '' })} title={$t("Champs : section ({sections}), « seed » (u64), ou « node.entrée » explicite (ex. 1.ckpt_name pour le model, le VAE ou une LoRA). Reliez chaque champ au port « config » des passes concernées.", { sections: CONFIG_SECTIONS.join(', ') })}>{$t("Ajouter un champ")}</button>
        </div>
      {:else}
        <p class="pe-hint pe-sel-body" title={$t("Cliquez un bloc pour l'éditer. Glissez la sortie « image » d'une passe vers l'entrée « image » d'une passe suivante ; glissez un champ de config vers le port « config » d'une passe. Suppr : supprimer la sélection.")}>{$t("Cliquez un bloc pour l’éditer.")}</p>
      {/if}
      {#if problems.length}
        <details class="sec pe-problems" data-pe-problems open>
          <summary><span class="sec-title">{$t("À compléter avant d'enregistrer ({n})", { n: problems.length })}</span></summary>
          <div class="sec-body"><ul>{#each problems as p}<li>{p}</li>{/each}</ul></div>
        </details>
      {/if}
      {@render side?.()}
    </div>
  </aside>
  <datalist id="pe-config-keys">{#each [...CONFIG_SECTIONS, 'seed'] as k (k)}<option value={k}></option>{/each}</datalist>
</section>

<style>
  .pe{display:grid;grid-template-columns:minmax(0,1fr) 320px;height:100%;min-height:0}
  .pe-main{display:flex;flex-direction:column;min-width:0;min-height:0;background:var(--canvas)}
  .pe-bar{gap:4px}
  .pe-bar select{width:180px;min-width:110px}
  .pe-sep{width:1px;height:18px;background:var(--line-strong);margin:0 4px;flex:none}
  .pe-zoom{min-width:40px;text-align:center;font-size:11.5px}
  .grow{flex:1}
  /* Canvas: block coordinates stay physical (left/top) on purpose. */
  .pe-canvas{position:relative;flex:1;min-height:0;overflow:hidden;background-color:var(--canvas);background-image:radial-gradient(var(--line) 1px,transparent 1px);background-size:24px 24px;touch-action:none;cursor:grab}
  .pe-world{position:absolute;inset:0;transform-origin:0 0}
  .pe-edges{position:absolute;top:0;left:0}
  .pe-block{position:absolute;border:1px solid var(--line-strong);border-radius:6px;background:var(--panel);user-select:none;font-size:12px;cursor:move}
  .pe-config{border-style:dashed}
  .pe-sel{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent)}
  .pe-head{display:flex;gap:6px;height:30px;padding:0 8px;align-items:center;font-weight:600;color:var(--text);background:var(--raised);border-bottom:1px solid var(--line);border-radius:5px 5px 0 0;overflow:hidden;white-space:nowrap}
  .pe-head small{font-weight:400;color:var(--muted);overflow:hidden;text-overflow:ellipsis}
  .pe-port{position:absolute;display:flex;align-items:center;gap:5px;padding:0 8px;font-size:11.5px;color:var(--text);cursor:crosshair;white-space:nowrap}
  .pe-port i{width:9px;height:9px;border-radius:50%;flex:none}
  .pe-port small{color:var(--faint)}
  .pe-in{left:0}.pe-out{right:0;justify-content:flex-end}
  .pe-edge-sel path:last-child{stroke:var(--accent)}
  .pe-foot{flex:none;display:grid;border-top:1px solid var(--line);background:var(--bg)}
  .pe-note{margin:0;padding:4px 12px;font-size:12px;color:var(--text);border-bottom:1px solid var(--line)}
  .pe-status{min-height:26px;line-height:26px;padding:0 12px;font-size:11.5px;color:var(--muted)}
  .pe-props h2{font-size:13px}
  .pe-sel-body{display:grid;gap:8px;padding:10px 12px 12px;border-bottom:1px solid var(--line)}
  .pe-sel-body label{gap:3px;color:var(--muted);font-size:12px}
  .pe-sel-body input,.pe-sel-body select,.pe-sel-body textarea{color:var(--text);width:100%}
  .pe-hint{margin:0;color:var(--muted);font-size:12px}
  .pe-field{display:grid;gap:4px;padding-bottom:8px;border-bottom:1px solid var(--line)}
  .pe-field button{justify-self:start;min-height:24px;font-size:12px}
  .pe-problems ul{margin:0;padding-inline-start:16px;color:var(--warning);font-size:12px}
  @media(max-width:1100px){.pe{grid-template-columns:minmax(0,1fr) 280px}}
</style>
