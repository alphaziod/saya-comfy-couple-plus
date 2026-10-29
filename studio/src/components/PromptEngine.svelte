<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message, native } from '../lib/api';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from './Icon.svelte';
  interface ExposedInput {instancePath:string[];input:string;type:string;status:string;reason:string;source:{node:string;slot:number}|null;targets:{node:string;input:string;slot:number}[]}
  interface Result { result:Record<string,unknown>&{exposedInputs?:ExposedInput[]}; sourceSha256:string; transport:string }
  interface Proposal { id:string; decision:string; createdAt:string; response:Result }
  let root=$state(''), prompt=$state(''), checkpoint=$state(''), target=$state('P1'), loras=$state('[]');
  let busy=$state(false),error=$state(''),note=$state(''), result=$state<Result|null>(null), proposalId=$state(''), confirmed=$state(false),history=$state<Proposal[]>([]);
  let workflow=$state(''), inspection=$state<Result|null>(null);
  async function chooseRoot(){const picked=await invoke<string[]>('pick_directories');if(picked?.length){root=picked[0];invalidate();}}
  function invalidate() { result=null; proposalId=''; confirmed=false; }
  async function refresh() { history=await call<Proposal[]>('proposals'); }
  async function run(tool:string) {
    if(busy)return;busy=true; error=''; note=''; invalidate();
    try {
      await call('set_preference',{key:'promptEngineRoot',value:root});
      const args=tool==='optimize'?{prompt,checkpoint,target,loras:JSON.parse(loras)}:{prompt};
      if(tool==='optimize') { const value=await call<{proposalId:string;response:Result}>('engine_tool',{root,tool,args}); result=value.response;proposalId=value.proposalId; await refresh(); }
      else result=await call<Result>('engine_tool',{root,tool,args});
    } catch(e) { error=message(e); } finally { busy=false; }
  }
  async function inspect() { if(busy)return;busy=true; error=''; inspection=null; try { inspection=await call<Result>('engine_tool',{root,tool:'inspect',args:{graph:JSON.parse(workflow)}}); } catch(e) { error=message(e); } finally { busy=false; } }
  async function file(event:Event) { const file=(event.target as HTMLInputElement).files?.[0]; if(!file)return;if(file.size>15_000_000){error=$t("Workflow trop volumineux (15 Mo maximum)");return;}try{workflow=await file.text();inspection=null;}catch(e){error=message(e);} }
  async function decide(decision:string) {
    if(!proposalId || !confirmed)return;if(busy)return;busy=true;error='';note='';
    try { prompt=await call<string>('decide_proposal',{id:proposalId,decision,confirmed,current:prompt}); invalidate(); note=decision==='accept'?$t("Proposition appliquée dans l’éditeur"):$t("Proposition rejetée"); await refresh(); }
    catch(e) { error=message(e); } finally { busy=false; }
  }
  async function review(item:Proposal) { if(busy)return;busy=true; error=''; invalidate(); try { const detail=await call<Proposal>('proposal',{id:item.id}); result=detail.response; proposalId=detail.decision==='pending'?detail.id:''; } catch(e) { error=message(e); } finally { busy=false; } }
  onMount(()=>{if(native) {call<Record<string,unknown>>('preferences').then(v=>root=String(v.promptEngineRoot??'')).catch(e=>error=message(e));refresh().catch(e=>error=message(e));}});
</script>
<div class="agents" data-agents-api>
 <section class="agents-intro">
  <strong>{$t("Agents & API")}</strong>
  <p>{$t("C’est le point d’entrée des agents et des scripts qui passent par l’API de l’app pour préparer des prompts et des scripts de test.")}</p>
  <p>{$t("Vous n’avez rien à faire ici au quotidien.")}</p>
  <p>{$t("Les réglages se trouvent dans le fichier AGENTS.md, à la racine du projet.")}</p>
 </section>
 <details class="agents-tools" data-agents-tools><summary>{$t("Outils avancés (pour les agents)")}</summary>
<div class="pe" data-prompt-engine>
<fieldset class="plain" disabled={busy || !native}>
<div class="pe-grid">
 <section class="pe-editor">
  <div class="wb-bar">
   <span class="bar-title">{$t("Prompt")}</span><span class="chip num">{prompt.length}</span>
   <span class="grow">{#if busy}<span class="muted" role="status" aria-live="polite">{$t("Traitement en cours… Les commandes seront disponibles à la fin.")}</span>{/if}</span>
   <button disabled={!root || !prompt.trim()} onclick={()=>run('optimize')}>{$t("Proposer une modification")}</button>
   <button class="primary" disabled={!root || !prompt.trim()} onclick={()=>run('analyze')}>{$t("Analyser")}</button>
  </div>
  {#if error||note}<div class="msgs">{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}</div>{/if}
  <label class="prompt-field"><span class="sr">{$t("Prompt complet exact")}</span><textarea spellcheck="false" bind:value={prompt} oninput={invalidate}></textarea></label>
  {#if result && typeof result.result.original_prompt==='string' && typeof result.result.full_prompt==='string'}
  <section class="compare">
   <div class="cmp-head"><h3>{$t("Comparer avant application")}</h3></div>
   <div class="ba">
    <label>{$t("Avant")}<textarea readonly rows="6" value={result.result.original_prompt}></textarea></label>
    <label>{$t("Proposition")}<textarea readonly rows="6" value={result.result.full_prompt}></textarea></label>
   </div>
   {#if proposalId}<div class="decide"><label class="check"><input type="checkbox" bind:checked={confirmed} />{$t("J’ai comparé les textes et confirme ma décision.")}</label><span class="grow"></span><button disabled={!confirmed} onclick={()=>decide('reject')}>{$t("Rejeter la proposition")}</button><button disabled={!confirmed} onclick={()=>decide('accept')}>{$t("Accepter et appliquer dans cet éditeur")}</button></div>{/if}
  </section>
  {:else if result && proposalId}
  <section class="compare"><div class="decide"><label class="check"><input type="checkbox" bind:checked={confirmed} />{$t("J’ai comparé les textes et confirme ma décision.")}</label><span class="grow"></span><button disabled={!confirmed} onclick={()=>decide('reject')}>{$t("Rejeter la proposition")}</button><button disabled={!confirmed} onclick={()=>decide('accept')}>{$t("Accepter et appliquer dans cet éditeur")}</button></div></section>
  {/if}
 </section>

 <aside class="wb-panel right">
  <header class="wb-head"><h2>{$t("Assistant local")}</h2><span class="hint" title={$t("Choisissez le dossier d’installation de confiance du Saya Prompt Engine. Python 3 exécute uniquement ses fonctions d’analyse de textes et de graphes. Grok et le serveur MCP ne sont pas contactés. Une acceptation ci-dessous valide une modification de texte, pas sa qualité visuelle.")}><Icon name="info" size={14} /></span></header>
  <div class="wb-scroll">
   <details class="sec" open={!root}><summary><span class="sec-title">{$t("Réglages")}</span>{#if root}<span class="chip ell" title={root}>{root.split('/').filter(Boolean).pop()}</span>{/if}</summary>
    <div class="sec-body">
     <label class="stack">{$t("Dossier Saya Prompt Engine (contenant src/saya_prompt_engine)")}<span class="with-btn"><input bind:value={root} oninput={invalidate} /><button class="icon-btn" onclick={chooseRoot} title={$t("Dossier…")} aria-label={$t("Dossier…")}><Icon name="folder" size={14} /></button></span></label>
    </div>
   </details>
   <details class="sec" open><summary><span class="sec-title">{$t("Proposer une modification")}</span></summary>
    <div class="sec-body">
     <label class="field-row"><span>{$t("Section cible")}</span><select bind:value={target} onchange={invalidate}>{#each ['QUALITY','P1','P2','ACTION','BG','NEG'] as section}<option>{section}</option>{/each}</select></label>
     <label class="stack">{$t("Checkpoint déclaré")}<input bind:value={checkpoint} oninput={invalidate} /></label>
     <label class="stack" title={$t("Exemple : [&#123;\"name\":\"nom\", \"documented_triggers\":[&#123;\"tag\":\"trigger exact\",\"source\":\"documentation vérifiée\"&#125;]&#125;]. Seuls ces triggers documentés peuvent être proposés.")}>{$t("LoRA documentées (JSON)")}<textarea class="mono" rows="3" bind:value={loras} oninput={invalidate}></textarea></label>
    </div>
   </details>
   {#if result}
   <details class="sec" open><summary><span class="sec-title">{$t("Détails du diagnostic et des modifications")}</span></summary>
    <div class="sec-body"><pre>{JSON.stringify(result.result,null,2)}</pre><p class="note hash">{$t("Source locale SHA-256 : {sha}",{sha:result.sourceSha256})}</p></div>
   </details>
   {/if}
   <details class="sec"><summary><span class="sec-title">{$t("Historique local · 50 dernières propositions")}</span><span class="chip num">{history.length}</span></summary>
    <div class="hist">{#each history as item}<button class="hrow" onclick={()=>review(item)}><span class="num">{item.createdAt}</span><span class="chip">{item.decision}</span></button>{/each}</div>
   </details>
   <details class="sec"><summary title={$t("Lecture seule. Valeurs sauvegardées et connexions non résolues sont distinguées des données d’exécution ; aucune conversion en requête GPU.")}><span class="sec-title">{$t("Inspecter un workflow API / UI et ses subgraphs")}</span></summary>
    <div class="sec-body">
     <label class="stack">{$t("Fichier JSON")}<input type="file" accept=".json,application/json" onchange={file} /></label>
     <label class="stack">{$t("JSON")}<textarea class="mono" rows="5" bind:value={workflow} oninput={()=>inspection=null}></textarea></label>
     <button disabled={!root || !workflow} onclick={inspect}>{$t("Inspecter sans importer ni exécuter")}</button>
     {#if inspection}
     {#if inspection.result.exposedInputs?.length}<section class="exposed" data-exposed-inputs><h3 title={$t("Chaque chemin désigne une instance distincte. Le câblage sauvegardé peut être vérifié sans connaître la valeur exécutée. Modification désactivée tant que cette valeur et son mapping API ne sont pas prouvés.")}>{$t("Entrées exposées · {n}",{n:inspection.result.exposedInputs.length})}</h3>
     {#each inspection.result.exposedInputs as input}<article><h4>{input.instancePath.join(' / ')} · {input.input} ({input.type})</h4><p class="note">{input.reason}</p>{#if input.source}<p class="note">{$t("Source : {node} · sortie {slot}",{node:input.source.node,slot:input.source.slot})}</p>{/if}{#if input.targets.length}<p class="note">{$t("Cibles : {list}",{list:input.targets.map(target=>`${target.node}.${target.input}`).join(', ')})}</p>{/if}</article>{/each}</section>{/if}
     <details class="inline"><summary>{$t("Diagnostic complet")}</summary><pre>{JSON.stringify(inspection.result,null,2)}</pre></details>{/if}
    </div>
   </details>
  </div>
 </aside>
</div>
</fieldset>
</div>
 </details>
</div>

<style>
.agents{height:100%;overflow:auto;padding:24px;display:flex;flex-direction:column;gap:16px;box-sizing:border-box}
.agents-intro{display:grid;gap:6px;max-width:640px}
.agents-intro p{margin:0;color:var(--muted);line-height:1.5}
.agents-tools>summary{cursor:pointer;color:var(--muted);font-size:12px}
.agents-tools .pe{height:640px;margin-top:12px;border:1px solid var(--line);border-radius:var(--radius)}
.pe{height:100%;min-height:480px;overflow:hidden}
fieldset.plain{border:0;margin:0;padding:0;min-width:0;height:100%}
.pe-grid{display:grid;grid-template-columns:minmax(0,1fr) 360px;height:100%;min-height:0}
.pe-editor{display:flex;flex-direction:column;min-width:0;min-height:0;background:var(--bg)}
h2{font-size:13px;font-weight:600;margin:0}
h3{font-size:12px;font-weight:600;margin:0}
h4{font-size:12px;font-weight:600;margin:0}
label{color:var(--muted);font-size:12px}
.stack{display:flex;flex-direction:column;gap:4px}
.field-row{display:grid;grid-template-columns:96px minmax(0,1fr);align-items:center;gap:8px}
.check{display:flex;flex-direction:row;align-items:center;gap:8px;color:var(--text)}
.with-btn{display:flex;gap:4px}.with-btn input{flex:1;min-width:0}
.bar-title{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted)}
.grow{flex:1;min-width:0}
.muted{color:var(--muted)}.ell{max-width:140px;overflow:hidden;text-overflow:ellipsis}
.hint{color:var(--faint);display:inline-grid;cursor:help}
.note{color:var(--muted);margin:0;overflow-wrap:anywhere;font-size:12px}
.hash{font-size:11px;color:var(--faint)}
.error{color:var(--danger);margin:0;overflow-wrap:anywhere}
.msgs{display:grid;gap:4px;padding:8px 12px;border-bottom:1px solid var(--line)}
.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap}
.prompt-field{position:relative;flex:1;min-height:200px;display:flex}
.prompt-field textarea{flex:1;resize:none;border:0;border-radius:0;background:var(--canvas);padding:16px 18px;font-family:ui-monospace,"JetBrains Mono",Menlo,monospace;font-size:13px;line-height:1.6}
.prompt-field textarea:focus{outline:0}
.mono{font-family:ui-monospace,"JetBrains Mono",Menlo,monospace;font-size:11.5px}
.compare{flex:none;border-top:1px solid var(--line);padding:10px 12px;display:grid;gap:8px;max-height:50%;overflow:auto;background:var(--panel)}
.ba{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,260px),1fr));gap:10px}
.ba label{display:flex;flex-direction:column;gap:4px}
.ba textarea{font-family:ui-monospace,"JetBrains Mono",Menlo,monospace;font-size:12px;resize:vertical}
.decide{display:flex;flex-wrap:wrap;align-items:center;gap:6px}
details{margin:0}details.inline>summary{color:var(--muted);font-size:12px;cursor:pointer}
pre{font-size:11px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:0;max-height:360px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}
.hist{display:grid;padding:0 0 8px}
.hrow{display:flex;align-items:center;justify-content:space-between;gap:8px;border:0;border-bottom:1px solid var(--line);border-radius:0;background:transparent;padding:0 12px;text-align:start}
.hrow:hover:not(:disabled){background:var(--hover)}
.exposed{display:grid;gap:6px}
.exposed article{border-top:1px solid var(--line);padding-top:6px;display:grid;gap:2px;overflow-wrap:anywhere}
@media (max-width:980px){.pe{height:auto}.pe-grid{grid-template-columns:minmax(0,1fr)}.prompt-field{min-height:320px}}
</style>
