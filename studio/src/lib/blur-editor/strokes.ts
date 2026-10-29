// Serialisable (JSON) stroke data model for non-destructive replay.

export interface StrokePoint { x: number; y: number }

export interface Stroke {
  id: string;
  size: number; // brush diameter in image pixels
  intensity: number; // 0 = aucun effet, 1 = flou maximal
  hardness: number; // 0 = gradient from the centre, 1 = sharp edge
  points: StrokePoint[];
}

export interface StrokeDocument { version: 1; strokes: Stroke[] }

export const MAX_STROKE_SIZE = 2000;
export const MAX_STROKE_POINTS = 20000;

function finite(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

export function validateStroke(raw: unknown): Stroke {
  if (!raw || typeof raw !== 'object') throw new Error('Tracé invalide.');
  const value = raw as Record<string, unknown>;
  if (typeof value.id !== 'string' || !value.id || value.id.length > 100) throw new Error('Identifiant de tracé invalide.');
  if (!finite(value.size) || value.size <= 0 || value.size > MAX_STROKE_SIZE) throw new Error('Taille de pinceau invalide.');
  if (!finite(value.intensity) || value.intensity < 0 || value.intensity > 1) throw new Error('Intensité de flou invalide.');
  if (!finite(value.hardness) || value.hardness < 0 || value.hardness > 1) throw new Error('Dureté de pinceau invalide.');
  if (!Array.isArray(value.points) || value.points.length < 1 || value.points.length > MAX_STROKE_POINTS) throw new Error('Tracé sans point ou trop long.');
  const points: StrokePoint[] = value.points.map((point) => {
    if (!point || typeof point !== 'object') throw new Error('Point de tracé invalide.');
    const { x, y } = point as Record<string, unknown>;
    if (!finite(x) || !finite(y) || Math.abs(x) > 1e7 || Math.abs(y) > 1e7) throw new Error('Coordonnée de tracé invalide.');
    return { x, y };
  });
  return { id: value.id, size: value.size, intensity: value.intensity, hardness: value.hardness, points };
}

export function serializeDocument(document: StrokeDocument): string {
  return JSON.stringify(document);
}

export function parseDocument(text: string): StrokeDocument {
  let raw: unknown;
  try { raw = JSON.parse(text); } catch { throw new Error('Document de retouche illisible (JSON invalide).'); }
  if (!raw || typeof raw !== 'object') throw new Error('Document de retouche invalide.');
  const value = raw as Record<string, unknown>;
  if (value.version !== 1) throw new Error('Version de document de retouche inconnue.');
  if (!Array.isArray(value.strokes)) throw new Error('Document de retouche sans liste de tracés.');
  return { version: 1, strokes: value.strokes.map(validateStroke) };
}

export function createStrokeId(): string {
  return globalThis.crypto.randomUUID();
}
