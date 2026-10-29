<script lang="ts">
  import {untrack} from 'svelte';
  import { t } from '../lib/i18n';
  import { imageSource, message } from '../lib/api';
  import { thumbCache } from '../lib/stream';
  let { id, preview = false, alt }: { id: string; preview?: boolean; alt?: string } = $props();
  let src = $state(untrack(()=>thumbCache.get((preview ? 'p:' : '') + id) ?? '')), error = $state(''), visible = $state(false);
  function observe(node: HTMLElement) { const observer = new IntersectionObserver(entries => { if (entries.some(e => e.isIntersecting)) { visible = true; observer.disconnect(); } }, { rootMargin: '600px' }); observer.observe(node); return { destroy: () => observer.disconnect() }; }
  $effect(() => {
    const asset = id, large = preview;
    const key = (large ? 'p:' : '') + asset;
    const cached = thumbCache.get(key);
    if (cached) { src = cached; error = ''; return; }
    if (!visible && !large) { src = ''; error = ''; return; }
    const controller = new AbortController();
    src = ''; error = '';
    imageSource(asset, large, controller.signal).then(value => { if (!controller.signal.aborted) { thumbCache.set(key, value); src = value; } }).catch(reason => { if (!controller.signal.aborted) error = message(reason); });
    return () => controller.abort();
  });
</script>
{#if src}<img {src} alt={alt ?? $t("Miniature")} />{:else}<div use:observe class="image-placeholder" title={error}>{error ? $t("Image indisponible") : ''}</div>{/if}
