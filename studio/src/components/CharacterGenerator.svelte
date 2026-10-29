<script lang="ts">
  import { t } from '../lib/i18n';
 import {onMount} from 'svelte';
 import {call,message,native,type Character} from '../lib/api';
 interface Attribute {id:string;label:string;permanent:boolean;values:string[]}
 interface Result {seed:string;name:string;identity:Record<string,unknown>;attributes:Record<string,string>;prompts:Record<string,string>;explanation:string;trace:{attribute:string;selected:string;reason:string;candidates:string[]}[]}
 interface PreparedVariant {characterId:string;revision:number;basePrompt:string;result:Result}
 let {onuse,character=null,onsaved=()=>{}}:{onuse:(result:Result)=>void;character?:Character|null;onsaved?:(character:string,variant:string)=>void|Promise<void>}=$props();
 let variantMode=$state(false),variantName=$state(''),preparedVariant=$state<PreparedVariant|null>(null),note=$state('');
 const sourceGenerator=$derived(character?.identity.generator as {version?:number;attributes?:Record<string,string>}|undefined);
 const canVary=$derived(!!character&&!character.archived&&sourceGenerator?.version===2);
 function mode(enabled:boolean){variantMode=enabled;const nextRequired={...required};const nextForbidden={...forbidden};const nextLocked={...locked};for(const field of fields.filter(f=>f.permanent)){delete nextRequired[field.id];delete nextForbidden[field.id];delete nextLocked[field.id];if(enabled&&sourceGenerator?.attributes){nextRequired[field.id]=sourceGenerator.attributes[field.id];nextLocked[field.id]=sourceGenerator.attributes[field.id];}}if(enabled&&sourceGenerator?.attributes){nextRequired.world=sourceGenerator.attributes.world;nextRequired.role=sourceGenerator.attributes.role;}required=nextRequired;forbidden=nextForbidden;locked=nextLocked;note='';}
 async function saveVariant(){if(!preparedVariant||stale||!variantMode)return;if(busy)return;busy=true;error='';try{const id=await call<string>('save_generated_variant',{character:preparedVariant.characterId,revision:preparedVariant.revision,input:JSON.parse(JSON.stringify(input)),name:variantName,confirmed:true});await onsaved(preparedVariant.characterId,id);note=$t("Variante enregistrée ; identité source conservée.");}catch(e){error=message(e);}finally{busy=false;}}

 let fields=$state<Attribute[]>([]),rules=$state<string[]>([]),required=$state<Record<string,string>>({world:'fantasy',role:'explorer'}),forbidden=$state<Record<string,string[]>>({}),locked=$state<Record<string,string>>({});
 let seed=$state('1'),family=$state(''),error=$state(''),busy=$state(false),result=$state<Result|null>(null),previous=$state<Result|null>(null),prepared=$state('');
 const input=$derived({seed,checkpointFamily:family,required:Object.fromEntries(Object.entries(required).filter(([,v])=>v)),forbidden:Object.fromEntries(Object.entries(forbidden).filter(([,v])=>v?.length)),locked});
 const signature=$derived(JSON.stringify({input,base:variantMode?{id:character?.id,revision:character?.revision}:null}));
 const stale=$derived(prepared!==signature);
 async function generate(){if(busy)return;busy=true;error='';note='';const frozen=JSON.parse(JSON.stringify(input)),ticket=signature,base=variantMode?character?.id:null;try{let next:Result;if(base){const proposal=await call<PreparedVariant>('prepare_generated_variant',{character:base,input:frozen});preparedVariant=proposal;next=proposal.result;}else{next=await call<Result>('generate_character',{input:frozen});preparedVariant=null;}previous=result;result=next;prepared=ticket;}catch(e){error=message(e);}finally{busy=false;}}
 function randomSeed(){const words=crypto.getRandomValues(new Uint32Array(2));seed=((BigInt(words[0])<<32n)|BigInt(words[1])).toString();}
 function lock(field:string,enabled:boolean){if(enabled&&result)locked={...locked,[field]:result.attributes[field]};else {const next={...locked};delete next[field];locked=next;}}
 onMount(()=>{if(native)call<{attributes:Attribute[];rules:string[]}>('generator_catalog').then(c=>{fields=c.attributes;rules=c.rules;}).catch(e=>error=message(e));});
</script>
<details class="sec generator"><summary title={$t("Les traits permanents et variables sont séparés. Les verrous conservent une valeur lors du prochain calcul. Une combinaison impossible est refusée avec sa cause.")}><span class="sec-title">{$t("Proposer un personnage avec contraintes")}</span></summary><div class="sec-body">
<div class="gen-bar"><label class="checkbox" title={$t("Les variantes automatiques demandent une fiche du générateur v2 dont le prompt n’a pas été retouché. Les autres fiches gardent leur éditeur manuel.")}><input type="checkbox" data-generator-variant-mode checked={variantMode} disabled={!canVary||busy} onchange={e=>mode(e.currentTarget.checked)} />{$t("Créer une variante de {name}",{name:character?.name??$t("la fiche sélectionnée")})}</label></div>
{#if note}<p class="muted" role="status">{note}</p>{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}
<div class="gen-fields"><label>{$t("Seed reproductible (u64)")}<span class="seed-row"><input data-generator-seed bind:value={seed} class="num" /><button class="quiet" disabled={busy} onclick={randomSeed}>{$t("Nouvelle seed")}</button></span></label><label>{$t("Famille checkpoint déclarée")}<input bind:value={family} placeholder={$t("Facultatif ; compatibilité à vérifier dans le runner")} /></label></div>
<div class="generator-rules">{#each fields as field}<div class="rule" data-generator-attribute={field.id}><strong>{field.label} <small>· {field.permanent?$t("permanent"):$t("variable")}</small></strong>
<label>{$t("Valeur obligatoire")}<select aria-label={$t("{field} : imposer",{field:field.label})} disabled={variantMode&&field.permanent} bind:value={required[field.id]}><option value="">{$t("Au choix")}</option>{#each field.values as value}<option {value}>{value}</option>{/each}</select></label>
<label>{$t("Valeurs interdites")}<select aria-label={$t("{field} : interdire",{field:field.label})} disabled={variantMode&&field.permanent} multiple size={Math.min(field.values.length,3)} bind:value={forbidden[field.id]}>{#each field.values as value}<option {value}>{value}</option>{/each}</select></label>
<label class="checkbox"><input type="checkbox" aria-label={$t("{field} : verrouiller",{field:field.label})} checked={!!locked[field.id]} disabled={!result||busy||(variantMode&&field.permanent)} onchange={e=>lock(field.id,e.currentTarget.checked)} />{$t("Verrouiller {value}",{value:locked[field.id]??result?.attributes[field.id]??''})}</label></div>{/each}</div>
<div class="gen-actions"><details class="sub"><summary>{$t("Règles de cohérence")}</summary>{#each rules as rule}<p>{rule}</p>{/each}</details><span class="grow"></span><button disabled={!native||busy||!fields.length||(variantMode&&!canVary)} onclick={generate}>{busy?$t("Calcul…"):$t("Calculer la proposition")}</button></div>
{#if result}<div class="gen-result"><p class="expl">{result.explanation}</p><p class="muted num" data-generator-used-seed>{$t("Seed utilisée : {seed}",{seed:result.seed})}</p>{#if stale}<p class="warn" role="status">{$t("Contraintes modifiées : recalculez avant d’utiliser cette proposition.")}</p>{/if}
<div class="comparison"><table><thead><tr><th>{$t("Trait")}</th>{#if previous}<th>{$t("Résultat précédent")}</th>{/if}<th>{$t("Résultat courant")}</th><th>{$t("Choix")}</th></tr></thead><tbody>{#each fields as field}<tr data-generator-result={field.id}><th>{field.label}</th>{#if previous}<td>{previous.attributes[field.id]}</td>{/if}<td>{result.attributes[field.id]}</td><td>{result.trace.find(t=>t.attribute===field.id)?.reason==='locked'?$t("Verrouillé"):result.trace.find(t=>t.attribute===field.id)?.reason==='required'?$t("Imposé"):$t("Seed et contraintes")}</td></tr>{/each}</tbody></table></div>
<details class="sub"><summary>{$t("Pourquoi ces valeurs ?")}</summary>{#each result.trace as trace}<p>{$t("{field} : {selected} · candidats compatibles : {candidates}",{field:fields.find(f=>f.id===trace.attribute)?.label??'',selected:trace.selected,candidates:trace.candidates.join(', ')})}</p>{/each}</details>
{#if variantMode&&preparedVariant}<h4>{$t("Prompt source conservé")}</h4><pre data-generator-base-prompt>{preparedVariant.basePrompt}</pre><h4>{$t("Prompt de la nouvelle variante")}</h4>{/if}
<pre data-generator-prompt>{result.prompts.P1}</pre>
{#if variantMode&&preparedVariant}<div class="gen-actions"><label class="grow">{$t("Nom de la nouvelle variante")}<input data-generated-variant-name bind:value={variantName} /></label><button disabled={busy||stale||!variantName.trim()||!canVary} onclick={saveVariant}>{$t("Confirmer la création de cette variante")}</button></div>{:else}<div class="gen-actions"><span class="grow"></span><button disabled={busy||stale} title={$t("La nouvelle fiche reste modifiable avant enregistrement. Aucun personnage existant n’est remplacé.")} onclick={()=>result&&onuse(result)}>{$t("Utiliser dans une nouvelle fiche")}</button></div>{/if}
</div>{/if}
</div></details>
<style>
 .generator{margin:0}
 label{display:flex;flex-direction:column;gap:4px;margin:0;color:var(--muted);font-size:12px}
 label.checkbox{flex-direction:row;align-items:center;gap:6px}
 input[type=checkbox]{width:auto;min-height:0;margin:0}
 details{margin:0}
 .grow{flex:1}
 .gen-fields{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px 14px}
 .seed-row{display:flex;gap:6px}.seed-row input{flex:1;min-width:0}
 .generator-rules{display:grid;grid-template-columns:repeat(auto-fill,minmax(190px,1fr));gap:1px;background:var(--line);border:1px solid var(--line);border-radius:var(--radius);overflow:hidden}
 .rule{background:var(--panel);padding:8px 10px;display:grid;gap:6px;align-content:start}
 .rule strong{font-size:12px;font-weight:600}.rule small{font-weight:400;color:var(--muted)}
 .rule select[multiple]{min-height:0;padding:2px 4px}
 .gen-actions{display:flex;align-items:flex-end;gap:8px}
 .sub>summary{color:var(--muted);font-size:12px;min-height:24px;display:flex;align-items:center}.sub p{margin:4px 0;font-size:12px}
 .gen-result{display:grid;gap:6px;padding-top:8px;border-top:1px solid var(--line)}
 .gen-result p{margin:0}.expl{font-size:12.5px}.warn{color:var(--warning)}
 .comparison{overflow:auto}
 table{width:100%;text-align:start;border-collapse:collapse;font-size:12px}
 th,td{padding:4px 8px;border-bottom:1px solid var(--line);text-align:start}thead th{color:var(--muted);font-weight:500;font-size:11px}tbody th{font-weight:500}
 pre{font-size:11.5px;max-height:240px;margin:0;padding:8px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius)}
 h4{margin:4px 0 0;font-size:11px;font-weight:600;color:var(--muted)}
 @media(max-width:1000px){.gen-fields{grid-template-columns:1fr}}
</style>
