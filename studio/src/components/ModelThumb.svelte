<script lang="ts" module>
  import { call } from '../lib/api';
  // Shared cache and a small concurrency gate: a grid of hundreds of cards decodes a few at a time.
  const cache = new Map<string, string>();
  let active = 0;
  const waiting: (() => void)[] = [];
  async function gated<T>(job: () => Promise<T>): Promise<T> {
    if (active >= 6) await new Promise<void>(r => waiting.push(r));
    active++;
    try { return await job(); } finally { active--; waiting.shift()?.(); }
  }
  export function loadThumb(key: string, request: () => Promise<string>): Promise<string> {
    const hit = cache.get(key);
    return hit ? Promise.resolve(hit) : gated(request).then(v => { cache.set(key, v); return v; });
  }
  export function modelThumb(kind: string, path: string) { return loadThumb(`${kind}:${path}`, () => call<string>('model_thumbnail', { kind, path })); }
  export function civitaiThumb(url: string) { return loadThumb(`civitai:${url}`, () => call<string>('civitai_thumbnail', { url })); }
</script>
<script lang="ts">
  import { t } from '../lib/i18n';
  let { load, alt = '', label }: { load: (() => Promise<string>) | null; alt?: string; label?: string } = $props();
  let src = $state(''), failed = $state(false), visible = $state(false);
  function observe(node: HTMLElement) {
    const o = new IntersectionObserver(e => { if (e.some(x => x.isIntersecting)) { visible = true; o.disconnect(); } }, { rootMargin: '300px' });
    o.observe(node); return { destroy: () => o.disconnect() };
  }
  $effect(() => { const fn = load; src = ''; failed = false; if (!fn || !visible) return; let live = true; fn().then(v => { if (live) src = v; }).catch(() => { if (live) failed = true; }); return () => { live = false; }; });
</script>
<div class="model-thumb" use:observe>{#if src}<img {src} {alt} />{:else}<span>{!load || failed ? (label ?? $t("Aucun aperçu")) : ''}</span>{/if}</div>
<style>
  .model-thumb{width:100%;height:100%;display:flex;align-items:center;justify-content:center;background:var(--raised);color:var(--faint);font-size:11.5px}
  img{width:100%;height:100%;object-fit:cover;object-position:top;display:block}
</style>
