import { t } from './translate-now.ts';
// Scene manifest contract (mirrors studio-core::scenes). Pure helpers only:
// texts stay byte-exact, validation is local, nothing here talks to IPC.

export type Composition = 'solo' | 'couple';
export type SceneTextSource = 'workflow' | 'preset' | 'custom';
export interface SceneZone { x: number; y: number; width: number; height: number; feather: number; enabled: boolean; z: number }
export interface ScenePerson { characterId: string; variantId: string | null; zone?: SceneZone | null }
export interface SceneText { source: SceneTextSource; from?: string; text?: string }
export interface SceneMask { nodeId: string; direction: 'vertical' | 'horizontal'; split: number; blur: number; swap: boolean }
export interface SceneManifest {
  grokSource?:{sceneId:string;variantIndex:number};
  workflowId: string;
  composition: Composition;
  p1: ScenePerson;
  p2?: ScenePerson | null;
  action: SceneText;
  bg: SceneText;
  quality?: SceneText | null;
  negative?: SceneText | null;
  mask?: SceneMask | null;
}

// The composition must match the workflow profile derived from saved P1/P2
// bindings; an unconfigured workflow supports no composition at all.
export function compositionForProfile(profile: string): Composition | null {
  if (profile === 'solo' || profile === 'couple') return profile;
  return null;
}

export function sceneText(source: SceneTextSource, from: string | null, text: string): SceneText {
  const spec: SceneText = { source };
  if (from) spec.from = from;
  spec.text = text;
  return spec;
}

export function validateManifest(manifest: SceneManifest): string[] {
  const problems: string[] = [];
  if (!manifest.workflowId || typeof manifest.workflowId !== 'string') problems.push(t("Workflow absent du manifeste."));
  if (manifest.composition !== 'solo' && manifest.composition !== 'couple') problems.push('Composition inconnue.');
  if (!manifest.p1?.characterId) problems.push(t("P1 : sélectionnez une fiche existante."));
  if (manifest.composition === 'solo' && manifest.p2) problems.push(t("Composition SOLO : le second personnage (P2) est refusé."));
  if (manifest.composition === 'couple' && !manifest.p2?.characterId) problems.push(t("Composition COUPLE : un second personnage (P2) est requis."));
  for (const [role, person] of [['P1', manifest.p1], ['P2', manifest.p2]] as const) {
    const zone = person?.zone;
    if (!zone) continue;
    if (![zone.x, zone.y, zone.width, zone.height, zone.feather].every(Number.isFinite)
      || zone.x < 0 || zone.y < 0 || zone.width <= 0 || zone.height <= 0
      || zone.x + zone.width > 1.000001 || zone.y + zone.height > 1.000001
      || zone.feather < 0 || zone.feather > 1) {
      problems.push(t("{role} : zone invalide (coordonnées normalisées 0..1, feather 0..1).", { role }));
    }
  }
  const shared: [string, SceneText | null | undefined][] = [['ACTION', manifest.action], ['BG', manifest.bg], ['QUALITY', manifest.quality], ['NEG', manifest.negative]];
  for (const [section, spec] of shared) {
    if (!spec) continue;
    if (spec.source === 'preset') {
      if (!spec.from) problems.push(t("{section} : la source preset exige un rôle (P1 ou P2).", { section }));
      else if (manifest.composition === 'solo' && spec.from !== 'P1') problems.push(t("{section} : en SOLO, la source preset ne peut venir que de P1.", { section }));
      else if (spec.from !== 'P1' && spec.from !== 'P2') problems.push(t("{section} : rôle de preset inconnu.", { section }));
    }
    if (typeof spec.text !== 'string') problems.push(t("{section} : texte attendu sous forme de chaîne.", { section }));
  }
  const mask = manifest.mask;
  if (mask) {
    if (!mask.nodeId) problems.push(t("Masque : sélectionnez un node SayaSplitMask mappé."));
    if (mask.direction !== 'vertical' && mask.direction !== 'horizontal') problems.push('Masque : direction verticale ou horizontale uniquement.');
    if (!Number.isInteger(mask.split) || mask.split < 0 || mask.split > 100) problems.push(t("Masque : séparation entière entre 0 et 100."));
    if (!Number.isFinite(mask.blur) || mask.blur < 0) problems.push(t("Masque : flou décimal positif requis."));
    if (typeof mask.swap !== 'boolean') problems.push(t("Masque : inversion booléenne attendue."));
  }
  return problems;
}

// Editor state -> manifest. Prompt texts are never trimmed or re-encoded.
export function buildManifest(input: {
  grokSource?:{sceneId:string;variantIndex:number};
  workflowId: string;
  composition: Composition;
  p1: ScenePerson;
  p2: ScenePerson | null;
  action: SceneText;
  bg: SceneText;
  quality: SceneText | null;
  negative: SceneText | null;
  mask: SceneMask | null;
}): SceneManifest {
  const manifest: SceneManifest = {
    workflowId: input.workflowId,
    composition: input.composition,
    p1: input.p1,
    action: input.action,
    bg: input.bg,
  };
  // P2 is kept even for SOLO so validation refuses it explicitly instead of
  // silently dropping the second person.
  if(input.grokSource)manifest.grokSource=input.grokSource;
  if (input.p2) manifest.p2 = input.p2;
  if (input.quality) manifest.quality = input.quality;
  if (input.negative) manifest.negative = input.negative;
  if (input.mask) manifest.mask = input.mask;
  return manifest;
}
