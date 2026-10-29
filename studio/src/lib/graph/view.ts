// Pure logic of the graph editor: Rust view -> scene (nodes, links, frames).
// No numeric value is read from document.params: only paramText is authoritative.
import type { ConnectionV1, EditorDocumentV1, EditorGroupV1, EditorNodeV1, SubgraphContainerV1 } from './editor';

export interface GraphView {
  workflow: string;
  sourceFormat?: string;
  sourceDetails?: string | null;
  diagnostics?: string[];
  document: EditorDocumentV1;
  paramText: Record<string, Record<string, string>>;
  originalIds: Record<string, string>;
  canUndo: boolean;
  canRedo: boolean;
  missing: { node: string; input: string }[];
  exportable: boolean;
  exportError: string | null;
  scope?: { id: string; name: string } | null;
}

export const NODE_W = 230;
export const HEAD_H = 30;
export const ROW_H = 20;
export const PAD_B = 8;
export const GROUP_PAD = 28;

export interface ScenePort { name: string; type: string; color: string; index: number; x: number; y: number; optional: boolean; linked: boolean }
export interface SceneNode {
  id: string; label: string; title: string; classType: string; color: string | null; x: number; y: number; w: number; h: number;
  inputs: ScenePort[]; outputs: ScenePort[]; locked: boolean; missing: boolean;
}
export interface SceneEdge { id: string; d: string; color: string; from: string; to: string }
export interface SceneFrame { id: string; title: string; color: string | null; x: number; y: number; w: number; h: number; kind: 'group' | 'subgraph'; nodeIds: string[] }
export interface Scene { nodes: SceneNode[]; edges: SceneEdge[]; frames: SceneFrame[] }

const PALETTE = ['#e0a458', '#6fb1d9', '#9ccf7b', '#d98fc4', '#c9c26a', '#8f9ee0', '#e07f6f', '#6fd1c4'];
const KNOWN: Record<string, string> = {
  MODEL: '#b39ddb', CLIP: '#f0d060', VAE: '#e57373', CONDITIONING: '#ffa726', LATENT: '#ff80ab',
  IMAGE: '#64b5f6', MASK: '#81c784', INT: '#7a9e9f', FLOAT: '#7a9e9f', STRING: '#a0a0a0', '*': '#888',
};

export function typeColor(type: string): string {
  if (KNOWN[type]) return KNOWN[type];
  let h = 0;
  for (let i = 0; i < type.length; i++) h = (h * 31 + type.charCodeAt(i)) >>> 0;
  return PALETTE[h % PALETTE.length];
}

/** Same rule as the Rust core: wildcard, or an "A,B" list containing the given type. */
export function typesCompatible(expected: string, provided: string): boolean {
  return expected === '*' || provided === '*' || expected.split(',').some(t => t.trim() === provided);
}

export function nodeLabel(node: EditorNodeV1, originalIds: Record<string, string>): string {
  return node.title?.trim() || `${node.classType} #${originalIds[node.id] ?? node.id}`;
}

/** Value of data-graph-node: original API id, otherwise internal id. */
export function nodeKey(id: string, originalIds: Record<string, string>): string {
  return originalIds[id] ?? id;
}

export function nodeHeight(node: EditorNodeV1): number {
  return HEAD_H + Math.max(node.inputs.length, node.outputs.length, 1) * ROW_H + PAD_B;
}

/** Fallback positions (grid) for nodes without a position. */
export function fallbackPosition(index: number): { x: number; y: number } {
  return { x: 40 + (index % 5) * (NODE_W + 50), y: 40 + Math.floor(index / 5) * 220 };
}

export function inputAnchor(n: { x: number; y: number }, index: number): { x: number; y: number } {
  return { x: n.x, y: n.y + HEAD_H + index * ROW_H + ROW_H / 2 };
}
export function outputAnchor(n: { x: number; y: number }, index: number): { x: number; y: number } {
  return { x: n.x + NODE_W, y: n.y + HEAD_H + index * ROW_H + ROW_H / 2 };
}

export function edgePath(a: { x: number; y: number }, b: { x: number; y: number }): string {
  const dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
  return `M${a.x},${a.y} C${a.x + dx},${a.y} ${b.x - dx},${b.y} ${b.x},${b.y}`;
}

export function subgraphMembers(doc: EditorDocumentV1): Set<string> {
  const s = new Set<string>();
  for (const sg of doc.subgraphs ?? []) for (const id of sg.nodeIds) s.add(id);
  return s;
}

function bounds(nodes: SceneNode[], ids: string[], pad: number) {
  const set = new Set(ids);
  const m = nodes.filter(n => set.has(n.id));
  if (!m.length) return null;
  const x0 = Math.min(...m.map(n => n.x)) - pad, y0 = Math.min(...m.map(n => n.y)) - pad - 20;
  const x1 = Math.max(...m.map(n => n.x + n.w)) + pad, y1 = Math.max(...m.map(n => n.y + n.h)) + pad;
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** offsets: transient local displacement (drag in progress), never persisted. */
export function buildScene(view: GraphView, offsets: Record<string, { x: number; y: number }> = {}): Scene {
  const doc = view.document;
  const locked = subgraphMembers(doc);
  const linkedIn = new Set(doc.connections.map(c => `${c.to.node}\u0000${c.to.input}`));
  const missing = new Set(view.missing.map(m => m.node));
  const nodes: SceneNode[] = doc.nodes.map((n, i) => {
    const p = n.position ?? fallbackPosition(i);
    const o = offsets[n.id];
    const x = p.x + (o?.x ?? 0), y = p.y + (o?.y ?? 0);
    return {
      id: n.id, classType: n.classType, label: nodeLabel(n, view.originalIds), title: n.title ?? '', color: n.color ?? null, x, y, w: NODE_W, h: nodeHeight(n),
      locked: locked.has(n.id), missing: missing.has(n.id),
      inputs: n.inputs.map((p2, k) => ({ name: p2.name, type: p2.type, color: typeColor(p2.type), index: k, optional: p2.optional, linked: linkedIn.has(`${n.id}\u0000${p2.name}`), ...inputAnchor({ x, y }, k) })),
      outputs: n.outputs.map((p2, k) => ({ name: p2.name, type: p2.type, color: typeColor(p2.type), index: k, optional: false, linked: false, ...outputAnchor({ x, y }, k) })),
    };
  });
  const byId = new Map(nodes.map(n => [n.id, n]));
  const edges: SceneEdge[] = [];
  for (const c of doc.connections as ConnectionV1[]) {
    const a = byId.get(c.from.node), b = byId.get(c.to.node);
    if (!a || !b) continue;
    const out = a.outputs[c.from.output], inp = b.inputs.find(p => p.name === c.to.input);
    if (!out || !inp) continue;
    edges.push({ id: c.id, d: edgePath(out, inp), color: out.color, from: a.id, to: b.id });
  }
  const frames: SceneFrame[] = [];
  for (const g of doc.groups as EditorGroupV1[]) {
    const b = g.bounds
      ? { x: g.bounds.x, y: g.bounds.y, w: g.bounds.width, h: g.bounds.height }
      : bounds(nodes, g.nodeIds, GROUP_PAD);
    if (b) frames.push({ id: g.id, title: g.title, color: g.color ?? null, kind: 'group', nodeIds: g.nodeIds, ...b });
  }
  for (const s of (doc.subgraphs ?? []) as SubgraphContainerV1[]) {
    const b = bounds(nodes, s.nodeIds, GROUP_PAD);
    if (b) frames.push({ id: s.id, title: `${s.title} (subgraph, non éditable)`, color: null, kind: 'subgraph', nodeIds: s.nodeIds, ...b });
  }
  return { nodes, edges, frames };
}

/** Is an output -> input link plausible on the front end? (Rust remains the arbiter.) */
export function canConnect(scene: Scene, from: { node: string; output: number }, to: { node: string; input: string }): boolean {
  if (from.node === to.node) return false;
  const a = scene.nodes.find(n => n.id === from.node), b = scene.nodes.find(n => n.id === to.node);
  if (!a || !b || a.locked || b.locked) return false;
  const out = a.outputs[from.output], inp = b.inputs.find(p => p.name === to.input);
  if (!out || !inp || inp.linked) return false;
  return typesCompatible(inp.type, out.type);
}

/** Filter by class or id (internal or original API), case-insensitive. */
export function filterNodes(view: GraphView, query: string): EditorNodeV1[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return view.document.nodes.filter(n =>
    n.classType.toLowerCase().includes(q) || n.id.toLowerCase().includes(q) ||
    (view.originalIds[n.id] ?? '').toLowerCase().includes(q));
}

/** Classes derived from the document (for the add list). */
export function knownClasses(view: GraphView): string[] {
  return [...new Set(view.document.nodes.map(n => n.classType))].sort();
}

/** Transform (pan/zoom) centring a world point in a viewport. */
export function centerOn(world: { x: number; y: number }, zoom: number, vw: number, vh: number) {
  return { zoom, panX: vw / 2 - world.x * zoom, panY: vh / 2 - world.y * zoom };
}

export function clampZoom(z: number): number { return Math.min(2.5, Math.max(0.15, z)); }

/** Transform enclosing the whole scene. */
export function fitTransform(scene: Scene, vw: number, vh: number) {
  if (!scene.nodes.length) return { zoom: 1, panX: 0, panY: 0 };
  const x0 = Math.min(...scene.nodes.map(n => n.x)), y0 = Math.min(...scene.nodes.map(n => n.y));
  const x1 = Math.max(...scene.nodes.map(n => n.x + n.w)), y1 = Math.max(...scene.nodes.map(n => n.y + n.h));
  const zoom = clampZoom(Math.min(vw / (x1 - x0 + 80), vh / (y1 - y0 + 80), 1));
  return { zoom, panX: vw / 2 - ((x0 + x1) / 2) * zoom, panY: vh / 2 - ((y0 + y1) / 2) * zoom };
}

/** Selection by rectangle (world). */
export function nodesInRect(scene: Scene, r: { x0: number; y0: number; x1: number; y1: number }): string[] {
  const xa = Math.min(r.x0, r.x1), xb = Math.max(r.x0, r.x1), ya = Math.min(r.y0, r.y1), yb = Math.max(r.y0, r.y1);
  return scene.nodes.filter(n => n.x < xb && n.x + n.w > xa && n.y < yb && n.y + n.h > ya).map(n => n.id);
}

export function roundPos(v: number): number { return Math.round(v * 10) / 10; }
