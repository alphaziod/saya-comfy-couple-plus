import { t } from './translate-now.ts';
// Local construction and validation of a pipeline manifest.
// Pure logic: the Rust core remains the authority (workflows, fingerprints, DAG,
// parameter resolution — studio_core::workflow_pipeline + orchestrator).
export interface DraftStep { id: string; workflowId: string; sourceStep: string; nodeId: string }
export interface WorkflowNode { id: string; classType: string; title: string | null }

// --- Global config (brief point 13): blocks linkable to passes. ---
// A value goes into step.parameters (key = QUALITY/P1/P2/ACTION/BG/NEG section
// or explicit "node.input", resolved by orchestrator.rs preflight) or into
// step.seed for the "seed" key. Several blocks per pipeline.
export const PIPELINE_FORMAT = 'saya-workflow-pipeline';
/** Version of the produced manifest: v2 (multiline) allows the \n of prompts;
 * the core reader accepts 1 and 2 (workflow_pipeline.rs:23-24,435). */
export const PIPELINE_MANIFEST_VERSION = 2;
export const CONFIG_SECTIONS = ['QUALITY', 'P1', 'P2', 'ACTION', 'BG', 'NEG'] as const;
export const MAX_PARAM_VALUE = 200_000;

export interface DraftConfigField { key: string; value: string }
export interface DraftConfig { id: string; label: string; fields: DraftConfigField[] }
export interface ConfigLink { config: string; step: string; field: string }

const STEP_ID = /^[A-Za-z0-9][A-Za-z0-9_-]*$/;
export const MAX_STEPS = 64;

export function newDraftStep(existing: DraftStep[]): DraftStep {
  const used = new Set(existing.map(s => s.id));
  let n = existing.length + 1;
  while (used.has(`passe${n}`)) n++;
  return { id: `passe${n}`, workflowId: '', sourceStep: '', nodeId: '' };
}

export function loadImageNodes(nodes: WorkflowNode[]): WorkflowNode[] {
  return nodes.filter(n => n.classType === 'LoadImage');
}

export function validateDraft(name: string, steps: DraftStep[]): string[] {
  const errors: string[] = [];
  if (!name.trim()) errors.push(t("Le nom du pipeline est vide."));
  if (!steps.length) errors.push(t("Ajoutez au moins une passe."));
  if (steps.length > MAX_STEPS) errors.push(t("Trop de passes (maximum {MAX_STEPS}).", { MAX_STEPS }));
  const seen = new Set<string>();
  steps.forEach((s, i) => {
    if (!STEP_ID.test(s.id)) errors.push(t("Passe {i} : identifiant invalide « {id} » (lettres, chiffres, _ et -, premier caractère alphanumérique).", { i: i + 1, id: s.id }));
    else if (seen.has(s.id)) errors.push(t("Passe {i} : identifiant « {id} » déjà utilisé.", { i: i + 1, id: s.id }));
    seen.add(s.id);
    if (!s.workflowId) errors.push(t("Passe {i} : aucun workflow choisi.", { i: i + 1 }));
    if (s.sourceStep) {
      if (i === 0) errors.push(t("La première passe ne peut pas avoir de liaison amont."));
      else if (!steps.slice(0, i).some(p => p.id === s.sourceStep)) errors.push(t("Passe {i} : l'étape source « {sourceStep} » doit précéder cette passe.", { i: i + 1, sourceStep: s.sourceStep }));
      if (!s.nodeId) errors.push(t("Passe {i} : choisissez le node LoadImage cible de la liaison.", { i: i + 1 }));
    } else if (s.nodeId) errors.push(t("Passe {i} : node cible sans étape source.", { i: i + 1 }));
  });
  return errors;
}

export function newDraftConfig(existing: DraftConfig[]): DraftConfig {
  const used = new Set(existing.map(c => c.id));
  let n = existing.length + 1;
  while (used.has(`config${n}`)) n++;
  return { id: `config${n}`, label: `Config ${n}`, fields: [] };
}

/** Port type displayed for a config field: the seed is an exact u64
 * integer, everything else travels as text (prompt sections, node.input). */
export function configFieldType(key: string): string {
  return key === 'seed' ? 'INT' : 'STRING';
}

/** Key accepted by the core (orchestrator.rs:305-331): "seed", a mapped
 * section, or explicit "node.input". Mirror of check_text (≤120, no
 * control characters); the core remains the final arbiter on save. */
export function configKeyError(key: string): string | null {
  if (!key) return t("clé de champ vide.");
  if (key.length > 120) return t("clé trop longue (maximum 120) : « {key} ».", { key });
  if ([...key].some(c => {
    const code = c.codePointAt(0) ?? 0;
    return (code < 32 && code !== 9) || code === 127;
  })) return t("caractère de contrôle interdit dans la clé « {key} ».", { key: JSON.stringify(key) });
  if (key === 'seed' || (CONFIG_SECTIONS as readonly string[]).includes(key)) return null;
  const dot = key.indexOf('.');
  if (dot <= 0 || dot === key.length - 1) {
    return t("clé « {key} » invalide : attendu une section ({CONFIG_SECTIONS}), « seed » ou « node.entrée ».", { key, CONFIG_SECTIONS: CONFIG_SECTIONS.join(', ') });
  }
  return null;
}

/** v2 value: bounded length, no control characters except \n \r \t
 * (workflow_pipeline.rs:605-615). */
export function configValueError(value: string): string | null {
  if (value.length > MAX_PARAM_VALUE) return t("valeur trop longue (maximum {MAX_PARAM_VALUE}).", { MAX_PARAM_VALUE });
  const bad = [...value].find(c => {
    const code = c.codePointAt(0) ?? 0;
    return code < 32 && code !== 10 && code !== 13 && code !== 9;
  });
  if (bad !== undefined) return t("caractère de contrôle interdit dans la valeur ({bad}).", { bad: JSON.stringify(bad) });
  return null;
}

/** Canonical decimal u64 seed (contracts.rs Seed::parse: identical re-parse). */
export function seedValueError(value: string): string | null {
  if (!/^\d+$/.test(value)) return t("seed : entier décimal attendu.");
  const parsed = BigInt(value);
  if (parsed > 18446744073709551615n) return t("seed : hors limite u64.");
  if (parsed.toString() !== value) return t("seed : notation décimale canonique attendue (sans zéro de tête).");
  return null;
}

/** Removes config links that point to a vanished block or field. */
export function pruneConfigLinks(configs: DraftConfig[], steps: DraftStep[], links: ConfigLink[]): ConfigLink[] {
  return links.filter(l =>
    configs.some(c => c.id === l.config && c.fields.some(f => f.key === l.field)) &&
    steps.some(s => s.id === l.step));
}

/** Full validation of the visual draft: passes (validateDraft), config
 * blocks, and uniqueness of each field supplied to a pass — the core refuses a
 * field defined twice (orchestrator.rs:332-341), so it is already refused here. */
export function validateVisualDraft(name: string, steps: DraftStep[], configs: DraftConfig[], links: ConfigLink[]): string[] {
  const errors = validateDraft(name, steps);
  const ids = new Set<string>();
  for (const config of configs) {
    const where = t("Config « {label} »", { label: config.label || config.id });
    if (!STEP_ID.test(config.id)) errors.push(t("{where} : identifiant invalide « {id} ».", { where, id: config.id }));
    else if (ids.has(config.id)) errors.push(t("{where} : identifiant « {id} » déjà utilisé.", { where, id: config.id }));
    ids.add(config.id);
    if (!config.label.trim()) errors.push(t("{where} : nom vide.", { where }));
    const keys = new Set<string>();
    for (const field of config.fields) {
      const keyError = configKeyError(field.key);
      if (keyError) errors.push(`${where} : ${keyError}`);
      else if (keys.has(field.key)) errors.push(t("{where} : champ « {key} » défini deux fois.", { where, key: field.key }));
      keys.add(field.key);
      if (field.key === 'seed' && !configKeyError(field.key)) {
        const seed = seedValueError(field.value.trim());
        if (seed) errors.push(`${where} : ${seed}`);
      }
      const value = configValueError(field.value);
      if (value) errors.push(`${where} : ${value}`);
    }
  }
  const provided = new Map<string, Map<string, string>>();
  for (const link of links) {
    const config = configs.find(c => c.id === link.config);
    if (!config) { errors.push(t("Liaison de config vers un bloc inconnu : {config}.", { config: link.config })); continue; }
    if (!steps.some(s => s.id === link.step)) { errors.push(t("Liaison de config vers une passe inconnue : {step}.", { step: link.step })); continue; }
    if (!config.fields.some(f => f.key === link.field)) { errors.push(t("Liaison « {field} » : champ absent de « {label} ».", { field: link.field, label: config.label })); continue; }
    if (!provided.has(link.step)) provided.set(link.step, new Map());
    const fields = provided.get(link.step)!;
    const previous = fields.get(link.field);
    if (previous) errors.push(t("Passe {step} : champ « {field} » fourni deux fois ({previous}, {label}) ; le noyau refuserait l'enregistrement.", { step: link.step, field: link.field, previous, label: config.label }));
    else fields.set(link.field, config.label);
  }
  return errors;
}

/** Full v2 manifest: chained passes + parameters/seed from the linked config
 * blocks. A single builder for the simple form and the canvas. */
export function buildVisualManifest(
  name: string,
  steps: DraftStep[],
  configs: DraftConfig[],
  links: ConfigLink[],
  id: string = crypto.randomUUID(),
): Record<string, unknown> {
  const parameters = new Map<string, Record<string, string>>();
  const seeds = new Map<string, string>();
  for (const link of links) {
    const config = configs.find(c => c.id === link.config);
    const field = config?.fields.find(f => f.key === link.field);
    if (!config || !field) continue;
    if (link.field === 'seed') seeds.set(link.step, field.value.trim());
    else {
      if (!parameters.has(link.step)) parameters.set(link.step, {});
      parameters.get(link.step)![link.field] = field.value;
    }
  }
  return {
    format: PIPELINE_FORMAT,
    version: PIPELINE_MANIFEST_VERSION,
    id,
    name: name.trim(),
    workflows: [...new Set(steps.map(s => s.workflowId).filter(Boolean))].map(w => ({ id: w })),
    steps: steps.map(s => {
      const bound = !!(s.sourceStep && s.nodeId);
      const step: Record<string, unknown> = { id: s.id, workflowId: s.workflowId, kind: 'image' };
      if (bound) step.inputs = [{ name: 'image', portType: 'IMAGE' }];
      step.outputs = [{ name: 'image', portType: 'IMAGE' }];
      if (bound) step.bindings = [{ port: 'image', nodeId: s.nodeId, input: 'image', source: { step: s.sourceStep, port: 'image' } }];
      const params = parameters.get(s.id);
      if (params && Object.keys(params).length) step.parameters = params;
      const seed = seeds.get(s.id);
      if (seed) step.seed = seed;
      return step;
    }),
  };
}

export function move<T>(list: T[], index: number, delta: number): T[] {
  const to = index + delta;
  if (to < 0 || to >= list.length) return list;
  const copy = [...list];
  [copy[index], copy[to]] = [copy[to], copy[index]];
  return copy;
}

/** Removes a pass and deletes the links that referenced it. */
export function removeStep(steps: DraftStep[], index: number): DraftStep[] {
  const gone = steps[index]?.id;
  return steps.filter((_, i) => i !== index).map(s => s.sourceStep === gone ? { ...s, sourceStep: '', nodeId: '' } : s);
}

/** After a move: a link whose source no longer precedes is removed. */
export function pruneBindings(steps: DraftStep[]): DraftStep[] {
  return steps.map((s, i) => s.sourceStep && !steps.slice(0, i).some(p => p.id === s.sourceStep) ? { ...s, sourceStep: '', nodeId: '' } : s);
}
