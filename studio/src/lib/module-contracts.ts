import { t } from './translate-now.ts';
// v1 contract of the "modules" responses of the Rust core (studio_core::modules).
// Defensive parsing: any non-conforming response is rejected, never improvised.

export interface ModulePort {
  name: string;
  kind: string;
  node: string;
  input?: string;
  outputIndex?: number;
}

export interface ModuleDependencies {
  classes: string[];
  checkpoints: string[];
  loras: string[];
}

export interface ModuleParameter {
  node: string;
  input: string;
  kind: string;
  value: string;
}

export interface ModuleSummary {
  id: string;
  name: string;
  description: string;
  currentVersion: number;
  versionCount: number;
  inputs: ModulePort[];
  outputs: ModulePort[];
  dependencies: ModuleDependencies;
}

export interface ModuleDetail {
  id: string;
  version: number;
  isCurrent: boolean;
  manifest: {
    manifestVersion: number;
    name: string;
    description: string;
    inputs: ModulePort[];
    outputs: ModulePort[];
    parameters: ModuleParameter[];
    dependencies: ModuleDependencies;
  };
  nodes: { id: string; classType: string }[];
  nodeCount: number;
  versions: { version: number; createdAt: string }[];
  warnings: string[];
  sourceSha256: string;
  sourceWorkflowId: string | null;
  /** True when dependencies were checked against a live engine schema. */
  engineChecked: boolean;
  createdAt: string;
}

export interface ModuleVersionDiff {
  module: string;
  a: number;
  b: number;
  nodes: { added: unknown[]; removed: unknown[]; changed: unknown[] };
  ports: { inputs: unknown; outputs: unknown };
  parameters: { added: unknown[]; removed: unknown[]; changed: unknown[] };
}

function assertObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(t("{label} invalide.", { label: t(label) }));
  return value as Record<string, unknown>;
}
function strings(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || !value.every(v => typeof v === 'string')) throw new Error(t("{label} invalides.", { label: t(label) }));
  return value as string[];
}
function ports(value: unknown, label: string): ModulePort[] {
  if (!Array.isArray(value)) throw new Error(t("{label} invalides.", { label: t(label) }));
  return value.map(raw => {
    const port = assertObject(raw, label);
    if (typeof port.name !== 'string' || typeof port.kind !== 'string' || typeof port.node !== 'string' ||
        (port.input !== undefined && typeof port.input !== 'string') ||
        (port.outputIndex !== undefined && !Number.isSafeInteger(port.outputIndex))) {
      throw new Error(t("{label} invalides.", { label: t(label) }));
    }
    return port as unknown as ModulePort;
  });
}

export function parseModuleList(value: unknown): ModuleSummary[] {
  if (!Array.isArray(value)) throw new Error(t("Liste de modules invalide."));
  return value.map(raw => {
    const item = assertObject(raw, 'Module');
    for (const key of ['id', 'name', 'description']) {
      if (typeof item[key] !== 'string') throw new Error(t("Module invalide."));
    }
    if (!Number.isSafeInteger(item.currentVersion) || (item.currentVersion as number) < 1 ||
        !Number.isSafeInteger(item.versionCount) || (item.versionCount as number) < 1) {
      throw new Error(t("Versions de module invalides."));
    }
    const dependencies = assertObject(item.dependencies, 'Dépendances');
    return {
      id: item.id as string,
      name: item.name as string,
      description: item.description as string,
      currentVersion: item.currentVersion as number,
      versionCount: item.versionCount as number,
      inputs: ports(item.inputs, 'Ports d’entrée'),
      outputs: ports(item.outputs, 'Ports de sortie'),
      dependencies: {
        classes: strings(dependencies.classes, 'Classes'),
        checkpoints: strings(dependencies.checkpoints, 'Checkpoints'),
        loras: strings(dependencies.loras, 'LoRA'),
      },
    };
  });
}

export function parseModuleDetail(value: unknown): ModuleDetail {
  const detail = assertObject(value, 'Module');
  if (typeof detail.id !== 'string' || !Number.isSafeInteger(detail.version) ||
      typeof detail.isCurrent !== 'boolean' || typeof detail.nodeCount !== 'number') {
    throw new Error(t("Détail de module invalide."));
  }
  const manifest = assertObject(detail.manifest, 'Manifeste');
  if (manifest.manifestVersion !== 1 || typeof manifest.name !== 'string') {
    throw new Error(t("Manifeste de module incompatible : format v1 attendu."));
  }
  if (!Array.isArray(detail.nodes) || !detail.nodes.every(node => {
    const item = assertObject(node, 'Node');
    return typeof item.id === 'string' && typeof item.classType === 'string';
  })) throw new Error(t("Nodes du module invalides."));
  if (!Array.isArray(detail.versions) || !detail.versions.every(v => {
    const item = assertObject(v, 'Version');
    return Number.isSafeInteger(item.version) && typeof item.createdAt === 'string';
  })) throw new Error(t("Versions du module invalides."));
  if (!Array.isArray(detail.warnings) || !detail.warnings.every(w => typeof w === 'string')) {
    throw new Error(t("Avertissements du module invalides."));
  }
  const parameters: ModuleParameter[] = (Array.isArray(manifest.parameters) ? manifest.parameters : []).map(raw => {
    const parameter = assertObject(raw, 'Paramètre');
    if (typeof parameter.node !== 'string' || typeof parameter.input !== 'string' ||
        typeof parameter.kind !== 'string' || typeof parameter.value !== 'string') {
      throw new Error(t("Paramètre du module invalide."));
    }
    return parameter as unknown as ModuleParameter;
  });
  const dependencies = assertObject(manifest.dependencies, 'Dépendances');
  return {
    id: detail.id as string,
    version: detail.version as number,
    isCurrent: detail.isCurrent as boolean,
    manifest: {
      manifestVersion: manifest.manifestVersion as number,
      name: manifest.name as string,
      description: typeof manifest.description === 'string' ? manifest.description : '',
      inputs: ports(manifest.inputs, 'Ports d’entrée'),
      outputs: ports(manifest.outputs, 'Ports de sortie'),
      parameters,
      dependencies: {
        classes: strings(dependencies.classes, 'Classes'),
        checkpoints: strings(dependencies.checkpoints, 'Checkpoints'),
        loras: strings(dependencies.loras, 'LoRA'),
      },
    },
    nodes: detail.nodes as { id: string; classType: string }[],
    nodeCount: detail.nodeCount,
    versions: detail.versions as { version: number; createdAt: string }[],
    warnings: detail.warnings as string[],
    sourceSha256: typeof detail.sourceSha256 === 'string' ? detail.sourceSha256 : '',
    sourceWorkflowId: typeof detail.sourceWorkflowId === 'string' ? detail.sourceWorkflowId : null,
    engineChecked: detail.engineChecked === true,
    createdAt: typeof detail.createdAt === 'string' ? detail.createdAt : '',
  };
}

export function parseModuleDiff(value: unknown): ModuleVersionDiff {
  const diff = assertObject(value, 'Diff');
  if (typeof diff.module !== 'string' || !Number.isSafeInteger(diff.a) || !Number.isSafeInteger(diff.b)) {
    throw new Error(t("Comparaison de versions invalide."));
  }
  const section = (raw: unknown, label: string) => {
    const section = assertObject(raw, label);
    for (const key of ['added', 'removed', 'changed']) {
      if (!Array.isArray(section[key])) throw new Error(t("{label} invalide.", { label: t(label) }));
    }
    return section as unknown as ModuleVersionDiff['nodes'];
  };
  const ports = assertObject(diff.ports, 'Ports');
  return {
    module: diff.module as string,
    a: diff.a as number,
    b: diff.b as number,
    nodes: section(diff.nodes, 'Nodes'),
    ports: { inputs: section(ports.inputs, 'Ports d’entrée'), outputs: section(ports.outputs, 'Ports de sortie') },
    parameters: section(diff.parameters, 'Paramètres') as ModuleVersionDiff['parameters'],
  };
}

// --- Composition and reuse helpers (pure, mirrored from studio_core::modules) ---

/** Same rule as Rust `compatible`: exact type, comma alternatives, or `*` wildcard. */
export function kindsCompatible(expected: string, provided: string): boolean {
  return expected === '*' || provided === '*' || expected.split(',').some(k => k.trim() === provided);
}

export interface ModuleLink { from: string; to: string; kind: string }
export interface Composition { links: ModuleLink[]; unsatisfied: ModulePort[]; composable: boolean }

/**
 * Type contract between two modules chained upstream → downstream: every
 * (output, input) pair whose types match, and the downstream inputs no
 * upstream output can feed. Contract only: nothing here executes a pass.
 */
export function composeModules(upstream: { outputs: ModulePort[] }, downstream: { inputs: ModulePort[] }): Composition {
  const links: ModuleLink[] = [];
  for (const output of upstream.outputs) for (const input of downstream.inputs) {
    if (kindsCompatible(input.kind, output.kind)) links.push({ from: output.name, to: input.name, kind: output.kind });
  }
  const unsatisfied = downstream.inputs.filter(input => !links.some(link => link.to === input.name));
  return { links, unsatisfied, composable: links.length > 0 };
}

export interface InputChoice { port: string; node: string; outputIndex: number }
export interface OutputChoice { port: string; node: string; input: string }

/**
 * Builds the Rust `ModuleConnection` (camelCase, overrides as an array) from
 * the user's choices. Every module input must be bound; outputs are optional.
 */
export function buildConnection(inputPorts: string[], inputs: Partial<Record<string, InputChoice>>, outputs: OutputChoice[]):
  { connection: { inputs: { port: string; node: string; outputIndex: number }[]; outputs: { port: string; node: string; input: string }[]; overrides: unknown[] }; missing: string[] } {
  const missing = inputPorts.filter(port => !inputs[port]);
  return {
    connection: {
      inputs: inputPorts.flatMap(port => inputs[port] ? [{ port, node: inputs[port]!.node, outputIndex: inputs[port]!.outputIndex }] : []),
      outputs: outputs.map(({ port, node, input }) => ({ port, node, input })),
      overrides: [],
    },
    missing,
  };
}

/** Lowercase identifier suitable for a module instance (Rust: 1–64 of [A-Za-z0-9._-]). */
export function instanceId(name: string): string {
  const slug = name.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().replace(/[^a-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48);
  return slug || 'module';
}
