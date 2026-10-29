<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import MemoryPanel from './MemoryPanel.svelte';
  import { onMount } from 'svelte';
  import LiveJobs from './LiveJobs.svelte';
  import { call, message, native } from '../lib/api';
  interface QueueEntry { number:number; promptId:string }
  interface Status { connected:boolean; version:string|null; running:number|null; pending:number|null; error:string|null; queue:{running:QueueEntry[];pending:QueueEntry[]}|null }
  let status = $state<Status | null>(null), loading = $state(false), error = $state(''), observed = $state('');
  let models = $state<{checkpoints:string[];loras:string[];nodeClasses:number}|null>(null), modelsBusy = $state(false), search = $state('');
  let alive = true;
  async function refresh() { if (loading || !native) return; loading = true; try { const value = await call<Status>('connection'); if (alive) { status = value; observed = new Date().toLocaleTimeString(); error = ''; } } catch(e) { if (alive) { status = null; error = message(e); } } finally { loading = false; } }
  async function loadModels() { modelsBusy = true; error = ''; try { const value = await call<typeof models>('models'); if (alive) models = value; } catch(e) { if (alive) { models = null; error = message(e); } } finally { modelsBusy = false; } }
  onMount(() => { alive = true; refresh(); const timer = setInterval(refresh,5000); return () => { alive = false; clearInterval(timer); }; });
</script>
<div class="sys-page engine-page">
  <header class="sys-head">
    <span class="dot" class:on={status?.connected} class:off={status && !status.connected}></span>
    <strong role="status">{status?.connected ? $t("Connecté · version {version}", { version: status.version ?? $t("non indiquée") }) : status ? $t("Connexion non établie") : $t("État non vérifié")}</strong>
    <span class="chip num">{$t("En cours · {n}", { n: status?.running ?? '—' })}</span>
    <span class="chip num">{$t("En attente · {n}", { n: status?.pending ?? '—' })}</span>
    <span class="grow"></span>
    {#if observed}<span class="muted num" title={$t("État actualisé toutes les 5 secondes. Suivez vos passes Saya dans Génération et les requêtes API dans Workflows.")}>{$t("observation à {time}", { time: observed })}</span>{/if}
    <button class="quiet" disabled={loading || !native} onclick={refresh}>{loading ? $t("Vérification…") : $t("Actualiser")}</button>
  </header>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if status?.error}<p class="error">{status.error}</p>{/if}
  <div class="live"><LiveJobs /></div>

  <section class="sys-sec">
    <h2 class="sys-title">{$t("File d’attente du moteur")}</h2>
    {#each status?.queue?.running ?? [] as item}<div class="row"><span class="state run">{$t("En cours")}</span><code>{item.promptId}</code><span class="muted num">{$t("position {n}", { n: item.number })}</span></div>{/each}
    {#each status?.queue?.pending ?? [] as item}<div class="row"><span class="state">{$t("En attente")}</span><code>{item.promptId}</code><span class="muted num">{$t("position {n}", { n: item.number })}</span></div>{/each}
    {#if !(status?.queue?.running?.length || status?.queue?.pending?.length)}<p class="muted empty">{$t("Aucune requête dans la file.")}</p>{/if}
  </section>

  <section class="sys-sec">
    <div class="sec-row">
      <h2 class="sys-title">{$t("Modèles annoncés par le moteur")}</h2>
      {#if models}<span class="chip num">{$tn("{n} classe de nodes déclarée", "{n} classes de nodes déclarées", models.nodeClasses)}</span>{/if}
      <span class="grow"></span>
      <input class="filter" bind:value={search} placeholder={$t("Filtrer les noms")} aria-label={$t("Filtrer les noms")} />
      <button disabled={!native || modelsBusy} onclick={loadModels}>{modelsBusy ? $t("Lecture…") : $t("Lire les modèles disponibles")}</button>
    </div>
    {#if models}<div class="model-cols">
      <div><h3 class="col-title num">{$t("Checkpoints · {n}", { n: models.checkpoints.length })}</h3><div class="names">{#each models.checkpoints.filter(v => v.toLocaleLowerCase().includes(search.toLocaleLowerCase())) as name}<p class="name">{name}</p>{/each}</div></div>
      <div><h3 class="col-title num">{$t("LoRA · {n}", { n: models.loras.length })}</h3><div class="names">{#each models.loras.filter(v => v.toLocaleLowerCase().includes(search.toLocaleLowerCase())) as name}<p class="name">{name}</p>{/each}</div></div>
    </div>{/if}
  </section>

  <MemoryPanel />
</div>

<style>
  .sys-page{max-width:1100px;display:grid;gap:0}
  .sys-head{display:flex;align-items:center;gap:8px;flex-wrap:wrap;min-height:36px;padding-bottom:8px;border-bottom:1px solid var(--line)}
  .sys-head strong{font-size:13.5px;font-weight:600}
  .grow{flex:1}
  .dot{width:8px;height:8px;border-radius:50%;background:var(--faint);flex:none}.dot.on{background:var(--ok)}.dot.off{background:var(--danger)}
  .live{padding:6px 0;border-bottom:1px solid var(--line);color:var(--muted)}
  .live :global(p){margin:2px 0;font-size:12px;color:var(--muted)}
  .live :global(.notice){background:none;border:0;padding:0}
  .sys-sec{padding:12px 0;border-bottom:1px solid var(--line)}
  .sys-title{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);margin:0 0 6px}
  .sec-row{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:6px}.sec-row .sys-title{margin:0}
  .row{display:grid;grid-template-columns:90px minmax(0,1fr) auto;align-items:center;gap:10px;min-height:30px;border-top:1px solid var(--line)}
  .row:first-of-type{border-top:0}
  .row code{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:12px}
  .state{font-size:11px;color:var(--muted)}.state.run{color:var(--accent)}
  .empty{margin:4px 0}
  .filter{width:220px}
  .model-cols{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:16px}
  .col-title{font-size:12px;font-weight:600;margin:4px 0}
  .names{max-height:320px;overflow:auto;border-top:1px solid var(--line)}
  .name{margin:0;padding:3px 0;border-bottom:1px solid var(--line);font-size:12px;overflow-wrap:anywhere}
  .error{margin:6px 0}
  .engine-page :global(.mem>summary){padding:0}
  .engine-page :global(.mem .sec-body){padding:2px 0 12px}
  @media(max-width:700px){.model-cols{grid-template-columns:1fr}.filter{width:100%}}
</style>
