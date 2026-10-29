import {thumbCache, previewCache} from './stream';
import { t } from './translate-now.ts';
import { invoke, isTauri } from '@tauri-apps/api/core';
export const native = isTauri();
export {userError as message} from './user-error';
// IPC call instrumentation (navigation perf, brief point 6): when a
// smoke harness installs `window.__SAYA_IPC__`, each `call()` records
// its action, count and duration there. The app behaves identically
// without this hook.
export interface IpcProbe { counts: Record<string, number>; ms: Record<string, number[]> }
export async function call<T = unknown>(action: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!native) throw new Error(t("Ouvrir le binaire Saya Studio : cet aperçu navigateur ne contient pas le noyau Rust."));
  const probe = (window as { __SAYA_IPC__?: IpcProbe }).__SAYA_IPC__;
  const start = probe ? performance.now() : 0;
  try {
    return await invoke<T>('studio', { request: { action, ...args } });
  } finally {
    if (probe) {
      probe.counts[action] = (probe.counts[action] ?? 0) + 1;
      (probe.ms[action] ??= []).push(performance.now() - start);
    }
  }
}
export interface Character { id: string; name: string; identity: Record<string, unknown>; imageIds: string[]; archived: boolean; revision: number }
export interface Asset { id: string; path: string; width: number | null; height: number | null; metadataState: string; favorite: boolean; note: string; review: string; contentSha256: string | null; captureRole: string | null; capturePhase: number | null; captureDay: string | null; captureFamille: string | null }
export type { Filters } from './gallery';
export interface Job { characters:{role:string;characterId:string;name:string;revision:number}[]; id: string; state: string; promptId: string | null; workflow: string; error: unknown; outputs: { assetId: string }[]; createdAt: string }
// Keep decoded images bounded even when a page mounts 48 cards at once.
let active = 0;
const waiting: (() => void)[] = [];
const imageRequests = new Map<string, {promise:Promise<string>; consumers:(AbortSignal|undefined)[]}>();
export async function imageSource(id: string, preview = false, signal?: AbortSignal): Promise<string> {
  signal?.throwIfAborted();
  const key = (preview ? 'p:' : '') + id;
  const cache = preview ? previewCache : thumbCache;
  const cached = cache.get(key);
  if (cached) return cached;
  let entry = imageRequests.get(key);
  if (entry) entry.consumers.push(signal);
  else {
    const consumers = [signal];
    const request = (async () => {
      if (active >= 3) await new Promise<void>(resolve => {
        // The opened image must not wait behind an entire grid of thumbnails.
        if (preview) waiting.unshift(resolve); else waiting.push(resolve);
      }); else active++;
      try {
        if (consumers.every(consumer=>consumer?.aborted)) throw new DOMException('Image hors écran', 'AbortError');
        const value = await call<string>('image', { id, preview });
        cache.set(key, value);
        return value;
      } finally { const next = waiting.shift(); if (next) next(); else active--; }
    })();
    entry = {promise:request, consumers};
    imageRequests.set(key, entry);
    void request.finally(() => imageRequests.delete(key)).catch(() => {});
  }
  // A scrolled-away card cannot cancel another viewer of the same image.
  const value = await entry.promise;
  signal?.throwIfAborted();
  return value;
}

export interface LiveProgress { node:string|null; value:number|null; max:number|null; preview:string|null; error:unknown; finished:boolean; nodes:Record<string,{value:number;max:number;state:string}> }
export interface LiveObservation { revision:number;connected:boolean;offlineNote:string|null;clientId:string;reconnects:number;retryMs:number;error:string|null;httpError:string|null;lastHttpSync:number|null;jobs:Record<string,LiveProgress> }
export interface FileStatus {state:string;reason:string|null;path:string}
export interface CharacterCard {portraitStatus:FileStatus|null; id: string; name: string; portrait: string | null; imageCount: number; visibleImageCount: number; variantCount: number; mainVariant: string | null }
export interface SheetReference { assetId: string; variantId: string | null; variantName: string | null; position: number; primary: boolean; availability: string; historicalVariant: boolean }
export interface CharacterSheetData {
  portraitStatus:FileStatus|null;
  id: string; name: string; archived: boolean; revision: number; portrait: string | null;
  identityPrompt: string; anatomyPrompt: string; description: string; traits: string;
  backgrounds: { variantId: string; variantName: string; bg: string }[];
  variants: { id: string; name: string; promptBlocks: Record<string, string>; constraints: Record<string, unknown>; revision: number }[];
  topImages: { id: string; path: string; width: number | null; height: number | null; favorite: boolean; review: string; modifiedAt: string | null }[];
  references: SheetReference[]; imageCount: number; visibleImageCount: number;
}

/** Draft job frozen by `prepare` (studio_core::workflow::Prepared). */
export interface PreparedJob { jobId: string; snapshotHash: string; requestJson: string; removed: string[]; changes: { node: string; input: string; before: string; after: string }[]; nodeCount: number }
