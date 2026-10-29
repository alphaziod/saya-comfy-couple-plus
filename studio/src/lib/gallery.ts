export interface Filters {
  checkpoint: string; checkpoints: string[]; lora: string; loras: string[]; character: string; characters: string[]; variant: string; workflow: string; job: string; search: string;
  availability: string; metadataOnly: boolean;
  offset: number; sort: string; metadataState: string; loraExact: boolean; duplicates: boolean; favorite: boolean; review: string;
  // '' = main view (PASS 1 or FINAL image of a multipass generation,
  // plus assets without a capture); 'all' = adds the intermediate passes.
  capture: string;
}
// Sort keys accepted by the engine (gallery_order), with their reader-facing labels.
export const gallerySorts: [string, string][] = [['newest','Plus récentes'],['oldest','Plus anciennes'],['name_asc','Nom A → Z'],['name_desc','Nom Z → A'],['favorites','Favoris d’abord'],['accepted','Validées d’abord'],['character','Par personnage'],['resolution','Plus grande résolution'],['filesize','Fichiers les plus lourds'],['shuffle','Mélange']];
export const reviewLabels: Record<string,string> = { unreviewed: 'À examiner', accepted: 'Validée', rejected: 'Rejetée' };
// Labels of organised captures (brief point 4): badge PASS n or Final.
export function captureLabel(role: string | null, phase: number | null, total: number | null): string {
  if (role === 'final') return 'Finale';
  if (phase !== null && total !== null) return `PASSE ${phase}/${total}`;
  if (phase !== null) return `PASSE ${phase}`;
  return '';
}
// Frontend fallback only: the backend `preferences` response carries the real
// per-user `defaultCaptureRoot`; never a personal path here.
export const defaultCaptureRoot = '';
export const defaultFilters: Filters = { availability: '', metadataOnly:false, checkpoint: '', checkpoints: [], lora: '', loras: [], character: '', characters: [], variant: '', workflow: '', job: '', search: '', offset: 0, sort: 'name_asc', metadataState: '', loraExact: false, duplicates: false, favorite: false, review: '', capture: '' };
export function restoreFilters(value: unknown): Filters {
  const result = { ...defaultFilters, loras: [] as string[], characters: [] as string[] };
  if (!value || typeof value !== 'object') return result;
  const v = value as Record<string, unknown>;
  for (const key of ['checkpoint', 'lora', 'character', 'search', 'variant', 'workflow', 'job'] as const) if (typeof v[key] === 'string') result[key] = v[key];
  if (Array.isArray(v.checkpoints)) result.checkpoints = [...new Set(v.checkpoints.filter((x): x is string => typeof x === 'string' && x.length > 0))].slice(0, 32);
  else if (typeof v.checkpoint === 'string' && v.checkpoint) result.checkpoints = [v.checkpoint];
  if (Array.isArray(v.loras)) result.loras = [...new Set(v.loras.filter((x): x is string => typeof x === 'string' && x.length > 0))].slice(0, 32);
  if (Array.isArray(v.characters)) result.characters = [...new Set(v.characters.filter((x): x is string => typeof x === 'string' && x.length > 0))].slice(0,32);
  if (typeof v.offset === 'number' && Number.isSafeInteger(v.offset) && v.offset >= 0 && v.offset <= 4294967295) result.offset = v.offset;
  for (const key of ['duplicates', 'favorite', 'loraExact', 'metadataOnly'] as const) result[key] = v[key] === true;
  if (gallerySorts.some(([id]) => id === String(v.sort))) result.sort = String(v.sort);
  if (['read','absent','invalid','unknown'].includes(String(v.metadataState))) result.metadataState = String(v.metadataState);
  if (['unreviewed','accepted','rejected'].includes(String(v.review))) result.review = String(v.review);
  if (['present','missing','unreadable','source_inaccessible','outside_roots','removed'].includes(String(v.availability))) result.availability=String(v.availability);
  if (['', 'main', 'all'].includes(String(v.capture))) result.capture = String(v.capture);
  return result;
}
