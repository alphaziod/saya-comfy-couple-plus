export interface Bootstrap {
  apiVersion: 1;
  appVersion: string;
  schemaVersion: number;
  counts: { characters: number; assets: number; jobs: number };
  comfyConnected: boolean;
  implemented: string[];
}

export function parseBootstrap(value: unknown): Bootstrap {
  const data = value as Partial<Bootstrap> | null;
  if (!data || data.apiVersion !== 1 || typeof data.appVersion !== 'string' ||
      !Number.isSafeInteger(data.schemaVersion) || (data.schemaVersion ?? 0) < 1 ||
      !data.counts || !['characters', 'assets', 'jobs'].every(key => {
        const count = data.counts?.[key as keyof Bootstrap['counts']];
        return Number.isSafeInteger(count) && (count ?? -1) >= 0;
      }) || typeof data.comfyConnected !== 'boolean' ||
      !Array.isArray(data.implemented) || !data.implemented.every(x => typeof x === 'string')) {
    throw new Error('Réponse du noyau incompatible avec le contrat v1.');
  }
  return data as Bootstrap;
}
