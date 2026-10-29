<script lang="ts">
  import { t } from '../lib/i18n';
  import PassComparison from './PassComparison.svelte';
  import {phaseRows} from '../lib/generation-state';
  let {run}:{run:any}=$props();
  const phases=$derived(phaseRows(run));
  const pairs=$derived(phases.slice(1).map((after,i)=>({before:phases[i],after})).filter(p=>p.before.asset&&p.after.asset));
</script>
<section class="phase-comparisons" data-phase-comparisons>
  <h2>{$t("Avant / après · chaque passe")}</h2>
  <div class="comparisons-grid">{#each pairs as pair (String(run.id)+':'+pair.after.n)}
    <article><h3>{$t("Passe {before} → passe {after}",{before:pair.before.n,after:pair.after.n})}</h3><PassComparison before={pair.before.asset} after={pair.after.asset} beforeLabel={$t("Avant · passe {n}",{n:pair.before.n})} afterLabel={$t("Après · passe {n}",{n:pair.after.n})}/></article>
  {:else}<p class="empty">{$t("Les comparaisons apparaissent ici dès que deux passes successives ont produit leur image.")}</p>{/each}</div>
</section>
<style>
.phase-comparisons{min-width:0;container-type:inline-size}
.phase-comparisons h2{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);margin:0 0 12px}
.phase-comparisons h3{font-size:12px;font-weight:500;margin:0 0 6px;color:var(--text)}
.comparisons-grid{display:grid;grid-template-columns:1fr;gap:16px}.comparisons-grid article{min-width:0}
.empty{color:var(--faint);margin:0}
@container (min-width:460px){.comparisons-grid{grid-template-columns:repeat(2,minmax(0,1fr))}}
@container (min-width:760px){.comparisons-grid{grid-template-columns:repeat(3,minmax(0,1fr))}}
@container (min-width:1200px){.comparisons-grid{grid-template-columns:repeat(4,minmax(0,1fr))}}
</style>
