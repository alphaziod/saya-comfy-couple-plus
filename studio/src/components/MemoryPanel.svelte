<script lang="ts">
  import { t } from '../lib/i18n';
  import { call, message, native } from '../lib/api';
  let { run }: { run?: string } = $props();
  interface Snapshot { sayaRss:number|null; comfyPid:number|null; comfyRss:number|null; systemRamFree:number|null; devices:{name:string;vramTotal:number;vramFree:number;torchVramTotal:number|null;torchVramFree:number|null}[]; gpu:{card:string;vramUsed:number;vramTotal:number}[]; engineError:string|null }
  let snapshot = $state<Snapshot|null>(null), report = $state<{released:boolean;before:Snapshot;after:Snapshot}|null>(null);
  let busy = $state(false), confirmed = $state(false), error = $state('');
  const mib = (v:number|null|undefined) => v == null ? $t("indisponible") : `${(v / 1048576).toFixed(1)} Mio`;
  async function measure() { if(busy)return;busy=true;error='';try{snapshot=await call<Snapshot>('memory_status');}catch(e){error=message(e);}finally{busy=false;} }
  async function release() {
    if (!run || !confirmed || busy) return;
    const target=run; busy=true;error='';report=null;
    try{report=await call('release_pipeline_memory',{run:target,confirmed:true});snapshot=report!.after;}catch(e){error=message(e);}finally{confirmed=false;busy=false;}
  }
</script>
<details class="sec mem" data-memory-panel>
  <summary title={$t("Mesures locales. La VRAM globale inclut les autres applications. Une commande de déchargement ne prouve pas une libération.")}><span class="sec-title">{$t("RAM et VRAM")}</span>{#if snapshot}<span class="chip num">{mib(snapshot.sayaRss)}</span>{/if}</summary>
  <div class="sec-body">
    <div class="mem-actions"><button data-memory-measure disabled={busy || !native} onclick={measure}>{$t("Mesurer la mémoire")}</button></div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if snapshot}
      <div class="mem-lines num" data-memory-snapshot>
        <p>{$t("RAM Saya Studio : {saya} · RAM processus moteur {pid} : {engine}",{saya:mib(snapshot.sayaRss),pid:snapshot.comfyPid ?? $t("inconnu"),engine:mib(snapshot.comfyRss)})}</p>
        <p>{$t("RAM système disponible : {size}",{size:mib(snapshot.systemRamFree)})}</p>
        {#each snapshot.devices as d}<p>{d.name} · {$t("VRAM globale utilisée : {used} / {total} · réserve Torch : {reserved} · réserve Torch inutilisée : {unused}",{used:mib(d.vramTotal-d.vramFree),total:mib(d.vramTotal),reserved:mib(d.torchVramTotal),unused:mib(d.torchVramFree)})}</p>{/each}
        {#each snapshot.gpu as g}<p>{g.card} · {$t("VRAM noyau, tous processus : {used} / {total}",{used:mib(g.vramUsed),total:mib(g.vramTotal)})}</p>{/each}
        {#if !snapshot.devices.length}<p class="muted">{$t("Aucune mesure VRAM moteur exploitable (CPU ou moteur indisponible).")}</p>{/if}
        {#if snapshot.engineError}<p class="error">{snapshot.engineError}</p>{/if}
      </div>
    {/if}
    {#if run}
      <div class="mem-release">
        <label class="checkbox" title={$t("Entre deux passes : les sorties doivent être enregistrées et vérifiées, le moteur au repos. Les modèles nécessaires à la prochaine passe seront rechargés par ComfyUI.")}><input type="checkbox" data-memory-confirm bind:checked={confirmed} disabled={busy} />{$t("Je confirme le déchargement des modèles ComfyUI pour cette exécution")}</label>
        <button data-memory-release disabled={!confirmed || busy || !native} onclick={release}>{$t("Décharger entre les passes")}</button>
      </div>
    {/if}
    {#if report}<p class="muted" role="status" data-memory-outcome>{report.released ? $t("Baisse mesurée des réservations Torch après déchargement.") : $t("Commande envoyée ; aucune libération mesurée des réservations Torch.")}</p><details class="raw"><summary>{$t("Mesures avant / après")}</summary><pre>{JSON.stringify(report,null,2)}</pre></details>{/if}
  </div>
</details>

<style>
  .mem{margin:0}
  .mem-actions{display:flex;gap:8px}
  .mem-lines{display:grid}
  .mem-lines p{margin:0;padding:4px 0;border-top:1px solid var(--line);font-size:12px;overflow-wrap:anywhere}
  .mem-release{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
  .mem-release .checkbox{color:var(--text);font-size:12.5px}
  p{margin:0}
  .raw{margin:0}.raw summary{color:var(--muted);font-size:12px}
  .raw pre{background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:8px;max-height:280px}
</style>
