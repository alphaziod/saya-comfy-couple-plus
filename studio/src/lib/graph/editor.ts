import { t } from '../translate-now.ts';
// v1 contract of the graph editing document — mirror of
// crates/studio-core/src/graph_editor.rs (serde camelCase, unknown fields
// rejected on the Rust side). The Rust core is the source of truth: this TypeScript
// boundary rejects any non-conforming document before display, and exposes
// the same structural rules (unique identifiers, resolved
// connections, type compatibility, no cycles, opaque subgraph
// boxes).

export const GRAPH_FORMAT_VERSION = 1;

export interface EditorPoint {
  x: number;
  y: number;
}

export interface EditorSize {
  width: number;
  height: number;
}

export interface InputPortV1 {
  name: string;
  type: string;
  optional: boolean;
}

export interface OutputPortV1 {
  name: string;
  type: string;
}

export interface ExposedPortV1 {
  name: string;
  type: string;
}

export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

export interface EditorNodeV1 {
  id: string;
  classType: string;
  title?: string;
  color?: string | null;
  position: EditorPoint | null;
  size: EditorSize | null;
  inputs: InputPortV1[];
  outputs: OutputPortV1[];
  params: { [key: string]: JsonValue };
}

export interface LinkEndV1 {
  node: string;
  output: number;
}

export interface LinkEndInputV1 {
  node: string;
  input: string;
}

export interface ConnectionV1 {
  id: string;
  from: LinkEndV1;
  to: LinkEndInputV1;
}

export interface EditorGroupV1 {
  id: string;
  title: string;
  nodeIds: string[];
  bounds?: { x: number; y: number; width: number; height: number } | null;
  color?: string | null;
}

/** Opaque box: members intact, only the existing exposed inputs/outputs
 * are described. No transformation of the content. */
export interface SubgraphContainerV1 {
  id: string;
  title: string;
  nodeIds: string[];
  inputs: ExposedPortV1[];
  outputs: ExposedPortV1[];
}

export interface EditorDocumentV1 {
  formatVersion: 1;
  id: string;
  name: string;
  nodes: EditorNodeV1[];
  connections: ConnectionV1[];
  groups: EditorGroupV1[];
  subgraphs: SubgraphContainerV1[];
}

function isRecord(value: unknown): value is { [key: string]: unknown } {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function str(value: unknown): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(t("Champ texte requis absent du document d'édition."));
  }
  return value;
}

function array(value: unknown): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(t("Tableau requis absent du document d'édition."));
  }
  return value;
}

function port(value: unknown, optional: boolean): InputPortV1 {
  if (!isRecord(value)) {
    throw new Error(t("Port d'entrée invalide."));
  }
  return { name: str(value.name), type: str(value.type), optional: typeof value.optional === 'boolean' ? value.optional : optional };
}

function outputPort(value: unknown): OutputPortV1 {
  if (!isRecord(value)) {
    throw new Error(t("Port de sortie invalide."));
  }
  return { name: str(value.name), type: str(value.type) };
}

function exposedPort(value: unknown): ExposedPortV1 {
  if (!isRecord(value)) {
    throw new Error(t("Port exposé invalide."));
  }
  return { name: str(value.name), type: str(value.type) };
}

function point(value: unknown): EditorPoint | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (!isRecord(value) || !isFiniteNumber(value.x) || !isFiniteNumber(value.y)) {
    throw new Error('Position invalide.');
  }
  return { x: value.x, y: value.y };
}

function size(value: unknown): EditorSize | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (!isRecord(value) || !isFiniteNumber(value.width) || !isFiniteNumber(value.height) ||
      value.width <= 0 || value.height <= 0) {
    throw new Error('Taille invalide.');
  }
  return { width: value.width, height: value.height };
}

function params(value: unknown): { [key: string]: JsonValue } {
  if (!isRecord(value)) {
    throw new Error(t("Paramètres invalides."));
  }
  for (const item of Object.values(value)) {
    if (Array.isArray(item) || item === null) {
      // An array would be a form of link: links are connections.
      throw new Error(t("Valeur de paramètre interdite (tableau ou null)."));
    }
  }
  return value as { [key: string]: JsonValue };
}

/** Same rule as the Rust core: wildcard, or an "A,B" list containing the given type. */
export function typesCompatible(expected: string, provided: string): boolean {
  return expected === '*' || provided === '*' ||
    expected.split(',').some(t => t.trim() === provided);
}

/** Required inputs with no connection or parameter: intermediate state
 * allowed while editing, flagged before any executable export. */
export function missingRequiredInputs(doc: EditorDocumentV1): Array<[string, string]> {
  const connected = new Set(doc.connections.map(c => `${c.to.node}\u0000${c.to.input}`));
  const missing: Array<[string, string]> = [];
  for (const node of doc.nodes) {
    for (const input of node.inputs) {
      const filled = connected.has(`${node.id}\u0000${input.name}`) || input.name in node.params;
      if (!input.optional && !filled) {
        missing.push([node.id, input.name]);
      }
    }
  }
  return missing;
}

export function parseGraphDocument(value: unknown): EditorDocumentV1 {
  if (!isRecord(value)) {
    throw new Error(t("Document d'édition invalide."));
  }
  if (value.formatVersion !== GRAPH_FORMAT_VERSION) {
    throw new Error(t("Format d'éditeur non pris en charge : {formatVersion} (attendu {GRAPH_FORMAT_VERSION}).", { formatVersion: String(value.formatVersion), GRAPH_FORMAT_VERSION }));
  }
  const known = new Set(['formatVersion', 'id', 'name', 'nodes', 'connections', 'groups', 'subgraphs']);
  for (const key of Object.keys(value)) {
    if (!known.has(key)) {
      throw new Error(t("Champ inconnu du format v1 : {key}.", { key }));
    }
  }
  const nodeIds = new Set<string>();
  const nodes: EditorNodeV1[] = [];
  for (const raw of array(value.nodes)) {
    if (!isRecord(raw)) {
      throw new Error(t("Node d'édition invalide."));
    }
    const node: EditorNodeV1 = {
      id: str(raw.id),
      classType: str(raw.classType),
      title: raw.title === undefined ? '' : str(raw.title),
      color: raw.color == null ? null : str(raw.color),
      position: point(raw.position ?? null),
      size: size(raw.size ?? null),
      inputs: array(raw.inputs ?? []).map(p => port(p, false)),
      outputs: array(raw.outputs ?? []).map(outputPort),
      params: params(raw.params ?? {}),
    };
    if (nodeIds.has(node.id)) {
      throw new Error(t("Identifiant de node dupliqué : {id}.", { id: node.id }));
    }
    nodeIds.add(node.id);
    nodes.push(node);
  }
  const byId = new Map(nodes.map(n => [n.id, n]));
  const ids = new Set(nodeIds);
  const targets = new Set<string>();
  const connections: ConnectionV1[] = [];
  for (const raw of array(value.connections)) {
    if (!isRecord(raw) || !isRecord(raw.from) || !isRecord(raw.to)) {
      throw new Error('Connexion invalide.');
    }
    const id = str(raw.id);
    const output = raw.from.output;
    if (!isFiniteNumber(output) || !Number.isSafeInteger(output) || output < 0) {
      throw new Error(t("Index de sortie invalide."));
    }
    const from: LinkEndV1 = { node: str(raw.from.node), output };
    const to = { node: str(raw.to.node), input: str(raw.to.input) };
    if (ids.has(id)) {
      throw new Error(t("Identifiant dupliqué : {id}.", { id }));
    }
    ids.add(id);
    const source = byId.get(from.node);
    const target = byId.get(to.node);
    if (!source || !target) {
      throw new Error(t("Connexion pendante vers un node absent."));
    }
    if (from.node === to.node) {
      throw new Error('Auto-connexion interdite.');
    }
    const provided = source.outputs[from.output];
    const expected = target.inputs.find(p => p.name === to.input);
    if (!provided || !expected) {
      throw new Error(t("Port inexistant : {node}.{input}.", { node: to.node, input: to.input }));
    }
    if (!typesCompatible(expected.type, provided.type)) {
      throw new Error(t("Types de lien incompatibles : attend {type}, reçu {type2}.", { type: expected.type, type2: provided.type }));
    }
    const key = `${to.node}\u0000${to.input}`;
    if (targets.has(key)) {
      throw new Error(t("Entrée déjà connectée : {node}.{input}.", { node: to.node, input: to.input }));
    }
    targets.add(key);
    connections.push({ id, from, to });
  }
  // Cycle detection: the target of a connection must not reach its
  // origin by following the outgoing connections.
  const outgoing = new Map<string, string[]>(nodes.map(n => [n.id, []]));
  for (const connection of connections) {
    outgoing.get(connection.from.node)?.push(connection.to.node);
  }
  for (const connection of connections) {
    const seen = new Set<string>();
    const queue = [connection.to.node];
    while (queue.length > 0) {
      const current = queue.pop();
      if (current === undefined || seen.has(current)) {
        continue;
      }
      seen.add(current);
      if (current === connection.from.node) {
        throw new Error(t("Cycle détecté dans le graphe d'édition."));
      }
      queue.push(...(outgoing.get(current) ?? []));
    }
  }
  const groups: EditorGroupV1[] = [];
  const membership = new Map<string, 'group' | 'subgraph'>();
  for (const raw of array(value.groups)) {
    if (!isRecord(raw)) {
      throw new Error('Groupe invalide.');
    }
    const id = str(raw.id);
    if (ids.has(id)) {
      throw new Error(t("Identifiant dupliqué : {id}.", { id }));
    }
    ids.add(id);
    const nodeIds: string[] = array(raw.nodeIds ?? []).map(m => str(m));
    if (nodeIds.length === 0) {
      throw new Error('Groupe sans node.');
    }
    for (const member of nodeIds) {
      if (!byId.has(member) || membership.has(member)) {
        throw new Error(t("Membre de groupe invalide : {member}.", { member }));
      }
      membership.set(member, 'group');
    }
    groups.push({ id, title: typeof raw.title === 'string' ? raw.title : '', nodeIds });
  }
  const subgraphs: SubgraphContainerV1[] = [];
  for (const raw of array(value.subgraphs)) {
    if (!isRecord(raw)) {
      throw new Error('Conteneur subgraph invalide.');
    }
    const id = str(raw.id);
    if (ids.has(id)) {
      throw new Error(t("Identifiant dupliqué : {id}.", { id }));
    }
    ids.add(id);
    const nodeIds: string[] = array(raw.nodeIds ?? []).map(m => str(m));
    if (nodeIds.length === 0) {
      throw new Error('Conteneur subgraph sans node.');
    }
    for (const member of nodeIds) {
      if (!byId.has(member) || membership.has(member)) {
        throw new Error(t("Membre de conteneur invalide : {member}.", { member }));
      }
      membership.set(member, 'subgraph');
    }
    subgraphs.push({
      id,
      title: typeof raw.title === 'string' ? raw.title : '',
      nodeIds,
      inputs: array(raw.inputs ?? []).map(exposedPort),
      outputs: array(raw.outputs ?? []).map(exposedPort),
    });
  }
  const document: EditorDocumentV1 = {
    formatVersion: 1,
    id: str(value.id),
    name: str(value.name),
    nodes,
    connections,
    groups,
    subgraphs,
  };
  return document;
}
