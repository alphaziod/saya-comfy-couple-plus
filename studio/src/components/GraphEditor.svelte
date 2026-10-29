<script lang="ts">
  import { t } from '../lib/i18n';
  // Choice: in-house HTML/SVG rendering (not @xyflow/svelte). @xyflow/svelte 1.7.0 (MIT, svelte ^5.25) would fit,
  // but node_modules is a link to another checkout: npm install would modify it there. The editing state
  // lives entirely in Rust; here only view -> scene (src/lib/graph/view.ts), pan/zoom and IPC operations.
  import { invoke } from '@tauri-apps/api/core';
  import Icon from './Icon.svelte';
  import { onDestroy, onMount } from 'svelte';
  import { call, message, native } from '../lib/api';
  import {
    buildScene, canConnect, centerOn, clampZoom, filterNodes, fitTransform, knownClasses, nodeKey, nodesInRect,
    roundPos, HEAD_H, ROW_H, type GraphView, type Scene,
  } from '../lib/graph/view';

  import {
    parseModuleList, instanceId, type ModuleSummary,
  } from '../lib/module-contracts';
  type Op = { op: string } & Record<string, unknown>;
  type ApiNode = { class_type: string; inputs: Record<string, unknown>; _meta?: { title?: string } };
  type ApiReport = {
    complete: boolean; validatedAgainstSchema: boolean; apiNodes: number;
    subgraphDefinitions: number; topLevelInstances: number; nestedInstances: number;
    hiddenWidgetsExcluded: number; promotedWidgetsResolved: number; knownWidgetRulesApplied: number;
    unresolved: { executionId: string; classType: string; cause: string }[];
    unverifiedClasses: string[];
  };
  type ApiPreview = { api: Record<string, ApiNode>; report: ApiReport; previewHash: string };
  type Fragment = { format: number; nodes: unknown[]; connections: unknown[] };
  type SessionItem = { id: string; name: string; workflow: string; sourceFormat: string; updatedAt: string };
  let workflows = $state<{ id: string; name: string; promptProfile: string }[]>([]);
  let workflowId = $state('');
  let filePath = $state(''), showEdges = $state(false);
  let session = $state<number | string | null>(null);
  let view = $state<GraphView | null>(null);
  let error = $state(''), note = $state(''), busy = $state(false);
  let selected = $state<string[]>([]), selectedEdge = $state<string | null>(null);
  let selectedGroupId = $state<string | null>(null);
  let selectedContainerId = $state<string | null>(null);
  let groupGesture = $state<null | { id: string; mode: 'move' | 'resize'; sx: number; sy: number; bounds: { x: number; y: number; width: number; height: number } }>(null);
  let zoom = $state(1), panX = $state(0), panY = $state(0);
  let offsets = $state<Record<string, { x: number; y: number }>>({});
  let search = $state(''), newClass = $state(''), groupTitle = $state('Groupe'), exportName = $state('');
  let catalog = $state<{name:string;displayName:string;category:string;description:string;inputs:any[];outputs:any[];outputNode:boolean}[]>([]);
  let catalogOpen = $state(false), catalogSearch = $state(''), catalogCategory = $state(''), catalogFavorites = $state<string[]>([]), catalogRecent = $state<string[]>([]);
  let apiPreview = $state<ApiPreview | null>(null), apiName = $state(''), apiConfirmed = $state(false);
  let clipboard = $state<Fragment | null>(null), pasteRound = $state(0);
  let sessions = $state<SessionItem[]>([]), resumeId = $state(''), sessionDeleteArmed = $state('');
  let drafts = $state<Record<string, string>>({});
  // Visual extraction of a module: the current selection becomes a reusable
  // module; the ports (crossing links) are computed by the backend.
  type ModuleExtraction = {
    id: string; version: number; nodeCount: number;
    inputs: { name: string; kind: string; node: string; input: string }[];
    outputs: { name: string; kind: string; node: string; outputIndex: number }[];
  };
  let moduleFormOpen = $state(false), moduleName = $state(''), moduleDescription = $state('');
  let moduleTarget = $state(''), modulesList = $state<ModuleSummary[]>([]), moduleResult = $state<ModuleExtraction | null>(null);
  let canvas = $state<HTMLDivElement | null>(null);
  let mode = $state<null | { kind: 'pan' | 'drag' | 'rect' | 'connect' | 'group-drag' | 'group-resize'; sx: number; sy: number; moved: boolean }>(null);
  let rect = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let link = $state<{ from: 'out' | 'in'; node: string; port: number; name: string; x: number; y: number; color: string; ax: number; ay: number } | null>(null);
  let queue: Promise<unknown> = Promise.resolve();

  const scene: Scene = $derived(view ? buildScene(view, offsets) : { nodes: [], edges: [], frames: [] });
  const originalIds = $derived(view?.originalIds ?? {});
  const matches = $derived(view ? filterNodes(view, search) : []);
  const classes = $derived(view ? knownClasses(view) : []);
  const single = $derived(selected.length === 1 ? scene.nodes.find(n => n.id === selected[0]) ?? null : null);
  const movableSelection = $derived(selected.filter(id => !scene.nodes.find(n => n.id === id)?.locked));
  const selectedGroup = $derived(view ? view.document.groups.find(g => g.id === selectedGroupId) ?? view.document.groups.find(g => g.nodeIds.some(id => selected.includes(id))) ?? null : null);
  const scoped = $derived(!!view?.scope);
  const selectedContainer = $derived(view ? view.document.subgraphs.find(c => c.id === selectedContainerId) ?? view.document.subgraphs.find(c => c.nodeIds.some((id: string) => selected.includes(id))) ?? null : null);
  const apiNodes = $derived(apiPreview ? Object.entries(apiPreview.api) : []);
  const apiReady = $derived(!!apiPreview && apiPreview.report.complete && apiPreview.report.validatedAgainstSchema);

  onMount(async () => {
    try{catalogFavorites=JSON.parse(localStorage.getItem('saya.nodeFavorites')||'[]');catalogRecent=JSON.parse(localStorage.getItem('saya.nodeRecent')||'[]');showEdges=localStorage.getItem('saya.showEdges')==='true'}catch{}
    if (!native) return;
    try { workflows = await call('workflows'); if (workflows.length) workflowId = workflows[0].id; } catch (e) { error = message(e); }
    await refreshSessions();
    void loadCatalog();
  });
  async function refreshSessions() {
    try { sessions = await call<SessionItem[]>('graph_sessions'); } catch (e) { error = message(e); }
  }
  onDestroy(() => { if (session !== null && native) void call('graph_close', { session }).catch(() => {}); });

  function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    const p = queue.then(async () => { busy = true; try { return await fn(); } catch (e) { error = message(e); return undefined; } finally { busy = false; } });
    queue = p.catch(() => {});
    return p;
  }
  function setView(v: GraphView) {
    view = v; offsets = {}; selected = selected.filter(id => v.document.nodes.some(n => n.id === id));
    selectedEdge = null; drafts = {}; apiPreview = null; apiConfirmed = false;
    if (selectedGroupId && !v.document.groups.some(g => g.id === selectedGroupId)) selectedGroupId = null;
    if (selectedContainerId && !v.document.subgraphs.some(c => c.id === selectedContainerId)) selectedContainerId = null;
  }
  async function edit(op: Op, ok = ''): Promise<boolean> {
    if (session === null) return false;
    error = ''; note = '';
    const r = await run(() => call<GraphView>('graph_edit', { session, op }));
    if (r) { setView(r); if (ok) note = ok; return true; }
    offsets = {};
    return false;
  }
  function viewport() { const r = canvas?.getBoundingClientRect(); return { w: r?.width ?? 800, h: r?.height ?? 600 }; }
  function fit() { const { w, h } = viewport(); const t = fitTransform(scene, w, h); zoom = t.zoom; panX = t.panX; panY = t.panY; }
  async function open() {
    if (!workflowId) return;
    error = ''; note = '';
    if (session !== null) await closeSession();
    selected = []; selectedGroupId = null; selectedContainerId = null; selectedEdge = null;
    const r = await run(() => call<{ session: number | string; view: GraphView }>('graph_open', { workflow: workflowId }));
    if (!r) return;
    session = r.session; setView(r.view);
    exportName = `${r.view.document.name || 'workflow'} (graphe)`;
    queueMicrotask(fit); await refreshSessions();
  }
  async function newWorkflow() {
    const r=await run(()=>call<{session:string;view:GraphView}>('graph_new'));
    if(!r)return;
    if(session!==null)await closeSession();
    session=r.session;setView(r.view);exportName='Nouveau workflow UI (copie)';queueMicrotask(fit);await refreshSessions();
  }
  async function openFile(pick = true) {
    error = ''; note = '';
    selected = []; selectedGroupId = null; selectedContainerId = null; selectedEdge = null;
    if (pick) {const path = await run(() => invoke<string | null>('pick_workflow_file')); if (!path) return; filePath = path;}
    if (!filePath.trim()) return;
    const r = await run(() => call<{session:string;view:GraphView}>('graph_open_file',{path:filePath.trim()}));
    if (!r) return;
    if (session !== null) await closeSession();
    session = r.session; setView(r.view); exportName = r.view.document.name + ' (copie)';
    queueMicrotask(fit); await refreshSessions();
  }
  async function saveUi() {
    if (session === null) return;
    error = ''; note = '';
    const r = await run(() => call<{path:string}>('graph_save_ui',{session,name:exportName.trim()}));
    if (r) note = $t("Copie UI enregistrée : {path}", { path: r.path });
  }
  async function enterSubgraphFrame(id:string) {
    if(session===null||busy)return;
    const r=await run(()=>call<GraphView>('graph_enter_subgraph',{session,container:id}));
    if(r){setView(r);selected=[];selectedContainerId=null;queueMicrotask(fit);}
  }
  async function enterSubgraph() {
    if (session === null || !selectedContainer) return;
    error = ''; note = '';
    const r = await run(() => call<GraphView>('graph_enter_subgraph', { session, container: selectedContainer.id }));
    if (!r) return;
    setView(r); selected = []; selectedContainerId = null;
    note = $t("Subgraph ouvert : positions et paramètres éditables ; modification structurelle refusée.");
  }
  async function exitSubgraph(commit: boolean) {
    if (session === null) return;
    error = ''; note = '';
    const r = await run(() => call<GraphView>('graph_exit_subgraph', { session, commit }));
    if (!r) return;
    setView(r); selected = [];
    note = commit ? $t("Subgraph réinjecté dans la source UI.") : $t("Sortie du subgraph sans modification.");
    await refreshSessions();
  }
  async function closeSession() {
    const s = session; session = null; view = null; selected = []; selectedEdge = null; offsets = {}; catalogOpen = false;
    if (s !== null) await run(() => call('graph_close', { session: s }));
    await refreshSessions();
  }
  async function resumeSession() {
    if (session !== null) await closeSession();
    const id = resumeId;
    if (!id) return;
    error = ''; note = '';
    const r = await run(() => call<{ session: string; view: GraphView }>('graph_resume', { id }));
    if (!r) return;
    session = r.session; setView(r.view); selected = [];
    exportName = `${r.view.document.name || 'workflow'} (reprise)`;
    queueMicrotask(fit); await refreshSessions();
    note = $t("Session reprise.");
  }
  async function deleteSession() {
    const id = resumeId;
    if (!id) return;
    if (sessionDeleteArmed !== id) {
      sessionDeleteArmed = id;
      note = $t("Cliquez à nouveau sur « Confirmer la suppression » pour retirer cette session.");
      return;
    }
    sessionDeleteArmed = ''; error = ''; note = '';
    const ok = await run(() => call('graph_delete_session', { id }));
    if (ok === undefined) return;
    if (resumeId === id) resumeId = '';
    note = $t("Session supprimée."); await refreshSessions();
  }
  async function exportNew() {
    if (session === null) return;
    error = ''; note = '';
    const r = await run(() => call<{ workflowId: string; nodeCount: number }>('graph_export', { session, name: exportName.trim() }));
    if (r) note = $t("Nouveau workflow {id} créé ({n} nodes). L’original est inchangé.", { id: r.workflowId, n: r.nodeCount });
  }
  async function prepareApi() {
    if (session === null) return;
    error = ''; note = ''; apiPreview = null; apiConfirmed = false;
    const r = await run(() => call<ApiPreview>('graph_prepare_api', { session }));
    if (!r) return;
    apiPreview = r;
    apiName = `${exportName.trim() || 'workflow'} (API)`;
    note = r.report.complete && r.report.validatedAgainstSchema
      ? $t("Conversion complète : {n} nodes API. Vérifiez les diagnostics puis confirmez la création.", { n: r.report.apiNodes })
      : $t("Conversion incomplète ou schéma non vérifié : la création d’une révision API est bloquée.");
  }
  async function saveApi() {
    const preview = apiPreview;
    if (session === null || !preview || !apiReady || !apiConfirmed) return;
    error = ''; note = '';
    const r = await run(() => call<{ workflowId: string; nodeCount: number }>('graph_save_api', {
      session, name: apiName.trim(), previewHash: preview.previewHash, confirmed: true,
    }));
    if (!r) return;
    note = $t("Révision API {id} créée ({n} nodes). Aucune soumission effectuée ; la source UI reste inchangée.", { id: r.workflowId, n: r.nodeCount });
    apiPreview = null; apiConfirmed = false;
    try { workflows = await call('workflows'); } catch { /* the list refreshes on next open */ }
    await refreshSessions();
  }

  // --- coordinates
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

  // --- pointer
  function bgDown(e: PointerEvent) {
    if (e.button !== 0 || !canvas) return;
    selectedGroupId = null;
    const w = toWorld(e.clientX, e.clientY);
    if (e.shiftKey) { rect = { x0: w.x, y0: w.y, x1: w.x, y1: w.y }; mode = { kind: 'rect', sx: e.clientX, sy: e.clientY, moved: false }; }
    else mode = { kind: 'pan', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function nodeDown(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    e.stopPropagation(); selectedEdge = null; selectedGroupId = null;
    if (e.shiftKey) selected = selected.includes(id) ? selected.filter(x => x !== id) : [...selected, id];
    else if (!selected.includes(id)) selected = [id];
    selectedContainerId = view?.document.subgraphs.find(c => c.nodeIds.includes(id))?.id ?? null;
    mode = { kind: 'drag', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function portDown(e: PointerEvent, from: 'out' | 'in', node: string, port: number, name: string, color: string, ax: number, ay: number) {
    if (e.button !== 0 || !canvas) return;
    e.stopPropagation();
    const w = toWorld(e.clientX, e.clientY);
    link = { from, node, port, name, color, x: w.x, y: w.y, ax, ay };
    mode = { kind: 'connect', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function subgraphDown(e: PointerEvent, frame: { id: string; nodeIds: string[] }) {
    if (e.button !== 0) return;
    e.stopPropagation();
    selected = [...frame.nodeIds];
    selectedEdge = null;
    selectedGroupId = null;
    selectedContainerId = frame.id;
  }
  function groupDown(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    e.stopPropagation();
    const frame = scene.frames.find(f => f.id === id && f.kind === 'group');
    if (!frame) return;
    selected = []; selectedEdge = null; selectedGroupId = id; selectedContainerId = null;
    groupGesture = { id, mode: 'move', sx: e.clientX, sy: e.clientY, bounds: { x: frame.x, y: frame.y, width: frame.w, height: frame.h } };
    mode = { kind: 'group-drag', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function groupResizeDown(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    e.stopPropagation();
    const frame = scene.frames.find(f => f.id === id && f.kind === 'group');
    if (!frame) return;
    selected = []; selectedEdge = null; selectedGroupId = id; selectedContainerId = null;
    groupGesture = { id, mode: 'resize', sx: e.clientX, sy: e.clientY, bounds: { x: frame.x, y: frame.y, width: frame.w, height: frame.h } };
    mode = { kind: 'group-resize', sx: e.clientX, sy: e.clientY, moved: false };
  }
  function move(e: PointerEvent) {
    if (!mode) return;
    const dx = e.clientX - mode.sx, dy = e.clientY - mode.sy;
    if (Math.abs(dx) + Math.abs(dy) > 3) mode.moved = true;
    if (mode.kind === 'pan') { panX += e.movementX; panY += e.movementY; }
    else if (mode.kind === 'drag' && mode.moved) {
      const o: Record<string, { x: number; y: number }> = {};
      for (const id of movableSelection) o[id] = { x: dx / zoom, y: dy / zoom };
      offsets = o;
    } else if (mode.kind === 'rect' && rect) { const w = toWorld(e.clientX, e.clientY); rect = { ...rect, x1: w.x, y1: w.y }; }
    else if (mode.kind === 'connect' && link) { const w = toWorld(e.clientX, e.clientY); link = { ...link, x: w.x, y: w.y }; }
  }
  async function up(e: PointerEvent) {
    const m = mode; mode = null;
    if (!m) return;
    if (m.kind === 'group-drag' || m.kind === 'group-resize') {
      const gesture = groupGesture; groupGesture = null;
      if (!gesture) return;
      const dx = (e.clientX - m.sx) / zoom, dy = (e.clientY - m.sy) / zoom;
      if (m.kind === 'group-drag') {
        if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
        await edit({ op: 'move_group', group: gesture.id, dx: roundPos(dx), dy: roundPos(dy) });
      } else {
        const width = Math.max(80, gesture.bounds.width + dx), height = Math.max(80, gesture.bounds.height + dy);
        await edit({ op: 'resize_group', group: gesture.id, bounds: { x: gesture.bounds.x, y: gesture.bounds.y, width: roundPos(width), height: roundPos(height) } });
      }
      return;
    }
    if (m.kind === 'pan' && !m.moved) { selected = []; selectedEdge = null; }
    else if (m.kind === 'rect' && rect) { selected = nodesInRect(scene, rect); rect = null; }
    else if (m.kind === 'drag' && m.moved) {
      const moves = movableSelection.map(id => { const n = scene.nodes.find(x => x.id === id)!; return { node: id, x: roundPos(n.x), y: roundPos(n.y) }; });
      if (moves.length) await edit({ op: 'move', moves });
      else offsets = {};
    } else if (m.kind === 'connect' && link) {
      const l = link; link = null;
      const portEl = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>('[data-port]');
      if (!portEl) return;
      const kind = portEl.dataset.port, node = portEl.dataset.node ?? '', port = Number(portEl.dataset.index), name = portEl.dataset.name ?? '';
      let fromN: string, outIdx: number, toN: string, inName: string;
      if (l.from === 'out' && kind === 'in') { fromN = l.node; outIdx = l.port; toN = node; inName = name; }
      else if (l.from === 'in' && kind === 'out') { fromN = node; outIdx = port; toN = l.node; inName = l.name; }
      else { error = $t("Relier une sortie à une entrée."); return; }
      if (!canConnect(scene, { node: fromN, output: outIdx }, { node: toN, input: inName })) {
        // Rust remains the arbiter: let it return the exact message (cycle, type, linked input…).
        if (fromN === toN) { error = $t("Un node ne peut pas être relié à lui-même."); return; }
      }
      await edit({ op: 'connect', fromNode: fromN, output: outIdx, toNode: toN, input: inName });
    }
  }
  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return;
    if (!view) return;
    const k = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && k === 'c') { e.preventDefault(); void copySelection(); }
    else if ((e.ctrlKey || e.metaKey) && k === 'v') { e.preventDefault(); void pasteClipboard(); }
    else if ((e.ctrlKey || e.metaKey) && k === 'd') { e.preventDefault(); void duplicateSelection(); }
    else if ((e.ctrlKey || e.metaKey) && k === 'a') { e.preventDefault(); selected = view.document.nodes.filter(n => !scene.nodes.find(s => s.id === n.id)?.locked).map(n => n.id); selectedEdge = null; }
    else if ((e.ctrlKey || e.metaKey) && k === 'z' && !e.shiftKey) { e.preventDefault(); if (view.canUndo) void edit({ op: 'undo' }); }
    else if ((e.ctrlKey || e.metaKey) && (k === 'y' || (k === 'z' && e.shiftKey))) { e.preventDefault(); if (view.canRedo) void edit({ op: 'redo' }); }
    else if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); void removeSelection(); }
  }
  async function copySelection() {
    if (session === null || !selected.length) return;
    error = ''; note = '';
    const r = await run(() => call<Fragment>('graph_copy', { session, nodes: [...selected] }));
    if (!r) return;
    clipboard = r; pasteRound = 0;
    note = $t("{n} node(s) copiés.", { n: selected.length });
  }
  async function pasteClipboard() {
    if (session === null || !clipboard) return;
    error = ''; note = '';
    const offset = { x: 30 * (pasteRound + 1), y: 30 * (pasteRound + 1) };
    const r = await run(() => call<{ nodes: string[]; view: GraphView }>('graph_paste', { session, fragment: clipboard, offset }));
    if (!r) return;
    setView(r.view); selected = r.nodes; pasteRound += 1;
    note = $t("{n} node(s) collés avec de nouveaux identifiants.", { n: r.nodes.length });
  }
  async function duplicateSelection() {
    if (session === null || !movableSelection.length) return;
    error = ''; note = '';
    const ids = [...movableSelection];
    const copy = await run(() => call<Fragment>('graph_copy', { session, nodes: ids }));
    if (!copy) return;
    const r = await run(() => call<{ nodes: string[]; view: GraphView }>('graph_paste', { session, fragment: copy, offset: { x: 20, y: 20 } }));
    if (!r) return;
    setView(r.view); selected = r.nodes;
    note = $t("{n} node(s) dupliqués.", { n: ids.length });
  }
  async function removeSelection() {
    if (selectedEdge) { await edit({ op: 'disconnect', connection: selectedEdge }); return; }
    if (movableSelection.length) { const ids = [...movableSelection]; if (await edit({ op: 'remove', nodes: ids })) selected = []; }
  }
  function centerNode(id: string) {
    const n = scene.nodes.find(x => x.id === id); if (!n) return;
    selected = [id]; selectedEdge = null;
    const { w, h } = viewport(); const t = centerOn({ x: n.x + n.w / 2, y: n.y + n.h / 2 }, zoom, w, h);
    panX = t.panX; panY = t.panY;
  }
  const catalogGroups = $derived.by(()=>{const map=new Map<string,typeof catalog>();for(const n of catalog){const parts=String(n.category||'Divers').split('/');const top=parts[0]||'Divers';if(!map.has(top))map.set(top,[]);map.get(top)!.push(n)}return [...map.entries()].sort((a,b)=>a[0].localeCompare(b[0]))});
  const catalogFiltered = $derived(catalog.filter(n=>{const q=catalogSearch.trim().toLowerCase();const cat=String(n.category||'');return (!catalogCategory||cat===catalogCategory||cat.startsWith(catalogCategory+'/'))&&(!q||`${n.displayName} ${n.name} ${cat}`.toLowerCase().includes(q))}).slice(0,200));
  async function loadCatalog(){if(catalog.length)return;const r=await call<{nodes:typeof catalog}>('node_catalog');catalog=r.nodes}
  async function addClassAt(cls:string,x?:number,y?:number){
    if(!canvas)return;const {w,h}=viewport();const pos=x==null?{x:(w/2-panX)/zoom-115,y:(h/2-panY)/zoom-40}:{x:roundPos(x??0),y:roundPos(y??0)};
    if(await edit({op:'add',classType:cls,x:pos.x,y:pos.y})){catalogRecent=[cls,...catalogRecent.filter(v=>v!==cls)].slice(0,12);localStorage.setItem('saya.nodeRecent',JSON.stringify(catalogRecent));}
  }
  function toggleCatalogFavorite(cls:string){catalogFavorites=catalogFavorites.includes(cls)?catalogFavorites.filter(v=>v!==cls):[...catalogFavorites,cls];localStorage.setItem('saya.nodeFavorites',JSON.stringify(catalogFavorites))}
  function dropNode(e:DragEvent){e.preventDefault();const cls=e.dataTransfer?.getData('text/saya-node');if(!cls||!canvas)return;const r=canvas.getBoundingClientRect();const x=(e.clientX-r.left-panX)/zoom,y=(e.clientY-r.top-panY)/zoom;void addClassAt(cls,x,y)}
  async function addNode() {
    const cls = newClass.trim(); if (!cls || !canvas) return;
    const { w, h } = viewport(); const c = { x: (w / 2 - panX) / zoom - 115, y: (h / 2 - panY) / zoom - 40 };
    if (await edit({ op: 'add', classType: cls, x: roundPos(c.x), y: roundPos(c.y) })) newClass = '';
  }
  async function makeGroup() {
    if (movableSelection.length && await edit({ op: 'group', title: groupTitle.trim() || 'Groupe', nodes: [...movableSelection] })) note = $t("Groupe créé.");
  }
  async function openModuleForm() {
    moduleFormOpen = true; moduleResult = null; moduleTarget = '';
    error = '';
    try { modulesList = parseModuleList(await call('modules')); }
    catch (e) { modulesList = []; error = message(e); }
  }
  async function extractModule() {
    if (session === null || !movableSelection.length || !moduleName.trim()) return;
    error = ''; note = '';
    const r = await run(() => call<ModuleExtraction>('graph_extract_module', {
      session, nodes: [...movableSelection], module: moduleTarget || null, name: moduleName.trim(), description: moduleDescription.trim(),
    }));
    if (!r) return;
    moduleResult = r;
    try { modulesList = parseModuleList(await call('modules')); } catch { /* refreshed on next open */ }
    note = $t("Module « {name} » enregistré (version {version}) : {inputs} entrée(s), {outputs} sortie(s). Réutilisable depuis la page Modules.", { name: moduleName.trim(), version: r.version, inputs: r.inputs.length, outputs: r.outputs.length });
    moduleName = ''; moduleDescription = ''; moduleTarget = '';
  }
  const dkey = (n: string, i: string) => `${n}\u0000${i}`;
  async function validate(node: string, input: string) {
    const v = drafts[dkey(node, input)] ?? view?.paramText[node]?.[input] ?? '';
    await edit({ op: 'set_param', node, input, value: v });
  }
  async function removeParam(node: string, input: string) { await edit({ op: 'set_param', node, input, value: null }); }
  function paramNames(id: string): string[] {
    const n = view?.document.nodes.find(x => x.id === id); if (!n) return [];
    return [...new Set([...n.inputs.map(i => i.name), ...Object.keys(view?.paramText[id] ?? {})])];
  }
  const nodeConnections = (id: string) => view ? view.document.connections.filter(c => c.from.node === id || c.to.node === id) : [];
</script>

<svelte:window onpointermove={move} onpointerup={up} onkeydown={onKey} />

<section class="ge" data-graph-editor>
  <header class="wb-bar ge-top">
    <select class="ge-wf" aria-label={$t("Workflow source")} title={$t("Workflow source")} bind:value={workflowId} disabled={!workflows.length}>{#each workflows as w}<option value={w.id}>{w.name}</option>{/each}</select>
    <button disabled={!workflowId || busy} onclick={open}>{$t("Ouvrir")}</button>
    <details class="ge-pop" data-graph-sessions>
      <summary class="ge-pop-btn">{$t("Sessions enregistrées ({n})", { n: sessions.length })}</summary>
      <div class="ge-pop-body">
        <select bind:value={resumeId} disabled={!sessions.length || busy} aria-label={$t("Sessions enregistrées ({n})", { n: sessions.length })}>
          <option value="">{$t("— choisir une session —")}</option>
          {#each sessions as s (s.id)}<option value={s.id}>{s.name} · {s.sourceFormat} · {s.updatedAt}</option>{/each}
        </select>
        <div class="ge-row">
          <button data-graph-resume disabled={!resumeId || busy} onclick={resumeSession}>{$t("Reprendre")}</button>
          <button class="quiet" disabled={busy} onclick={refreshSessions}>{$t("Actualiser")}</button>
          <span class="grow"></span>
          {#if sessionDeleteArmed && sessionDeleteArmed === resumeId}
            <button class="danger" data-graph-session-delete disabled={!resumeId || busy} onclick={deleteSession}>{$t("Confirmer la suppression")}</button>
          {:else}
            <button class="quiet icon-btn" data-graph-session-delete disabled={!resumeId || busy} onclick={deleteSession} title={$t("Supprimer")} aria-label={$t("Supprimer")}><Icon name="trash" /></button>
          {/if}
        </div>
      </div>
    </details>
    <span class="ge-sep" aria-hidden="true"></span>
    <button class="quiet icon-btn" data-graph-new disabled={busy || !native} onclick={newWorkflow} title={$t("Nouveau workflow UI")} aria-label={$t("Nouveau workflow UI")}>
      <svg class="ge-ico" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
    </button>
    <button class="quiet icon-btn" data-graph-open-file disabled={busy || !native} onclick={() => openFile()} title={$t("Ouvrir un workflow ComfyUI")} aria-label={$t("Ouvrir un workflow ComfyUI")}><Icon name="folder" /></button>
    <details class="ge-pop">
      <summary class="ge-pop-btn ge-pop-icon" title={$t("Chemin du fichier")} aria-label={$t("Chemin du fichier")}><Icon name="more" /></summary>
      <div class="ge-pop-body">
        <input aria-label={$t("Chemin workflow JSON")} placeholder={$t("Chemin du fichier")} bind:value={filePath} onkeydown={e => { if (e.key === 'Enter' && filePath.trim()) void openFile(false); }} />
        <div class="ge-row"><span class="grow"></span><button data-graph-open-path disabled={busy || !filePath.trim()} onclick={() => openFile(false)}>{$t("Ouvrir ce fichier")}</button></div>
      </div>
    </details>
    {#if view}
      <span class="ge-sep" aria-hidden="true"></span>
      <button class="quiet icon-btn" data-graph-undo disabled={!view.canUndo || busy} onclick={() => edit({ op: 'undo' })} title={$t("Annuler")} aria-label={$t("Annuler")}><span class="ge-flip"><Icon name="redo" /></span></button>
      <button class="quiet icon-btn" data-graph-redo disabled={!view.canRedo || busy} onclick={() => edit({ op: 'redo' })} title={$t("Rétablir")} aria-label={$t("Rétablir")}><Icon name="redo" /></button>
      <span class="grow"></span>
      <label class="ge-export" title={view.exportable ? $t("nom du nouveau workflow") : $t("Export impossible : {detail}.", { detail: view.exportError ?? $t("entrées requises manquantes") })}><span class="ge-sr">{$t("Exporter comme nouveau workflow")}</span><input bind:value={exportName} placeholder={$t("nom du nouveau workflow")} /></label>
      <button data-graph-save-ui disabled={busy || !exportName.trim() || scoped} onclick={saveUi}>{$t("Enregistrer une copie UI")}</button>
      <button class="primary" data-graph-export disabled={busy || !exportName.trim() || !view.exportable || scoped} onclick={exportNew} title={$t("Exporter comme nouveau workflow")}>{$t("Exporter")}</button>
      <button class="quiet" disabled={busy} onclick={closeSession}>{$t("Fermer")}</button>
    {/if}
  </header>

  <div class="ge-body" class:cat={catalogOpen && !!view} class:insp={!!view}>
    {#if catalogOpen && view}
      <aside class="wb-panel left node-catalog" data-node-catalog>
        <div class="wb-head node-catalog-head"><h2>{$t("Catalogue")}</h2><span class="chip num">{catalog.length}</span><button class="quiet icon-btn" title={$t("Fermer")} aria-label={$t("Fermer")} onclick={()=>catalogOpen=false}><Icon name="close" /></button></div>
        <div class="node-catalog-tools">
          <input data-node-search bind:value={catalogSearch} placeholder={$t("Rechercher un node, une catégorie…")} aria-label={$t("Rechercher un node, une catégorie…")} />
          <select bind:value={catalogCategory} aria-label={$t("Catalogue")}>
            <option value="">{$t("Tout")} ({catalog.length})</option>
            {#each catalogGroups as [cat, items] (cat)}<option value={cat}>{cat} ({items.length})</option>{/each}
          </select>
        </div>
        <div class="wb-scroll">
          {#if catalogFavorites.length}<div class="node-catalog-fav"><span class="sec-title">{$t("Favoris")}</span><div>{#each catalogFavorites as cls}<button draggable="true" ondragstart={(e)=>e.dataTransfer?.setData('text/saya-node',cls)} onclick={()=>addClassAt(cls)}>{cls}</button>{/each}</div></div>{/if}
          {#if catalogRecent.length}<div class="node-catalog-fav"><span class="sec-title">{$t("Récents")}</span><div>{#each catalogRecent as cls}<button draggable="true" ondragstart={(e)=>e.dataTransfer?.setData('text/saya-node',cls)} onclick={()=>addClassAt(cls)}>{cls}</button>{/each}</div></div>{/if}
          <div class="node-catalog-results">{#each catalogFiltered as n (n.name)}<div class="node-catalog-item" role="button" tabindex="0" draggable="true" title={n.description || n.name} ondragstart={(e)=>e.dataTransfer?.setData('text/saya-node',n.name)} onclick={()=>addClassAt(n.name)} onkeydown={(e)=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();addClassAt(n.name)}}}><div class="nci-text"><strong>{n.displayName}</strong><small>{n.category} · {$t("{inputs} entrée(s) · {outputs} sortie(s)", { inputs: n.inputs.length, outputs: n.outputs.length })}</small></div><button class="quiet icon-btn" class:fav={catalogFavorites.includes(n.name)} data-node-fav={n.name} onclick={(e)=>{e.stopPropagation();toggleCatalogFavorite(n.name)}}>{catalogFavorites.includes(n.name)?'★':'☆'}</button></div>{:else}<p class="muted nci-empty">{$t("Aucun node pour cette recherche.")}</p>{/each}</div>
        </div>
      </aside>
    {/if}

    <div class="ge-center">
      {#if view}
        <div class="ge-tools">
          <button class:on={catalogOpen} data-graph-catalog-open disabled={busy} onclick={()=>{catalogOpen=!catalogOpen;void loadCatalog()}} title={$t("Catalogue de nodes")}>{$t("Catalogue de nodes")}</button>
          <input class="ge-add" aria-label={$t("Ajouter une classe")} list="graph-classes" bind:value={newClass} placeholder={$t("classe exacte")} onkeydown={e => { if (e.key === 'Enter') void addNode(); }} />
          <datalist id="graph-classes">{#each classes as c}<option value={c}></option>{/each}</datalist>
          <button disabled={!newClass.trim() || busy} onclick={addNode}>{$t("Ajouter")}</button>
          <span class="ge-sep" aria-hidden="true"></span>
          <div class="ge-search">
            <input data-graph-search aria-label={$t("Rechercher un node")} bind:value={search} placeholder={$t("Rechercher un node")} onkeydown={e => { if (e.key === 'Enter' && matches.length) centerNode(matches[0].id); }} />
            {#if matches.length}<div class="graph-matches">{#each matches.slice(0, 8) as m}<button class="quiet" onclick={() => centerNode(m.id)}>{m.classType} <span class="num muted">#{nodeKey(m.id, originalIds)}</span></button>{/each}{#if matches.length > 8}<small class="muted num">+{matches.length - 8}</small>{/if}</div>{/if}
          </div>
          <span class="ge-sep" aria-hidden="true"></span>
          <button class="quiet" disabled={busy} onclick={() => edit({ op: 'layout' }).then(ok => ok && queueMicrotask(fit))}>{$t("Organiser")}</button>
          <button class="quiet" onclick={fit}>{$t("Ajuster")}</button>
          <button class="quiet icon-btn" onclick={() => { zoom = clampZoom(zoom / 1.2); }} aria-label={$t("Zoom arrière")} title={$t("Zoom arrière")}>−</button>
          <span class="num muted ge-zoom">{Math.round(zoom * 100)}%</span>
          <button class="quiet icon-btn" onclick={() => { zoom = clampZoom(zoom * 1.2); }} aria-label={$t("Zoom avant")} title={$t("Zoom avant")}>+</button>
          <label class="checkbox ge-wires"><input type="checkbox" checked={showEdges} onchange={(e)=>{showEdges=e.currentTarget.checked;localStorage.setItem('saya.showEdges',String(showEdges))}} /> {$t("Afficher les fils")}</label>
        </div>
        {#if view.scope || selectedContainer}
          <div class="ge-scope">
            {#if view.scope}<span data-graph-scope class="graph-scope" title={$t("Subgraph « {name} » ouvert : positions et paramètres éditables, liens de frontière conservés.", { name: view.scope.name })}>{$t("Subgraph : {name}", { name: view.scope.name })}</span>{/if}
            {#if selectedContainer}
              <button data-graph-enter-subgraph disabled={busy} onclick={enterSubgraph}>{$t("Entrer dans le subgraph")}</button>
              {#if !view.scope}<span class="muted" data-subgraph-summary>{selectedContainer.title} · {$t("{inputs} entrée(s) promue(s) · {outputs} sortie(s)", { inputs: selectedContainer.inputs.length, outputs: selectedContainer.outputs.length })}</span>{/if}
            {/if}
            {#if view.scope}
              <button data-graph-exit-subgraph disabled={busy} onclick={() => exitSubgraph(true)}>{$t("Sortir (enregistrer)")}</button>
              <button class="quiet" data-graph-exit-cancel disabled={busy} onclick={() => exitSubgraph(false)}>{$t("Annuler")}</button>
            {/if}
          </div>
        {/if}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="graph-canvas" bind:this={canvas} use:wheel onpointerdown={bgDown} role="application" aria-label={$t("Canevas du graphe")} ondragover={(e)=>e.preventDefault()} ondrop={dropNode} ondblclick={()=>{catalogOpen=true;void loadCatalog()}}>
          <div class="graph-world" style="transform:translate({panX}px,{panY}px) scale({zoom})">
            {#each scene.frames as f (f.id + f.kind)}
              <div class="graph-frame" class:graph-subgraph={f.kind === 'subgraph'} class:graph-group-sel={f.kind === 'group' && selectedGroupId === f.id} data-graph-frame={f.kind} data-graph-group={f.kind === 'group' ? f.id : undefined} style="left:{f.x}px;top:{f.y}px;width:{f.w}px;height:{f.h}px;{f.color ? `border-color:${f.color};` : ''}" onpointerdown={f.kind === 'group' ? (e) => groupDown(e, f.id) : f.kind === 'subgraph' ? (e) => subgraphDown(e, f) : undefined}>
                <span>{f.title}</span>
                {#if f.kind === 'subgraph'}<button class="graph-frame-open" data-graph-open-frame={f.id} onpointerdown={(e)=>e.stopPropagation()} onclick={()=>enterSubgraphFrame(f.id)}>{$t("Ouvrir")}</button>{/if}
                {#if f.kind === 'group'}<span class="group-resize" data-group-resize={f.id} onpointerdown={(e) => groupResizeDown(e, f.id)}></span>{/if}
              </div>
            {/each}
            <svg class="graph-edges" width="1" height="1" style="overflow:visible">
              {#if showEdges}{#each scene.edges as e (e.id)}
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <g data-graph-edge={e.id} class:graph-edge-sel={selectedEdge === e.id} onpointerdown={ev => { ev.stopPropagation(); selectedEdge = e.id; selected = []; }}>
                  <path d={e.d} stroke="transparent" stroke-width="12" fill="none" />
                  <path d={e.d} stroke={e.color} stroke-width={selectedEdge === e.id ? 4 : 2} fill="none" />
                </g>
              {/each}
              {#if link}
                <path d={`M${link.from === 'out' ? link.ax : link.x},${link.from === 'out' ? link.ay : link.y} L${link.from === 'out' ? link.x : link.ax},${link.from === 'out' ? link.y : link.ay}`} stroke={link.color} stroke-width="2" stroke-dasharray="5 4" fill="none" />
              {/if}{/if}
            </svg>
            {#each scene.nodes as n (n.id)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="graph-node" class:graph-sel={selected.includes(n.id)} class:graph-locked={n.locked} class:graph-missing={n.missing}
                data-graph-node={nodeKey(n.id, originalIds)} style="left:{n.x}px;top:{n.y}px;width:{n.w}px;height:{n.h}px" onpointerdown={e => nodeDown(e, n.id)}>
                <div class="graph-head" style="height:{HEAD_H}px;{n.color ? `background:${n.color};color:#101418;` : ''}" title={n.label}>{n.label}</div>
                {#each n.inputs as p}
                  <span class="graph-port graph-in" class:graph-linked={p.linked} data-port="in" data-node={n.id} data-index={p.index} data-name={p.name} title="{p.type}{p.optional ? ' (optionnel)' : ''}"
                    style="top:{HEAD_H + p.index * ROW_H}px;height:{ROW_H}px"
                    onpointerdown={e => !n.locked && portDown(e, 'in', n.id, p.index, p.name, p.color, p.x, p.y)}><i style="background:{p.color}"></i>{p.name}<small>{p.type}</small></span>
                {/each}
                {#each n.outputs as p}
                  <span class="graph-port graph-out" data-port="out" data-node={n.id} data-index={p.index} data-name={p.name} title={p.type}
                    style="top:{HEAD_H + p.index * ROW_H}px;height:{ROW_H}px"
                    onpointerdown={e => !n.locked && portDown(e, 'out', n.id, p.index, p.name, p.color, p.x, p.y)}><small>{p.type}</small>{p.name}<i style="background:{p.color}"></i></span>
                {/each}
              </div>
            {/each}
            {#if rect}<div class="graph-rect" style="left:{Math.min(rect.x0, rect.x1)}px;top:{Math.min(rect.y0, rect.y1)}px;width:{Math.abs(rect.x1 - rect.x0)}px;height:{Math.abs(rect.y1 - rect.y0)}px"></div>{/if}
          </div>
        </div>
      {:else}
        <div class="ge-empty">
          {#if !native}<p class="notice" role="status">{$t("Aperçu navigateur : ouvrir le binaire Saya Studio.")}</p>{/if}
          <p>{$t("Ouvrir un workflow ou reprendre une session")}</p>
          <div class="ge-row">
            <button disabled={busy || !native} onclick={newWorkflow}>{$t("Nouveau workflow UI")}</button>
            <button disabled={busy || !native} onclick={() => openFile()}>{$t("Ouvrir un workflow ComfyUI")}</button>
          </div>
        </div>
      {/if}
      <footer class="ge-foot">
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        {#if note}<p class="notice" role="status">{note}</p>{/if}
        {#if view}
          <div class="ge-status">
            <span class="chip" title={view.scope ? $t("Subgraph « {name} » ouvert : positions et paramètres éditables, liens de frontière conservés.", { name: view.scope.name }) : view.sourceFormat === 'comfy_ui' ? $t("Document UI conservé. Déplacements et widgets reconnus modifiables ; structure, subgraphs et champs inconnus préservés dans la copie UI.") : $t("Source API : positions reconstruites, données visuelles originales non disponibles.")}>{view.sourceFormat === 'comfy_ui' ? 'UI' : 'API'}</span>
            <span class="graph-status num" data-graph-status class:graph-bad={!view.exportable}>
              {$t("{nodes} nodes · {links} liens · {selected} sélectionné(s) · {clipboard} ·", { nodes: view.document.nodes.length, links: view.document.connections.length, selected: selected.length, clipboard: clipboard ? $t("presse-papiers {n}", { n: clipboard.nodes.length }) : $t("presse-papiers vide") })}
              {#if view.exportable}{$t("exportable")}{:else}{$t("non exportable")}{#if view.exportError} : {view.exportError}{/if}{/if}
              {#if view.missing.length} · {$t("entrées manquantes :")} {view.missing.slice(0, 6).map(m => `${nodeKey(m.node, originalIds)}.${m.input}`).join(', ')}{view.missing.length > 6 ? ` (+${view.missing.length - 6})` : ''}{/if}
            </span>
          </div>
        {/if}
      </footer>
    </div>

    {#if view}
      <aside class="wb-panel right graph-props" data-graph-props>
        <div class="wb-head">
          {#if single}<h3 title={single.label}>{single.label}</h3><span class="chip num">#{nodeKey(single.id, originalIds)}</span>
          {:else}<h3>{$t("Sélection")}</h3><span class="chip num">{selected.length}</span>{/if}
        </div>
        <div class="wb-scroll">
          {#if single}
            <p class="ge-meta muted">{$t("id interne {id} · classe {classType}", { id: single.id, classType: single.classType })}</p>
            {#if single.locked}<p class="ge-meta muted">{$t("Node d’un subgraph : boîte opaque non éditable.")}</p>{/if}
            <details class="sec" open>
              <summary><span class="sec-title">{$t("Paramètres (texte exact)")}</span></summary>
              <div class="sec-body">
                {#each paramNames(single.id) as name}
                  {@const linked = single.inputs.find(i => i.name === name)?.linked}
                  {#if !linked}
                    <div class="ge-param">
                      <label>{name}
                        {#if single.inputs.find(p => p.name === name)?.type === 'STRING'}
                          <textarea data-graph-param={name} disabled={single.locked} rows="4" value={drafts[dkey(single.id, name)] ?? view.paramText[single.id]?.[name] ?? ''} oninput={e => { drafts[dkey(single.id, name)] = e.currentTarget.value; }}></textarea>
                        {:else}
                        <input data-graph-param={name} disabled={single.locked} value={drafts[dkey(single.id, name)] ?? view.paramText[single.id]?.[name] ?? ''} oninput={e => { drafts[dkey(single.id, name)] = e.currentTarget.value; }} onkeydown={e => { if (e.key === 'Enter') void validate(single.id, name); }} />
                        {/if}
                      </label>
                      {#if !single.locked}<div class="ge-row ge-param-actions"><button data-graph-apply={name} disabled={busy} onclick={() => validate(single.id, name)}>{$t("Valider")}</button><button class="quiet" disabled={busy || view.paramText[single.id]?.[name] === undefined} onclick={() => removeParam(single.id, name)}>{$t("Retirer")}</button></div>{/if}
                    </div>
                  {/if}
                {:else}<p class="muted">{$t("Aucun paramètre.")}</p>{/each}
              </div>
            </details>
            {#if !single.locked}
              <details class="sec">
                <summary><span class="sec-title">{$t("Node")}</span></summary>
                <div class="sec-body">
                  <label class="field-row"><span>{$t("Nom du node")}</span><input data-graph-node-title value={single.title} onchange={(e) => edit({ op: 'style_node', node: single.id, title: e.currentTarget.value, color: single.color ?? null })} /></label>
                  <div class="field-row"><span>{$t("Couleur du node")}</span><div class="ge-row"><input class="ge-color" data-graph-node-color type="color" aria-label={$t("Couleur du node")} value={single.color ?? '#6fb1d9'} onchange={(e) => edit({ op: 'style_node', node: single.id, title: single.title, color: e.currentTarget.value })} /><button class="quiet" data-graph-node-color-clear disabled={busy || !single.color} onclick={() => edit({ op: 'style_node', node: single.id, title: single.title, color: null })}>{$t("Effacer la couleur")}</button></div></div>
                </div>
              </details>
            {/if}
            <details class="sec">
              <summary><span class="sec-title">{$t("Ports")}</span><span class="chip num">{single.inputs.length + single.outputs.length}</span></summary>
              <div class="sec-body">
                <ul class="graph-ports">
                  {#each single.inputs as p}<li><i style="background:{p.color}"></i>{$t("entrée")} <b>{p.name}</b> <small>{p.type}{p.optional ? ' · ' + $t("opt.") : ''}{p.linked ? ' · ' + $t("lié") : ''}</small></li>{/each}
                  {#each single.outputs as p}<li><i style="background:{p.color}"></i>{$t("sortie")} <b>{p.name}</b> <small>{p.type}</small></li>{/each}
                </ul>
              </div>
            </details>
            {#if nodeConnections(single.id).length}
              <details class="sec">
                <summary><span class="sec-title">{$t("Liens")}</span><span class="chip num">{nodeConnections(single.id).length}</span></summary>
                <div class="sec-body">
                  <ul class="graph-ports">{#each nodeConnections(single.id) as c}<li><span class="num">{nodeKey(c.from.node, originalIds)}[{c.from.output}] → {nodeKey(c.to.node, originalIds)}.{c.to.input}</span> {#if !single.locked}<button class="quiet" disabled={busy} onclick={() => edit({ op: 'disconnect', connection: c.id })}>{$t("Retirer")}</button>{/if}</li>{/each}</ul>
                </div>
              </details>
            {/if}
          {:else if selected.length > 1}
            <p class="ge-meta muted">{$t("{n} nodes sélectionnés. Glisser pour déplacer ensemble, « Grouper » pour les cadrer.", { n: selected.length })}</p>
          {:else}
            <p class="ge-meta muted">{$t("Sélectionner un node (Maj : sélection multiple ou rectangle). Glisser un port vers un autre pour relier. Suppr : supprimer.")}</p>
          {/if}

          <details class="sec" open>
            <summary><span class="sec-title">{$t("Sélection")}</span></summary>
            <div class="sec-body">
              <div class="ge-grid">
                <button data-graph-copy disabled={!selected.length || busy} onclick={copySelection}>{$t("Copier")}</button>
                <button data-graph-paste disabled={!clipboard || busy} onclick={pasteClipboard}>{$t("Coller")}</button>
                <button data-graph-duplicate disabled={!movableSelection.length || busy} onclick={duplicateSelection}>{$t("Dupliquer")}</button>
                <button class="danger" disabled={(!movableSelection.length && !selectedEdge) || busy} onclick={removeSelection}>{$t("Supprimer")}</button>
              </div>
              <label class="field-row"><span>{$t("Titre du groupe")}</span><input bind:value={groupTitle} /></label>
              <div class="ge-grid">
                <button disabled={!movableSelection.length || busy || scoped} onclick={makeGroup}>{$t("Grouper la sélection")}</button>
                <button disabled={!selectedGroup || busy || scoped} onclick={() => selectedGroup && edit({ op: 'ungroup', group: selectedGroup.id })}>{$t("Dissoudre le groupe")}</button>
              </div>
              {#if selectedGroup}
                <label class="field-row"><span>{$t("Nom du groupe sélectionné")}</span><input data-graph-group-title value={selectedGroup.title} onchange={(e) => edit({ op: 'style_group', group: selectedGroup.id, title: e.currentTarget.value, color: selectedGroup.color ?? null })} /></label>
                <div class="field-row"><span>{$t("Couleur du groupe")}</span><div class="ge-row"><input class="ge-color" data-graph-group-color type="color" aria-label={$t("Couleur du groupe")} value={selectedGroup.color ?? '#e0a458'} onchange={(e) => edit({ op: 'style_group', group: selectedGroup.id, title: selectedGroup.title, color: e.currentTarget.value })} /><button class="quiet" data-graph-group-color-clear disabled={busy || !selectedGroup.color} onclick={() => edit({ op: 'style_group', group: selectedGroup.id, title: selectedGroup.title, color: null })}>{$t("Effacer la couleur")}</button></div></div>
              {/if}
              <button data-graph-module-open disabled={!movableSelection.length || busy || scoped} onclick={openModuleForm}>{$t("Créer un module")}</button>
            </div>
          </details>

          {#if moduleFormOpen}
            <details class="sec" open data-graph-module>
              <summary title={$t("{n} node(s) sélectionné(s). Les entrées/sorties du module sont déduites des liens traversants : une entrée par lien venant de l'extérieur, une sortie par lien allant vers l'extérieur.", { n: movableSelection.length })}><span class="sec-title">{$t("Créer un module depuis la sélection")}</span><span class="chip num">{movableSelection.length}</span></summary>
              <div class="sec-body">
                <label>{$t("Nom du module")}<input data-graph-module-name maxlength="300" bind:value={moduleName} placeholder={$t("ex. Chaîne sampling")} /></label>
                <label>{$t("Description")}<input data-graph-module-description maxlength="4000" bind:value={moduleDescription} /></label>
                {#if modulesList.length}
                  <label>{$t("Ajouter comme version de")}<select data-graph-module-target bind:value={moduleTarget}>
                    <option value="">{$t("Nouveau module")}</option>
                    {#each modulesList as m}<option value={m.id}>{m.name}</option>{/each}
                  </select></label>
                {/if}
                <button class="primary" data-graph-module-save disabled={busy || !movableSelection.length || !moduleName.trim()} onclick={extractModule}>{$t("Extraire et enregistrer")}</button>
                {#if moduleResult}
                  <div data-module-created={moduleResult.id} class="ge-result" title={$t("Réutilisation, composition et insertion : page Modules. Cette session reste inchangée.")}>
                    <p><strong>{$t("Module créé : version {version} · {n} nodes conservés à l'identique.", { version: moduleResult.version, n: moduleResult.nodeCount })}</strong></p>
                    <p data-module-created-ports class="muted">{$t("Entrées :")} {moduleResult.inputs.map(p => `${p.name} (${p.kind})`).join(' · ') || $t("aucune")}<br />{$t("Sorties :")} {moduleResult.outputs.map(p => `${p.name} (${p.kind})`).join(' · ') || $t("aucune")}</p>
                  </div>
                {/if}
                {#if scoped}<p class="muted">{$t("Vue subgraph : fermez le subgraph pour extraire un module (frontières opaques ici).")}</p>{/if}
              </div>
            </details>
          {/if}

          <details class="sec" open={!!apiPreview}>
            <summary title={$t("Convertit le graphe UI courant (subgraphs et widgets promus compris) en prompt API, sans modifier la source et sans soumission.")}><span class="sec-title">{$t("Préparation API")}</span></summary>
            <div class="sec-body">
              <button data-graph-prepare-api disabled={busy || session === null || view.sourceFormat !== 'comfy_ui' || scoped} onclick={prepareApi} title={view.sourceFormat !== 'comfy_ui' ? $t("Disponible pour un workflow UI ; une révision API est déjà exécutable.") : ''}>{$t("Préparer en API")}</button>
              {#if apiPreview}
                <section data-graph-api class="graph-api">
                  <p data-graph-api-status class:graph-bad={!apiReady}>
                    {apiReady ? $t("Convertible : conversion complète et schéma vérifié.") : apiPreview.report.complete ? $t("Conversion complète, mais schéma non vérifié : création bloquée.") : $t("Conversion incomplète : création bloquée.")}
                  </p>
                  <p class="muted">{$t("Schéma")} <code>{$t("object_info")}</code> : {apiPreview.report.validatedAgainstSchema ? $t("vérifié") : $t("non vérifié")} · {$t("{nodes} nodes API · {defs} subgraphs ({top} instances racine, {nested} imbriquées) · {promoted} widgets promus · {hidden} widgets cachés exclus.", { nodes: apiPreview.report.apiNodes, defs: apiPreview.report.subgraphDefinitions, top: apiPreview.report.topLevelInstances, nested: apiPreview.report.nestedInstances, promoted: apiPreview.report.promotedWidgetsResolved, hidden: apiPreview.report.hiddenWidgetsExcluded })}</p>
                  <details class="ge-sub"><summary>{$t("Nodes convertis ({n})", { n: apiNodes.length })}</summary><ul>{#each apiNodes as [id, node]}<li><code>{id}</code> · {node.class_type}</li>{/each}</ul></details>
                  {#if apiPreview.report.unresolved.length}
                    <div class="error" role="alert"><strong>{$t("Éléments non résolus")}</strong><ul>{#each apiPreview.report.unresolved as u}<li><code>{u.executionId}</code> · {u.classType} : {u.cause}</li>{/each}</ul></div>
                  {/if}
                  {#if apiPreview.report.unverifiedClasses.length}<p class="error">{$t("Classes hors schéma : {classes}.", { classes: apiPreview.report.unverifiedClasses.join(', ') })}</p>{/if}
                  <label>{$t("Nom de la révision API")}<input data-graph-api-name bind:value={apiName} disabled={busy || !apiReady} /></label>
                  <label class="checkbox" title={$t("La source UI n’est pas modifiée. La révision créée reste soumise aux confirmations existantes du runner.")}><input type="checkbox" data-graph-api-confirm bind:checked={apiConfirmed} disabled={busy || !apiReady} />{$t("Je confirme créer une nouvelle révision API à partir de ce graphe. Aucune soumission n’est effectuée.")}</label>
                  <button class="primary" data-graph-api-save disabled={busy || !apiReady || !apiConfirmed || !apiName.trim()} onclick={saveApi}>{$t("Créer la révision API")}</button>
                </section>
              {/if}
            </div>
          </details>

          {#if view.diagnostics?.length}
            <details class="sec" data-conversion-diagnostics open>
              <summary><span class="sec-title">{$t("Incompatibilités de conversion ({n})", { n: view.diagnostics.length })}</span></summary>
              <div class="sec-body"><ul class="ge-list">{#each view.diagnostics as d}<li>{d}</li>{/each}</ul></div>
            </details>
          {/if}
          {#if view.sourceDetails}
            <details class="sec">
              <summary><span class="sec-title">{$t("Source complète : widgets, subgraphs et métadonnées visuelles")}</span></summary>
              <div class="sec-body"><pre class="ge-pre">{view.sourceDetails}</pre></div>
            </details>
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</section>

<style>
  .ge{display:grid;grid-template-rows:auto minmax(0,1fr);height:100%;min-height:0;background:var(--bg)}
  .ge-top{gap:4px;flex-wrap:wrap;padding-block:4px}
  .ge-wf{width:220px;min-width:120px;flex:0 1 220px}
  .ge-sep{width:1px;height:18px;background:var(--line-strong);margin:0 4px;flex:none}
  .ge-flip{display:inline-grid;transform:scaleX(-1)}
  .ge-ico{stroke:currentColor;fill:none;stroke-width:1.6;stroke-linecap:round;display:block}
  .ge-export{display:flex;flex-direction:row;align-items:center;gap:0;margin:0;min-width:0;flex:0 1 220px}
  .ge-export input{width:100%;min-width:90px}
  .ge-sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
  .ge-row{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
  .grow{flex:1}

  /* Popovers of the top bar (sessions, file path). */
  .ge-pop{position:relative;margin:0}
  .ge-pop>summary{list-style:none}
  .ge-pop>summary::-webkit-details-marker{display:none}
  .ge-pop-btn{display:inline-flex;align-items:center;gap:6px;min-height:26px;padding:0 10px;border:1px solid var(--line-strong);border-radius:var(--radius);background:var(--raised);font-size:12px;white-space:nowrap;cursor:pointer}
  .ge-pop-btn::after{content:"▾";color:var(--faint);font-size:10px}
  .ge-pop-icon{width:28px;padding:0;justify-content:center;background:transparent;border-color:transparent;color:var(--muted)}
  .ge-pop-icon::after{content:none}
  .ge-pop[open]>.ge-pop-btn{background:var(--hover);color:var(--text)}
  .ge-pop-body{position:absolute;z-index:50;top:calc(100% + 4px);inset-inline-start:0;width:340px;display:grid;gap:8px;padding:10px;background:var(--panel);border:1px solid var(--line-strong);border-radius:6px;box-shadow:0 8px 24px #0008}
  .ge-pop-body select,.ge-pop-body input{width:100%}

  .ge-body{display:grid;grid-template-columns:minmax(0,1fr);min-height:0}
  .ge-body.insp{grid-template-columns:minmax(0,1fr) 300px}
  .ge-body.cat{grid-template-columns:260px minmax(0,1fr)}
  .ge-body.cat.insp{grid-template-columns:260px minmax(0,1fr) 300px}

  /* Node catalog (left, collapsible). */
  .node-catalog-head h2{font-size:13px}
  .node-catalog-tools{display:grid;gap:6px;padding:8px 12px;border-bottom:1px solid var(--line)}
  .node-catalog-tools input,.node-catalog-tools select{width:100%}
  .node-catalog-fav{padding:8px 12px 4px;display:grid;gap:4px}
  .node-catalog-fav div{display:flex;flex-wrap:wrap;gap:4px}
  .node-catalog-fav button{min-height:22px;font-size:11px;padding:0 6px;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .node-catalog-results{display:grid;padding:4px 0 8px}
  .node-catalog-item{display:flex;align-items:center;gap:6px;padding:4px 6px 4px 12px;border-bottom:1px solid var(--line);cursor:grab;min-width:0}
  .node-catalog-item:hover{background:var(--raised)}
  .nci-text{display:grid;min-width:0;flex:1}
  .nci-text strong{font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .nci-text small{color:var(--muted);font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .node-catalog-item .icon-btn{min-height:24px;width:24px;flex:none}
  .node-catalog-item .fav{color:var(--accent-warm)}
  .nci-empty{padding:12px}

  /* Center: canvas + floating tools + status strip. */
  .ge-center{position:relative;display:flex;flex-direction:column;min-width:0;min-height:0;background:var(--canvas)}
  .ge-tools{position:absolute;z-index:20;top:8px;left:8px;right:8px;display:flex;align-items:center;flex-wrap:wrap;gap:4px;pointer-events:none}
  .ge-tools>*{pointer-events:auto}
  .ge-tools button,.ge-tools input{min-height:26px;font-size:12px}
  .ge-tools>button:not(.quiet),.ge-tools>input,.ge-search input{background:var(--panel)}
  .ge-tools>.quiet,.ge-zoom,.ge-wires{background:var(--panel)}
  .ge-tools button.on{background:var(--hover);color:var(--text);border-color:var(--sel)}
  .ge-add{width:150px}
  .ge-zoom{min-width:44px;text-align:center;line-height:26px;border-radius:var(--radius);font-size:11.5px}
  .ge-wires{flex-direction:row;align-items:center;gap:6px;min-height:26px;padding:0 8px;border-radius:var(--radius);font-size:12px;color:var(--muted)}
  .ge-search{position:relative}
  .ge-search input{width:180px}
  .graph-matches{position:absolute;top:calc(100% + 4px);inset-inline-start:0;min-width:220px;display:grid;padding:4px;background:var(--panel);border:1px solid var(--line-strong);border-radius:6px;box-shadow:0 8px 24px #0008}
  .graph-matches button{justify-content:flex-start;text-align:start;min-height:24px;font-size:12px}
  .graph-matches small{padding:2px 8px}
  .ge-scope{position:absolute;z-index:20;top:44px;left:8px;display:flex;align-items:center;gap:6px;padding:4px 6px;background:var(--panel);border:1px solid var(--line-strong);border-radius:var(--radius);font-size:12px}
  .graph-scope{color:var(--accent);font-weight:600;padding:0 4px}
  .ge-scope button{min-height:24px;font-size:12px}
  .ge-empty{flex:1;display:grid;place-content:center;justify-items:center;gap:10px;color:var(--muted);padding:24px;text-align:center}
  .ge-empty p{margin:0}
  .ge-foot{flex:none;display:grid;border-top:1px solid var(--line);background:var(--bg)}
  .ge-foot:empty{display:none}
  .ge-foot p{margin:0;padding:4px 12px;font-size:12px;border-bottom:1px solid var(--line);overflow-wrap:anywhere;white-space:pre-wrap;background:transparent}
  .ge-foot .notice{color:var(--text)}
  .ge-status{display:flex;align-items:center;gap:8px;min-height:26px;padding:0 12px;font-size:11.5px;color:var(--muted);min-width:0}
  .graph-status{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}
  .graph-bad{color:var(--warning)}

  /* Canvas: world coordinates stay physical (left/top) on purpose. */
  .graph-canvas{position:relative;flex:1;min-height:0;overflow:hidden;background-color:var(--canvas);background-image:radial-gradient(var(--line) 1px,transparent 1px);background-size:24px 24px;cursor:grab;touch-action:none;user-select:none}
  .graph-world{position:absolute;left:0;top:0;transform-origin:0 0}
  .graph-edges{position:absolute;left:0;top:0;pointer-events:none}
  .graph-edges g{pointer-events:stroke;cursor:pointer}
  .graph-frame{position:absolute;border:1px solid var(--line-strong);border-radius:6px;background:#ffffff06}
  .graph-frame span{position:absolute;left:8px;top:-20px;font-size:12px;color:var(--muted);white-space:nowrap}
  .graph-frame.graph-subgraph{border-style:dashed;border-color:var(--faint);background:#ffffff04}
  .graph-frame.graph-subgraph span{color:var(--text)}
  .graph-frame.graph-group-sel{border-color:var(--accent)}
  .graph-frame-open{position:absolute;right:6px;top:-24px;min-height:20px;font-size:11px;padding:0 6px}
  .group-resize{position:absolute;right:-5px;bottom:-5px;width:10px;height:10px;border-radius:2px;background:var(--accent);cursor:nwse-resize}
  .graph-frame span.group-resize{left:auto;top:auto}
  .graph-node{position:absolute;background:var(--panel);border:1px solid var(--line-strong);border-radius:6px;font-size:12px;cursor:move}
  .graph-node.graph-sel{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent)}
  .graph-node.graph-locked{opacity:.65;cursor:default;border-style:dashed}
  .graph-node.graph-missing{border-color:var(--warning)}
  .graph-head{padding:5px 10px;background:var(--raised);color:var(--text);border-radius:5px 5px 0 0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-weight:600;border-bottom:1px solid var(--line)}
  .graph-port{position:absolute;display:flex;align-items:center;gap:5px;line-height:20px;cursor:crosshair;white-space:nowrap;color:var(--text)}
  .graph-port small{color:var(--faint);font-size:10px}
  .graph-port i,.graph-ports i{display:inline-block;width:9px;height:9px;border-radius:50%;border:1px solid #0008;flex:none}
  .graph-in{left:-5px;padding-right:8px}
  .graph-out{right:-5px;padding-left:8px;justify-content:flex-end}
  .graph-in.graph-linked small{color:var(--muted)}
  .graph-port:hover i{box-shadow:0 0 0 2px var(--sel)}
  .graph-rect{position:absolute;border:1px dashed var(--sel);background:#cfd3dc12;pointer-events:none}

  /* Inspector (right). */
  .graph-props .wb-head h3{font-size:13px;font-weight:600;margin:0;flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .ge-meta{margin:0;padding:8px 12px;font-size:12px;border-bottom:1px solid var(--line);overflow-wrap:anywhere}
  .ge-param{display:grid;gap:4px}
  .ge-param label{gap:3px;color:var(--muted);font-size:12px}
  .ge-param textarea{min-height:72px;color:var(--text)}
  .ge-param-actions button{min-height:24px;font-size:12px}
  .ge-grid{display:grid;grid-template-columns:1fr 1fr;gap:6px}
  .ge-color{width:40px;padding:2px;min-height:26px}
  .graph-props .field-row{grid-template-columns:96px minmax(0,1fr);flex-direction:row;font-size:12.5px;color:var(--text)}
  .graph-ports{list-style:none;padding:0;margin:0;font-size:12px;display:grid;gap:3px}
  .graph-ports li{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
  .graph-ports small{color:var(--muted)}
  .graph-ports button{min-height:22px;font-size:11px;padding:0 6px}
  .graph-api{display:grid;gap:8px;font-size:12px}
  .graph-api p{margin:0;overflow-wrap:anywhere}
  .graph-api .checkbox{flex-direction:row;align-items:flex-start;color:var(--text);font-size:12px}
  .graph-api label{color:var(--muted);font-size:12px}
  .ge-sub summary{color:var(--muted);font-size:12px}
  .ge-sub ul,.ge-list{margin:4px 0 0;padding-inline-start:16px;font-size:12px;overflow-wrap:anywhere}
  .ge-sub{margin:0}
  .ge-result p{margin:0 0 4px;font-size:12px;overflow-wrap:anywhere}
  .ge-pre{max-height:300px;margin:0;font-size:11px}
  .graph-props .sec-body>label:not(.field-row){gap:3px;color:var(--muted);font-size:12px}
  @media(max-width:1100px){
    .ge-body.insp{grid-template-columns:minmax(0,1fr) 260px}
    .ge-body.cat.insp{grid-template-columns:220px minmax(0,1fr) 260px}
    .ge-body.cat{grid-template-columns:220px minmax(0,1fr)}
    .ge-top{flex-wrap:wrap;padding-block:4px}
  }
</style>
