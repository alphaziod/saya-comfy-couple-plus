<script lang="ts">
  import { t } from '../lib/i18n';
  import { stateLabel } from '../lib/state-label';
  import { onMount } from 'svelte';
  import { call, message, native, type Job, type Character, type LiveObservation } from '../lib/api';
  import Thumbnail from './Thumbnail.svelte';
  import {identityText,applyIdentityPreset,composePrompt} from '../lib/identity-preset';
  import Couple from './Couple.svelte';
  import LiveJobs from './LiveJobs.svelte';
  import JobControls from './JobControls.svelte';
  import Checkpoints from './Checkpoints.svelte';
  import Icon from './Icon.svelte';
  interface Field { nodeId:string; inputName:string; classType:string; valueType:string; value:string; enabled:boolean; section:string|null }
  interface Variant {id:string;name:string;promptBlocks:Record<string,string>}
  interface Workflow { id:string; name:string; fields:Field[]; nodeCount:number }
  interface Prepared { jobId:string; snapshotHash:string; requestJson:string; removed:string[]; changes:unknown[]; nodeCount:number }
  let list = $state<{id:string;name:string;promptProfile:'solo'|'couple'|'unconfigured'}[]>([]), workflow = $state<Workflow|null>(null), text = $state(''), name = $state('');
  let prepared = $state<Prepared|null>(null), confirmed = $state(false), allowRemovals = $state(false), busy = $state(false), error = $state(''), note = $state(''), jobs = $state<Job[]>([]);
  let profileFilter=$state('');
  const visibleWorkflows=$derived(list.filter(item=>!profileFilter||item.promptProfile===profileFilter));
  let submissionError=$state<{jobId:string;message:string}|null>(null);
  $effect(()=>{const failure=submissionError;if(failure&&jobs.some(job=>job.id===failure.jobId&&['queued','running','awaiting_review','succeeded','failed','cancelled','interrupted'].includes(job.state)))submissionError=null;});
  let live=$state<LiveObservation|null>(null);
  let characters=$state<Character[]>([]), selectedPeople=$state(['','']),selectedVariants=$state(['','']),personVariants=$state<Variant[][]>([[],[]]);
  let baseline=$state<{fields:Field[];people:string[];variants:string[];options:Variant[][];append:boolean}|null>(null);
  let appendPreset=$state(false);
  let presetPreview=$state<{index:number;text:string;before:string;fingerprint:string}|null>(null);
  function fingerprint(){return JSON.stringify({fields:workflow?.fields,people:selectedPeople,variants:selectedVariants,appendPreset});}
  const profileLabel:Record<string,string>={solo:'Solo · mapping P1',couple:'Couple · mappings P1 et P2',unconfigured:'À configurer'};
  function when(value:string){if(value==='jamais')return value;const date=new Date(value);return Number.isNaN(date.getTime())?value:date.toLocaleDateString();}
  function lastUse(name:string){const job=jobs.find(j=>j.workflow===name);return job?job.createdAt:'jamais';}
  function checkpoint(){if(workflow)baseline=structuredClone($state.snapshot({fields:workflow.fields,people:selectedPeople,variants:selectedVariants,options:personVariants,append:appendPreset}));}
  function undo(){if(!workflow||!baseline)return;++serial;invalidate();const saved=structuredClone($state.snapshot(baseline));workflow.fields=saved.fields;selectedPeople=saved.people;selectedVariants=saved.variants;personVariants=saved.options;appendPreset=saved.append;error='';note=$t("Édition annulée localement : valeurs et variantes du début de l’édition ou de la dernière préparation réussie restaurées. Aucune écriture SQLite.");}
  const personSerial=[0,0];
  let serial = 0;
  let savedMapping=$state('');
  function mappingKey(fields:Field[]){return JSON.stringify(fields.filter(f=>f.enabled).map(f=>[f.nodeId,f.inputName,f.section]));}
  const sections = ['QUALITY','P1','P2','ACTION','BG','NEG'];
  function invalidate() { presetPreview=null; prepared = null; confirmed = false; allowRemovals = false; }
  async function refresh() { try { [list,jobs,characters] = await Promise.all([call<typeof list>('workflows'),call<Job[]>('jobs'),call<Character[]>('characters')]); } catch(e) { error = message(e); } }
  async function choose(id: string) { const ticket = ++serial; invalidate(); selectedPeople=['',''];selectedVariants=['',''];personVariants=[[],[]];appendPreset=false; baseline=null; workflow = null; error = ''; try { const value = await call<Workflow>('workflow',{id}); if (ticket === serial) {workflow = value;savedMapping=mappingKey(value.fields);checkpoint();} } catch(e) { if(ticket === serial) error = message(e); } }
  async function choosePerson(index:number){invalidate();selectedVariants[index]='';personVariants[index]=[];const person=selectedPeople[index],ticket=++personSerial[index],workflowTicket=serial;error='';try{if(person){const variants=await call<Variant[]>('variants',{id:person});if(ticket===personSerial[index]&&workflowTicket===serial)personVariants[index]=variants;}}catch(e){if(ticket===personSerial[index]&&workflowTicket===serial)error=message(e);}}
  function loadPerson(index:number){if(!workflow)return;error='';try{const variant=personVariants[index].find(v=>v.id===selectedVariants[index]);if(selectedVariants[index]&&!variant)throw new Error($t("Variante indisponible ; sélectionnez à nouveau le personnage."));const section=index===0?'P1':'P2',text=identityText(characters.find(c=>c.id===selectedPeople[index]),variant);applyIdentityPreset(workflow.fields,section,text);const before=workflow.fields.find(f=>f.enabled&&f.section===section)!.value;presetPreview={index,text:composePrompt(before,text,appendPreset),before,fingerprint:fingerprint()};}catch(e){error=message(e);}}
  function applyPreset(){if(!workflow||!presetPreview)return;const preview=presetPreview;if(preview.fingerprint!==fingerprint()){presetPreview=null;error=$t("Édition modifiée : comparez à nouveau le preset.");return;}workflow.fields=applyIdentityPreset(workflow.fields,preview.index===0?'P1':'P2',preview.text);invalidate();}
  async function discard(){if(!workflow)return;await choose(workflow.id);note=$t("Réinitialisation : valeurs de la copie importée rechargées ; mapping enregistré conservé. Associations et confirmation réinitialisées.");}
  async function importWorkflow() { if (busy) return; busy = true; error = ''; try { const id = await call<string>('import_workflow',{name,text}); profileFilter=''; await refresh(); await choose(id); note = $t("Copie importée. Le fichier original est conservé."); } catch(e) { error = message(e); } finally { busy = false; } }
  async function file(event: Event) { const value = (event.target as HTMLInputElement).files?.[0]; if (!value) return; if (value.size>16_000_000) { error = $t("Workflow supérieur à 16 Mo"); return; } error = ''; try { text = await value.text(); } catch(e) { error = $t("Lecture du fichier impossible : {error}", { error: message(e) }); return; } if (!name) name = value.name.replace(/\.json$/i,''); }
  async function prepare() { if (!workflow) return; if (busy) return; busy = true; error = ''; invalidate(); try { if(mappingKey(workflow.fields)!==savedMapping)throw new Error($t("Mapping modifié : enregistrez-le explicitement avant la préparation."));const fields = workflow.fields.filter(f => f.enabled).map(f => ({nodeId:f.nodeId,inputName:f.inputName,value:f.value,section:f.section||null})); prepared = await call<Prepared>('prepare',{id:workflow.id,edits:fields,characters:selectedPeople.flatMap((person,index)=>person?[{role:`P${index+1}`,characterId:person,variantId:selectedVariants[index]||null}]:[])}); checkpoint(); await refresh(); } catch(e) { error = message(e); } finally { busy = false; } }
  async function saveMapping() { if(!workflow) return; if(busy)return;busy=true; error=''; invalidate(); try { await call('save_bindings',{id:workflow.id,fields:workflow.fields.filter(f=>f.enabled)});savedMapping=mappingKey(workflow.fields); if(baseline)baseline.fields=workflow.fields.map(f=>({...f,value:baseline!.fields.find(old=>old.nodeId===f.nodeId&&old.inputName===f.inputName)?.value??f.value}));await refresh();note=$t("Mapping enregistré. Relire le mapping dans le panneau Saya Couple."); } catch(e) { error=message(e); } finally { busy=false; } }
  async function prepareCouple(input:Record<string,unknown>) { if(!workflow) return false; if(busy)return false;busy=true; error=''; invalidate(); try { prepared=await call<Prepared>('prepare_couple',{id:workflow.id,input}); await refresh();return true; } catch(e) { error=message(e);return false; } finally { busy=false; } }
  async function submit() { if (!prepared || !confirmed || (prepared.removed.length && !allowRemovals)) return; if (busy) return; busy = true; error = ''; const draft = prepared; const removalDecision = allowRemovals; note=''; submissionError=null; invalidate(); try { await call('submit',{job:draft.jobId,hash:draft.snapshotHash,confirmed:true,allow_removals:removalDecision}); note = $t("Requête transmise. Synchronisez pour lire son état réel."); } catch(e) { submissionError={jobId:draft.jobId,message:message(e)}; } finally { await refresh(); busy = false; } }
  async function sync() { if (busy) return; busy = true; error = ''; try { jobs = await call<Job[]>('sync_jobs'); } catch(e) { error = message(e); } finally { busy = false; } }
  onMount(() => { let alive=true,pending=false; if(native) refresh(); const timer=setInterval(async()=>{if(!native||pending)return;pending=true;try{const value=await call<Job[]>('jobs');if(alive)jobs=value;}catch(e){if(alive)error=message(e);}finally{pending=false;}},2000); return () => { alive=false;clearInterval(timer);++serial; }; });
</script>
<div class="wb wf" data-workflows-page>
<aside class="wb-panel left">
 <header class="wb-head"><h2>{$t("Workflows")}</h2><span class="chip num">{visibleWorkflows.length}</span><span class="hint" title={$t("Prise en charge : graphes API, entrées primitives et liens explicites. LoRA composites LoraManager conservées sans conversion ; les autres widgets composites et subgraphs UI exigent un adaptateur. La préparation ne charge pas les modèles ni les LoRA.")}><Icon name="info" size={14} /></span></header>
 <div class="wb-scroll">
  <details class="sec"><summary><span class="sec-title">{$t("Importer un workflow JSON API")}</span></summary>
   <form class="sec-body" onsubmit={e => {e.preventDefault();importWorkflow();}}>
    <label class="stack">{$t("Nom")}<input required maxlength="300" bind:value={name} /></label>
    <label class="stack">{$t("Fichier JSON")}<input type="file" accept=".json,application/json" onchange={file} /></label>
    <label class="stack">{$t("Document complet")}<textarea rows="6" bind:value={text}></textarea></label>
    <button disabled={!native || busy || !text}>{$t("Importer une copie")}</button>
   </form>
  </details>
  <div class="lib-filter" title={$t("Le profil décrit les champs d’identité enregistrés, pas le nombre de personnes dans le résultat. Filtrer la liste conserve le workflow ouvert et ses éditions. Le graphe sera vérifié avant préparation.")}>
   <select data-workflow-profile aria-label={$t("Choisir un profil de workflow")} disabled={busy} bind:value={profileFilter}><option value="">{$t("Tous les workflows")}</option><option value="solo">{$t("SOLO · mapping P1")}</option><option value="couple">{$t("COUPLE · mappings P1 et P2")}</option><option value="unconfigured">{$t("À configurer")}</option></select>
  </div>
  {#if visibleWorkflows.length}
  <table class="lib" data-workflow-library title={$t("Chaque entrée est une copie importée d’un workflow API ComfyUI ; l’original n’est jamais modifié. « Dernière utilisation » lit l’historique local (100 générations les plus récentes).")}>
   <thead><tr><th scope="col">{$t("Nom")}</th><th scope="col">{$t("Mapping")}</th><th scope="col" class="end">{$t("Dernière utilisation")}</th></tr></thead>
   <tbody>{#each visibleWorkflows as item (item.id)}<tr data-workflow-row={item.id} class:open={workflow?.id===item.id}>
    <td><button class="row-open" data-workflow-name={item.name} disabled={busy} aria-pressed={workflow?.id===item.id} onclick={() => choose(item.id)}>{item.name}</button></td>
    <td class="muted">{$t(profileLabel[item.promptProfile] ?? item.promptProfile)}</td>
    <td class="end num muted" data-workflow-last-use={item.id} title={lastUse(item.name)}>{when(lastUse(item.name))}</td>
   </tr>{/each}</tbody>
  </table>
  {:else if list.length}<p class="empty">{$t("Aucun workflow avec ce profil enregistré. Choisissez « Tous les workflows » pour configurer un mapping.")}</p>
  {:else}<p class="empty">{$t("Aucun workflow importé : utilisez « Importer un workflow JSON API » ci-dessus.")}</p>{/if}
 </div>
</aside>

<section class="wb-panel wf-main">
 {#if workflow}
 <header class="wb-head"><h2 title={workflow.name}>{workflow.name}</h2><span class="chip num">{$t("{n} nodes",{n:workflow.nodeCount})}</span></header>
 {/if}
 <div class="msgs">
  {#if error}<p class="error" role="alert">{error}</p>{/if}{#if submissionError}<p class="error" role="alert" data-submission-error>{$t("Envoi non confirmé pour {job} : {message}. Consultez l’état du job ; aucun renvoi automatique.",{job:submissionError.jobId,message:submissionError.message})}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}
 </div>
 <div class="wb-scroll">
 {#if workflow}
  <details class="sec" open><summary title={$t("Activer uniquement les entrées à adapter, puis enregistrer explicitement le mapping avant la préparation. Associer une section à une entrée texte distincte ; P1 et P2 restent indépendants. Les liens de conditioning ne sont pas éditables ici.")}><span class="sec-title">{$t("Mapping")}</span><span class="chip num">{workflow.fields.filter(f=>f.enabled).length} / {workflow.fields.length}</span></summary>
   <div class="fields">
   {#each workflow.fields as field}<div class="wf-field" class:on={field.enabled} data-workflow-node={field.nodeId} data-workflow-input={field.inputName}>
    <label class="check"><input type="checkbox" disabled={busy} bind:checked={field.enabled} onchange={invalidate} /><span class="fid">{$t("Node {id}",{id:field.nodeId})}</span><span class="muted ell">{field.classType} · {field.inputName}</span><span class="chip">{field.valueType}</span></label>
    {#if field.enabled}<div class="wf-edit">{#if field.valueType === 'string'}<label class="field-row"><span>{$t("Section")}</span><select disabled={busy} bind:value={field.section} onchange={invalidate}><option value={null}>{$t("Champ libre")}</option>{#each sections as section}<option value={section}>{section}</option>{/each}</select></label><label class="stack">{$t("Texte exact")}<textarea disabled={busy} rows="3" bind:value={field.value} oninput={invalidate}></textarea></label>{:else}<label class="field-row"><span>{$t("Valeur")}</span><input class="num" disabled={busy} bind:value={field.value} oninput={invalidate} /></label>{/if}</div>{/if}
   </div>{/each}
   </div>
   <div class="sec-foot"><button disabled={busy} onclick={saveMapping}>{$t("Enregistrer les champs et leurs rôles")}</button></div>
  </details>
  {#key workflow.id}<Couple id={workflow.id} {busy} {invalidate} prepare={prepareCouple} />{/key}
  <details class="sec" open><summary title={$t("Sélectionner associe la fiche ; le bouton charge son texte exact dans l’unique entrée mappée. La section P1 du preset décrit l’identité, même lorsqu’elle est chargée dans P2. Le panneau Saya Couple conserve ses propres sélections.")}><span class="sec-title">{$t("Personnages et variantes de la préparation générale")}</span></summary>
   <fieldset class="sec-body plain" disabled={busy} data-general-identities>
    <label class="check"><input data-general-append type="checkbox" bind:checked={appendPreset} onchange={invalidate} />{$t("Ajouter le preset après le texte existant, sans retirer de tags")}</label>
    <div class="people">{#each selectedPeople as person,index}<section data-general-person={index}>
     <label class="field-row"><span>P{index+1}</span><select bind:value={selectedPeople[index]} onchange={()=>choosePerson(index)}><option value="">{$t("Aucune association")}</option>{#each characters as character}<option value={character.id}>{character.name}</option>{/each}</select></label>
     <label class="field-row"><span>{$t("Variante P{n}",{n:index+1})}</span><select bind:value={selectedVariants[index]} onchange={invalidate}><option value="">{$t("Identité de la fiche")}</option>{#each personVariants[index] as variant}<option value={variant.id}>{variant.name}</option>{/each}</select></label>
     <button disabled={!person} onclick={()=>loadPerson(index)}>{$t("Charger le preset dans P{n}",{n:index+1})}</button>
    </section>{/each}</div>
    {#if presetPreview}<section class="compare" data-preset-preview title={$t("Seul ce champ texte reçoit le résultat affiché. Le mode ajout conserve le texte existant suivi du preset exact. Les autres sections du preset restent à configurer explicitement.")}><h3>{$t("Comparer le preset avant application dans P{n}",{n:presetPreview.index+1})}</h3><div class="ba"><div><h4>{$t("Avant")}</h4><pre>{presetPreview.before}</pre></div><div><h4>{$t("Après")}</h4><pre>{presetPreview.text}</pre></div></div><div class="row"><button disabled={busy} onclick={applyPreset}>{$t("Appliquer ce preset à P{n}",{n:presetPreview.index+1})}</button><button class="quiet" onclick={()=>presetPreview=null}>{$t("Annuler ce chargement")}</button></div></section>{/if}
   </fieldset>
  </details>
 {:else}<p class="empty center">{$t("Choisissez un workflow dans la bibliothèque.")}</p>{/if}
 </div>
 {#if workflow}
 <footer class="wf-foot">
  {#if prepared}<section class="prepared" data-prepared-request><h3>{$t("Requête avant lancement")} <span class="chip num">{$t("{n} nodes · brouillon {id}",{n:prepared.nodeCount,id:prepared.jobId})}</span></h3><details><summary>{$t("Détails techniques · requête envoyée au moteur")}</summary><pre>{prepared.requestJson}</pre></details><details><summary>{$t("Différences appliquées")}</summary><pre>{JSON.stringify(prepared.changes,null,2)}</pre></details>{#if prepared.removed.length}<p class="error">{$t("Tags ou fragments retirés :")}</p><pre>{prepared.removed.join('\n')}</pre><label class="check"><input type="checkbox" bind:checked={allowRemovals} />{$t("J’autorise explicitement ces suppressions.")}</label>{/if}<label class="check"><input type="checkbox" bind:checked={confirmed} />{$t("Je confirme le lancement GPU de cette requête.")}</label></section>{/if}
  <div class="row">
   <button class="quiet" disabled={busy||!baseline} onclick={undo} title={$t("Annuler restaure le début de l’édition ou la dernière préparation réussie. Réinitialiser recharge les valeurs importées et le mapping enregistré. Aucun de ces boutons ne supprime les brouillons de l’historique.")}>{$t("Annuler les éditions non appliquées")}</button>
   <button class="quiet" disabled={busy} onclick={discard}>{$t("Réinitialiser depuis la copie importée")}</button>
   <span class="grow">{#if busy}<span class="muted" role="status" aria-live="polite">{$t("Traitement en cours… Les commandes seront disponibles à la fin.")}</span>{/if}</span>
   {#if prepared}<button class="primary" disabled={busy || !confirmed || (!!prepared.removed.length && !allowRemovals)} onclick={submit}>{$t("Lancer la génération confirmée")}</button>
   {:else}<button class="primary" disabled={busy} onclick={prepare}>{busy ? $t("Traitement…") : $t("Vérifier les réglages et préparer le lancement")}</button>{/if}
  </div>
 </footer>
 {/if}
</section>

<aside class="wb-panel right">
 <header class="wb-head"><h2 title={$t("État lu dans la file et l’historique ComfyUI. Progression annoncée par node via WebSocket ; récupération HTTP automatique. Une soumission incertaine n’est jamais relancée automatiquement.")}>{$t("Historique des générations")}</h2><span class="chip num">{jobs.length}</span><button class="icon-btn quiet" disabled={busy || !native} onclick={sync} title={$t("Synchroniser avec le moteur")} aria-label={$t("Synchroniser avec le moteur")}><Icon name="redo" size={14} /></button></header>
 <div class="live"><LiveJobs onupdate={value=>live=value} /></div>
 <div class="wb-scroll">
  {#each jobs as job (job.id)}<article class="job">
   <strong>{job.workflow} · {$t(stateLabel(job.state))}</strong>
   <p class="meta num" title={job.promptId ?? $t("aucun prompt_id reçu")}>{job.createdAt} · {job.promptId ?? $t("aucun prompt_id reçu")}</p>
   {#if job.characters.length}<p class="meta">{$t("Personnages prévus : {list}",{list:job.characters.map(c=>$t("{role} : {name} (rév. {revision})",{role:c.role,name:c.name,revision:c.revision})).join(' · ')})}</p>{/if}
   {#if job.promptId}<LiveJobs promptId={job.promptId} observed={live} />{/if}
   {#if job.error}<pre class="error">{JSON.stringify(job.error,null,2)}</pre>{/if}
   {#if job.outputs.length}<div class="thumbs">{#each job.outputs as output}<Thumbnail id={output.assetId} />{/each}</div>{/if}
   <JobControls {job} /><Checkpoints {job} />
  </article>{:else}<p class="empty">{$t("Aucune génération.")}</p>{/each}
 </div>
</aside>
</div>

<style>
.wf{--wb-left:380px;--wb-right:340px;height:100%;min-height:480px;overflow:hidden}
.wf-main{background:var(--bg)}
.hint{color:var(--faint);display:inline-grid;cursor:help}
h2{font-size:13px;font-weight:600;margin:0}
h3{font-size:12.5px;font-weight:600;margin:0 0 6px}
h4{font-size:11px;font-weight:600;color:var(--muted);margin:0 0 4px;text-transform:uppercase;letter-spacing:.04em}
details{margin:0}
label{color:var(--muted);font-size:12px}
.stack{display:flex;flex-direction:column;gap:4px}
.check{display:flex;flex-direction:row;align-items:center;gap:8px;color:var(--text)}
.check input{min-height:0}
.field-row{display:grid;grid-template-columns:96px minmax(0,1fr);align-items:center;gap:8px}
.muted{color:var(--muted)}.ell{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1}
.empty{color:var(--muted);padding:16px 12px;margin:0}.empty.center{text-align:center;padding-top:48px}
.error{color:var(--danger);margin:0;overflow-wrap:anywhere}.note{color:var(--muted);margin:0;overflow-wrap:anywhere}
.msgs{display:grid;gap:4px;padding:0 12px}.msgs:has(p){padding:8px 12px;border-bottom:1px solid var(--line)}
pre{font-size:11.5px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:0;max-height:240px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}
.lib-filter{padding:8px 12px;border-bottom:1px solid var(--line)}
.lib{width:100%;border-collapse:collapse;font-size:12px}
.lib th{font-size:10.5px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--faint);text-align:start;padding:6px 8px;border-bottom:1px solid var(--line)}
.lib td{padding:0 8px;border-bottom:1px solid var(--line);vertical-align:middle;height:32px;overflow-wrap:anywhere}
.lib td.muted{font-size:11.5px}
.lib .end{text-align:end;white-space:nowrap}
.lib tr:hover td{background:var(--hover)}
.lib tr.open td{background:var(--raised)}
.lib tr.open td:first-child{box-shadow:inset 2px 0 0 var(--accent)}
.row-open{background:transparent;border:0;padding:0;min-height:28px;text-align:start;width:100%;color:var(--text)}
.row-open:hover:not(:disabled){background:transparent}
.fields{border-top:1px solid var(--line)}
.wf-field{padding:6px 12px;border-bottom:1px solid var(--line);overflow-wrap:anywhere}
.wf-field .fid{font-variant-numeric:tabular-nums;white-space:nowrap}
.wf-field.on{background:var(--panel)}
.wf-edit{display:grid;gap:6px;padding:6px 0 4px 24px}
.sec-foot{padding:8px 12px 12px;display:flex;justify-content:flex-end}
fieldset.plain{border:0;margin:0;min-width:0}
.people{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,280px),1fr));gap:12px}
.people section{display:grid;gap:6px;align-content:start}
.people section button{justify-self:start}
.compare{border-top:1px solid var(--line);padding-top:8px;display:grid;gap:6px}
.ba{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr));gap:8px}
.row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}.grow{flex:1;min-width:0}
.wf-foot{border-top:1px solid var(--line);background:var(--panel);padding:8px 12px;display:grid;gap:8px;max-height:55%;overflow:auto;flex:none}
.prepared{display:grid;gap:6px}
.prepared summary{color:var(--muted);font-size:12px}
.live{padding:6px 12px;border-bottom:1px solid var(--line);font-size:11.5px;color:var(--muted)}
.job{padding:8px 12px;border-bottom:1px solid var(--line);display:grid;gap:4px;overflow-wrap:anywhere}
.job strong{font-size:12.5px;font-weight:600}
.meta{margin:0;font-size:11px;color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.thumbs{display:flex;flex-wrap:wrap;gap:4px}
.thumbs :global(img){width:72px;height:72px;object-fit:cover;border-radius:var(--r-img);display:block;background:var(--canvas)}
@media (max-width:1200px){.wf{--wb-left:300px;--wb-right:280px}}
@media (max-width:980px){.wf{grid-template-columns:minmax(0,1fr);height:auto}.wf>.wb-panel{max-height:none;border:0;border-bottom:1px solid var(--line)}}
</style>
