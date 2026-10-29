<script lang="ts">
  import { t } from '../lib/i18n';
  import { call, message } from '../lib/api';
  let {workflows,oncreated}:{workflows:{id:string;name:string}[];oncreated:(id:string)=>void}= $props();
  type Boundary={node:string;slot:number};
  type Node={id:string;class:string;category:string;parameters:string;finalOutput:boolean;outputs:(Boundary&{type:string;supported:boolean})[]};
  type Analysis={sourceSha256:string;nodes:Node[]};
  type Preview={previewHash:string;plan:{transport:string;excludedNodes:string[];passes:{step:{id:string;inputs:unknown[];outputs:unknown[];bindings:unknown[]};apiJson:string;nodeCount:number}[]}};
  let workflow=$state(''),name=$state('Workflow en passes'),analysis=$state<Analysis|null>(null);
  let cuts=$state<Boundary[]>([]),candidate=$state(''),output=$state(''),preview=$state<Preview|null>(null);
  let busy=$state(false),error=$state(''),note=$state(''),confirmed=$state(false);
  const ports=$derived(analysis?.nodes.flatMap(n=>n.outputs.map(p=>({...p,label:n.id+' · '+n.class+' ['+p.slot+'] · '+p.type})))??[]);
  function invalidate(){preview=null;confirmed=false;}
  async function analyze(){if(busy)return;busy=true;error='';note='';invalidate();cuts=[];output='';analysis=null;try{analysis=await call('split_analysis',{workflow});}catch(e){error=message(e)}finally{busy=false}}
  function add(){const p=ports.find(p=>p.node+':'+p.slot===candidate);if(p&&!cuts.some(c=>c.node===p.node&&c.slot===p.slot)){cuts=[...cuts,{node:p.node,slot:p.slot}];invalidate()}}
  async function inspect(){if(busy)return;busy=true;error='';invalidate();try{preview=await call('split_preview',{workflow,cuts,output})}catch(e){error=message(e)}finally{busy=false}}
  async function save(){if(!preview||!confirmed)return;if(busy)return;busy=true;error='';try{const result=await call<{pipeline:string}>('split_save',{workflow,cuts,output,name,previewHash:preview.previewHash,confirmed});note=$t("Passes et pipeline enregistrés. Aucune génération lancée.");oncreated(result.pipeline);invalidate()}catch(e){error=message(e)}finally{busy=false}}
</script>
<details class="sec ws" data-workflow-split>
  <summary title={$t("Choisissez les frontières dans l’ordre du graphe. Transport IMAGE via PNG uniquement. Modèles, VAE et conditionings sont reconstruits dans chaque passe ; les états internes du sampler ne sont pas transmis.")}><span class="sec-title">{$t("Préparer les passes d’un workflow")}</span></summary>
  <div class="sec-body">
    <label>{$t("Workflow API qualifié")}<select data-split-workflow bind:value={workflow} disabled={busy} onchange={()=>{analysis=null;invalidate()}}><option value="">{$t("— choisir —")}</option>{#each workflows as w}<option value={w.id}>{w.name}</option>{/each}</select></label>
    <button data-split-analyze disabled={busy||!workflow} onclick={analyze}>{$t("Analyser les dépendances")}</button>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if note}<p class="notice" role="status">{note}</p>{/if}
    {#if analysis}
      <details class="ws-sub"><summary>{$t("Graphe source : {n} nodes · SHA-256 {sha}",{n:analysis.nodes.length,sha:analysis.sourceSha256})}</summary>
        {#each analysis.nodes as n}<details class="ws-sub"><summary>#{n.id} {n.class} · {n.category}</summary><pre>{n.parameters}</pre></details>{/each}
      </details>
      <label>{$t("Frontière après une sortie")}<select data-split-boundary bind:value={candidate} disabled={busy}><option value="">{$t("— choisir —")}</option>{#each ports as p}<option value={p.node+':'+p.slot} disabled={!p.supported}>{p.label}{p.supported?'':' · '+$t("transport refusé")}</option>{/each}</select></label>
      <button data-split-add disabled={busy||!candidate||cuts.length>=31} onclick={add}>{$t("Ajouter cette frontière")}</button>
      {#if cuts.length}<ol class="ws-cuts">{#each cuts as c,i}<li><span class="num">{$t("Après #{node}[{slot}] · IMAGE",{node:c.node,slot:c.slot})}</span> <button class="quiet" disabled={busy} onclick={()=>{cuts=cuts.filter((_,j)=>j!==i);invalidate()}}>{$t("Retirer")}</button></li>{/each}</ol>{/if}
      <label>{$t("Sortie finale")}<select data-split-output bind:value={output} disabled={busy} onchange={invalidate}><option value="">{$t("— choisir SaveImage —")}</option>{#each analysis.nodes.filter(n=>n.finalOutput) as n}<option value={n.id}>#{n.id} {n.class}</option>{/each}</select></label>
      <button data-split-preview disabled={busy||!output} onclick={inspect}>{$t("Prévisualiser les passes")}</button>
    {/if}
    {#if preview}
      <section class="ws-plan" data-split-plan>
        <p class="muted">{preview.plan.transport}</p>
        <p class="muted">{$t("Nodes hors de la sortie choisie : {list}. Ces branches restent dans l’original.",{list:preview.plan.excludedNodes.join(', ')||$t("aucun")})}</p>
        {#each preview.plan.passes as p}<details class="ws-sub" open><summary>{p.step.id} · {$t("{n} nodes",{n:p.nodeCount})}</summary><p class="muted">{$t("Entrées / sorties / liaisons : {json}",{json:JSON.stringify(p.step)})}</p><details class="ws-sub"><summary>{$t("API complète et paramètres exacts")}</summary><pre>{p.apiJson}</pre></details></details>{/each}
        <label>{$t("Nom du pipeline")}<input data-split-name bind:value={name} disabled={busy}/></label>
        <label class="checkbox"><input type="checkbox" data-split-confirm bind:checked={confirmed} disabled={busy}/>{$t("Je confirme ces frontières et les branches exclues. Créer de nouvelles copies API et leur pipeline.")}</label>
        <button data-split-save disabled={busy||!confirmed||!name.trim()} onclick={save}>{$t("Enregistrer les passes")}</button>
      </section>
    {/if}
  </div>
</details>
<style>
  .ws .sec-body>label,.ws-plan>label:not(.checkbox){gap:3px;color:var(--muted);font-size:12px}
  .ws select,.ws input:not([type=checkbox]){width:100%;color:var(--text)}
  .ws .checkbox{flex-direction:row;align-items:flex-start;gap:6px;font-size:12px;color:var(--text)}
  .ws p{margin:0;font-size:12px;overflow-wrap:anywhere}
  .ws-sub{margin:0;font-size:12px}
  .ws-sub>summary{color:var(--muted);cursor:pointer;overflow-wrap:anywhere}
  .ws-sub .ws-sub{margin-inline-start:10px}
  .ws-plan{display:grid;gap:8px}
  .ws-cuts{margin:0;padding-inline-start:18px;font-size:12px}
  .ws-cuts button{min-height:22px;font-size:11px}
  pre{max-height:320px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;font-size:11px;margin:4px 0}
</style>
