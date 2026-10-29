<script lang="ts">
  import { imageSource } from '../lib/api';
  // Portrait when its file is readable, otherwise the name's initials: never an error box in a list row.
  let { id = null, name, size = 44 }: { id?: string | null; name: string; size?: number } = $props();
  let src = $state('');
  const initials = $derived(name.split(/[\s-]+/).filter(Boolean).slice(0, 2).map(w => w[0]?.toLocaleUpperCase()).join('') || '?');
  const hue = $derived([...name].reduce((h, ch) => (h * 31 + ch.charCodeAt(0)) % 360, 7));
  $effect(() => { const asset = id; src = ''; if (!asset) return; const controller = new AbortController(); imageSource(asset, false, controller.signal).then(v => { if (!controller.signal.aborted) src = v; }).catch(() => {}); return () => controller.abort(); });
</script>
<span class="avatar" style="width:{size}px;height:{size}px;--hue:{hue}" aria-hidden="true">{#if src}<img {src} alt="" />{:else}{initials}{/if}</span>
<style>
  .avatar{flex:none;display:inline-flex;align-items:center;justify-content:center;border-radius:50%;overflow:hidden;font-size:14px;font-weight:600;color:var(--text);background:hsl(var(--hue) 28% 28%);border:1px solid hsl(var(--hue) 30% 42%)}
  img{width:100%;height:100%;object-fit:cover;object-position:top}
</style>
