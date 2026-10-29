<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import CharacterContext from './CharacterContext.svelte';
  import {identityText,composePrompt} from '../lib/identity-preset';
  import { call, message, type Character } from '../lib/api';
  interface Role { section:string; nodeId:string; inputName:string; value:string }
  interface Mask { nodeId:string; enabled:boolean; inputs:Record<string,unknown> }
  interface Inspection { promptProfile:'solo'|'couple'|'unconfigured'; roles:Role[]; masks:Mask[]; links:unknown[]; warnings:string[]; scope:string }
  interface Variant { id:string; name:string; promptBlocks:Record<string,string> }
  let { id, busy, invalidate, prepare }: { id:string; busy:boolean; invalidate:()=>void; prepare:(input:Record<string,unknown>)=>Promise<boolean> } = $props();
  let inspection = $state<Inspection|null>(null), characters = $state<Character[]>([]), error = $state('');
  const coupled=$derived(inspection?.promptProfile==='couple');
  let people = $state([{id:'',variant:'',variants:[] as Variant[],text:''},{id:'',variant:'',variants:[] as Variant[],text:''}]);
  let action = $state(''), bg = $state(''), quality = $state(''), negative = $state('');
  let mask = $state(''), direction = $state('vertical'), split = $state(50), blur = $state(0), swap = $state(false);
  let loading = $state(false),appendPreset=$state(false),note=$state('');
  const sharedSections=['QUALITY','ACTION','BG','NEG'] as const;
  type Section='P1'|'P2'|'QUALITY'|'ACTION'|'BG'|'NEG';
  let selectedShared=$state<string[][]>([[],[]]);
  let preview=$state<{fingerprint:string;changes:{section:Section;before:string;after:string}[]}|null>(null);
  const personSerial=[0,0];
  function capture(){return structuredClone($state.snapshot({people,action,bg,quality,negative,mask,direction,split,blur,swap,selectedShared,appendPreset}));}
  let baseline=$state<ReturnType<typeof capture>|null>(null);
  function changed(){preview=null;invalidate();}
  function prompts():Record<Section,string>{return {P1:people[0].text,P2:people[1].text,ACTION:action,BG:bg,QUALITY:quality,NEG:negative};}
  function writePrompt(section:Section,value:string){if(section==='P1')people[0].text=value;else if(section==='P2')people[1].text=value;else if(section==='ACTION')action=value;else if(section==='BG')bg=value;else if(section==='QUALITY')quality=value;else negative=value;}
  function fingerprint(){return JSON.stringify({people:people.map(p=>({id:p.id,variant:p.variant})),prompts:prompts(),mask,direction,split,blur,swap,selectedShared,appendPreset});}
  function undo(){if(!baseline)return;const saved=structuredClone($state.snapshot(baseline));personSerial[0]++;personSerial[1]++;people=saved.people;action=saved.action;bg=saved.bg;quality=saved.quality;negative=saved.negative;mask=saved.mask;direction=saved.direction;split=saved.split;blur=saved.blur;swap=saved.swap;selectedShared=saved.selectedShared;appendPreset=saved.appendPreset;changed();error='';note=$t("Édition Couple annulée localement ; dernière base restaurée, sans écriture.");}
  function applyPreview(){if(!preview)return;if(preview.fingerprint!==fingerprint()){preview=null;error=$t("Édition modifiée : comparez à nouveau le preset.");return;}for(const change of preview.changes)writePrompt(change.section,change.after);changed();note=$t("Preset appliqué uniquement aux sections confirmées.");}
  function selectShared(index:number,section:string,checked:boolean){selectedShared[index]=checked?[...selectedShared[index],section]:selectedShared[index].filter(s=>s!==section);changed();}

  const role = (key:string) => inspection?.roles.find(r => r.section === key);
  async function inspect() {
    loading = true; error = ''; changed();
    const first=inspection===null;
    try { inspection = await call<Inspection>('couple_inspection',{id}); if(first)loadMapped(); }
    catch(e) { error = message(e); } finally { loading = false; }
  }
  function loadMapped() {
    people[0].text=role('P1')?.value??''; people[1].text=role('P2')?.value??'';
    action=role('ACTION')?.value??''; bg=role('BG')?.value??''; quality=role('QUALITY')?.value??''; negative=role('NEG')?.value??'';for(const person of people){person.id='';person.variant='';person.variants=[];}personSerial[0]++;personSerial[1]++;selectedShared=[[],[]];appendPreset=false;mask='';direction='vertical';split=50;blur=0;swap=false;changed();baseline=capture();note=$t("Textes du workflow chargés comme nouvelle base ; associations réinitialisées.");
  }
  async function choose(index:number) {
    changed();
    const person=people[index]; const selected=person.id,ticket=++personSerial[index];selectedShared[index]=[]; person.variants=[]; person.variant=''; error='';
    try { if(selected) { const values=await call<Variant[]>('variants',{id:selected}); if(ticket===personSerial[index]&&person.id===selected) person.variants=values; } } catch(e) { if(ticket===personSerial[index])error=message(e); }
  }
  function loadPerson(index:number) {
    const person=people[index],character=characters.find(c=>c.id===person.id),variant=person.variants.find(v=>v.id===person.variant);
    error='';try {
      if(person.variant&&!variant)throw new Error($t("Variante indisponible ; sélectionnez à nouveau le personnage."));
      const section:Section=index===0?'P1':'P2',current=prompts();
      if(!role(section))throw new Error($t("Mapping {section} absent.",{section}));
      const requested:{section:Section;text:string}[]=[{section,text:identityText(character,variant)}];
      for(const shared of sharedSections.filter(s=>selectedShared[index].includes(s))){if(!role(shared)||typeof variant?.promptBlocks[shared]!=='string')throw new Error($t("Section {section} absente du preset ou du mapping.",{section:shared}));requested.push({section:shared,text:variant.promptBlocks[shared]});}
      preview={fingerprint:fingerprint(),changes:requested.map(r=>({section:r.section,before:current[r.section],after:composePrompt(current[r.section],r.text,appendPreset)}))};
    }catch(e){error=message(e);preview=null;}
  }
  function loadMask() {
    const inputs=inspection?.masks.find(m=>m.nodeId===mask)?.inputs;
    if(inputs) { direction=String(inputs.direction); split=Number(inputs.split); blur=Number(inputs.blur); swap=inputs.swap===true; } changed();
  }
  async function prepareCouple() {
    error='';
    if(mask&&!(typeof split==='number'&&Number.isInteger(split)&&split>=0&&split<=100)){error=$t("Séparation invalide : saisissez un nombre entier entre 0 et 100.");return;}
    const success=await prepare({characters:people.flatMap((p,index)=>p.id&&(index===0||coupled)?[{role:`P${index+1}`,characterId:p.id,variantId:p.variant||null}]:[]),p1:people[0].text,p2:coupled?people[1].text:'',action,bg,quality:role('QUALITY')?quality:null,negative:role('NEG')?negative:null,
      mask:mask?{nodeId:mask,direction,split,blur,swap}:null});
    if(success)baseline=capture();
  }
  onMount(() => { baseline=capture();inspect(); call<Character[]>('characters').then(v=>characters=v).catch(e=>error=message(e)); });
</script>
<details class="sec couple" data-couple-editor><summary title={$t("Enregistrez P1, plus P2 pour éditer deux identités. Seules les sections partagées mappées sont affichées. Ce profil ne modifie aucun mode interne du workflow. Les presets chargent une identité et seulement les sections partagées cochées. LoRA et anatomy restent à configurer explicitement. Les textes restent exacts. Les fiches sélectionnées sont associées à la requête et aux images récupérées ; cela ne valide pas leur ressemblance.")}><span class="sec-title">{$t("Éditeur Saya")}</span><span class="chip">{inspection?.promptProfile==='solo'?$t("SOLO · une identité"):coupled?$t("COUPLE · deux identités"):$t("mapping à configurer")}</span></summary>
<div class="sec-body">
{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}
<div class="row">
 <button class="quiet" disabled={busy || loading} onclick={inspect}>{$t("Relire le mapping enregistré")}</button>
 {#if inspection}<button class="quiet" disabled={busy} onclick={loadMapped}>{$t("Réinitialiser depuis les textes du workflow")}</button><button class="quiet" disabled={busy||!baseline} onclick={undo}>{$t("Annuler les éditions Couple")}</button>{/if}
</div>
{#if inspection}
{#each inspection.warnings as warning}<p class="note">{warning}</p>{/each}
<details class="inline"><summary>{$t("Inspecter rôles et connexions")}</summary><p class="note">{inspection.scope}</p><pre>{JSON.stringify({roles:inspection.roles,links:inspection.links},null,2)}</pre></details>
<fieldset class="plain" disabled={busy}>
<label class="check"><input type="checkbox" bind:checked={appendPreset} onchange={changed} />{$t("Ajouter les presets à la fin des textes existants, sans retirer de tags")}</label>
<div class="people">{#each people as person,index}{#if index===0||coupled}<section data-couple-person={index}><h3>P{index+1}</h3>
<label class="field-row"><span>{$t("Personnage")}</span><select bind:value={person.id} onchange={()=>choose(index)}><option value="">{$t("Texte libre")}</option>{#each characters as character}<option value={character.id}>{character.name}</option>{/each}</select></label>
<label class="field-row"><span>{$t("Preset d’identité")}</span><select bind:value={person.variant} onchange={()=>{selectedShared[index]=[];changed();}}><option value="">{$t("Identité de la fiche")}</option>{#each person.variants as variant}<option value={variant.id}>{variant.name} · {$t("section P1")}</option>{/each}</select></label>
{#if person.variant}<div class="checks">{#each sharedSections as section}<label class="check"><input type="checkbox" disabled={!role(section)||typeof person.variants.find(v=>v.id===person.variant)?.promptBlocks[section]!=='string'} checked={selectedShared[index].includes(section)} onchange={e=>selectShared(index,section,e.currentTarget.checked)} />{$t("Charger aussi {section}",{section})}</label>{/each}</div>{/if}
<button class="cmp" disabled={!person.id} onclick={()=>loadPerson(index)}>{$t("Comparer ce preset pour P{n}",{n:index+1})}</button>
{#each characters.filter(c=>c.id===person.id) as character (character.id)}<CharacterContext {character} variant={person.variant} />{/each}
<label class="stack">{$t("Prompt P{n} exact",{n:index+1})}<textarea rows="4" bind:value={person.text} oninput={changed}></textarea></label></section>{/if}{/each}</div>
<div class="shared">
{#if role('ACTION')}<label class="stack">{$t("ACTION commune")}<textarea rows="3" bind:value={action} oninput={changed}></textarea></label>{/if}{#if role('BG')}<label class="stack">{$t("BG indépendant")}<textarea rows="3" bind:value={bg} oninput={changed}></textarea></label>{/if}
{#if role('QUALITY')}<label class="stack">{$t("QUALITY")}<textarea rows="2" bind:value={quality} oninput={changed}></textarea></label>{/if}
{#if role('NEG')}<label class="stack">{$t("NEG")}<textarea rows="2" bind:value={negative} oninput={changed}></textarea></label>{/if}
</div>
<div class="mask">
<label class="field-row"><span>{$t("Zones SayaSplitMask")}</span><select bind:value={mask} onchange={loadMask}><option value="">{$t("Conserver tous les masques du workflow")}</option>{#each inspection.masks as item}<option value={item.nodeId} disabled={!item.enabled}>{$t("Node {id}",{id:item.nodeId})}{item.enabled?'':' · '+$t("mapping incomplet")}</option>{/each}</select></label>
{#if mask}<label class="field-row"><span>{$t("Direction")}</span><select bind:value={direction} onchange={changed}><option value="vertical">{$t("Gauche / droite")}</option><option value="horizontal">{$t("Haut / bas")}</option></select></label><label class="field-row"><span>{$t("Séparation (%)")}</span><input class="num" type="number" min="0" max="100" step="1" bind:value={split} oninput={changed} /></label><label class="field-row"><span>{$t("Flou (sigma pixels)")}</span><input class="num" type="number" min="0" step="0.1" bind:value={blur} oninput={changed} /></label><label class="check" title={$t("Le latent et les connexions de conditioning sont conservés. L’inversion ne permute pas les prompts.")}><input type="checkbox" bind:checked={swap} onchange={changed} />{$t("Inverser les deux masques")}</label>{/if}
</div>
{#if preview}<section class="compare" data-couple-preview><h3>{$t("Comparer avant application")}</h3>{#each preview.changes as change}<div class="ba"><div><h4>{change.section} · {$t("avant")}</h4><pre>{change.before}</pre></div><div><h4>{change.section} · {$t("après")}</h4><pre>{change.after}</pre></div></div>{/each}<div class="row"><button onclick={applyPreview}>{$t("Appliquer les sections affichées")}</button><button class="quiet" onclick={()=>preview=null}>{$t("Annuler ce chargement Couple")}</button></div></section>{/if}
<div class="row end"><button disabled={inspection.promptProfile==='unconfigured'||!people[0].text.trim()||(coupled&&!people[1].text.trim())} onclick={prepareCouple}>{coupled?$t("Valider et préparer Saya Couple"):$t("Valider et préparer SOLO")}</button></div>
</fieldset>{/if}
</div>
</details>

<style>
h3{font-size:12.5px;font-weight:600;margin:0}
h4{font-size:11px;font-weight:600;color:var(--muted);margin:0 0 4px;text-transform:uppercase;letter-spacing:.04em}
label{color:var(--muted);font-size:12px}
.stack{display:flex;flex-direction:column;gap:4px}
.check{display:flex;flex-direction:row;align-items:center;gap:8px;color:var(--text)}
.field-row{display:grid;grid-template-columns:110px minmax(0,1fr);align-items:center;gap:8px}
.field-row>*{min-width:0}
.error{color:var(--danger);margin:0;overflow-wrap:anywhere}.note{color:var(--muted);margin:0;overflow-wrap:anywhere}
pre{font-size:11.5px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:0;max-height:240px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}
details.inline{margin:0}details.inline>summary{color:var(--muted);font-size:12px;cursor:pointer}
fieldset.plain{border:0;margin:0;padding:0;min-width:0;display:grid;gap:10px}
.row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}.row.end{justify-content:flex-end}
.people{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,300px),1fr));gap:12px}
.people section{min-width:0;display:grid;gap:6px;align-content:start;border-top:1px solid var(--line);padding-top:8px}
.people .cmp{justify-self:start}
.checks{display:flex;flex-wrap:wrap;gap:4px 12px}
.shared,.mask{display:grid;gap:6px}
.mask{border-top:1px solid var(--line);padding-top:8px}
.compare{border-top:1px solid var(--line);padding-top:8px;display:grid;gap:6px}
.ba{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr));gap:8px}
</style>
