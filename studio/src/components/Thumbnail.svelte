<script lang="ts">
  import { t } from '../lib/i18n';
  import {untrack} from 'svelte';
  import { imageSource, message } from '../lib/api';
  import { previewCache, thumbCache } from '../lib/stream';
  let { id, preview = false, alt }: { id: string; preview?: boolean; alt?: string } = $props();
  let src = $state(untrack(()=>(preview ? previewCache : thumbCache).get((preview ? 'p:' : '') + id) ?? '')), error = $state(''), visible = $state(false);
  function observe(node: HTMLElement) { const observer = new IntersectionObserver(entries => { if (entries.some(e => e.isIntersecting)) { visible = true; observer.disconnect(); } }, { rootMargin:'200px' }); observer.observe(node); return { destroy: () => observer.disconnect() }; }
  $effect(() => {
    const asset = id, large = preview;
    const key = (large ? 'p:' : '') + asset;
    // Previews go through the bounded LRU (8) shared with neighbour
    // preloading: previous/next navigation without re-decoding, without
    // an unbounded RAM cache.
    const cache = large ? previewCache : thumbCache;
    const cached = cache.get(key); if (cached) { src = cached; error = ''; return; }
    if (!visible && !large) { src = ''; error = ''; return; }
    const controller = new AbortController(); src = ''; error = '';
    imageSource(asset, large, controller.signal).then(value => { if (!controller.signal.aborted) { cache.set(key, value); src = value; } }).catch(reason => { if (!controller.signal.aborted) error = message(reason); });
    return () => controller.abort();
  });
</script>
{#if src}<img {src} alt={alt ?? $t("Image de la bibliothèque")} class:preview />{:else}<div use:observe class="image-placeholder" title={error}>{error ? $t("Image indisponible") : $t("Chargement…")}</div>{/if}
