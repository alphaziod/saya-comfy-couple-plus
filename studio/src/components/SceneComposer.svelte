<script lang="ts">
  import { t } from '../lib/i18n';
  import GrokScenes from "./GrokScenes.svelte";
  import { onMount } from 'svelte';
  import CharacterContext from './CharacterContext.svelte';
  import { call, message, native, type Character, type PreparedJob } from '../lib/api';
  import PreparedDraft from './PreparedDraft.svelte';
  import Icon from './Icon.svelte';
  import { buildManifest, compositionForProfile, sceneText, validateManifest, type Composition, type SceneManifest, type SceneText, type SceneTextSource, type SceneZone } from '../lib/scene-manifest';
  interface WorkflowItem { id:string; name:string; promptProfile:'solo'|'couple'|'unconfigured' }
  interface Variant { id:string; name:string; promptBlocks:Record<string,string> }
  interface Role { section:string; nodeId:string; inputName:string; value:string }
  interface MaskInfo { nodeId:string; enabled:boolean; inputs:Record<string,unknown> }
  interface Inspection { promptProfile:'solo'|'couple'|'unconfigured'; roles:Role[]; masks:MaskInfo[]; warnings:string[]; scope:string }
  interface SceneSummary { id:string; name:string; composition:Composition; workflowId:string; updatedAt:string }
  interface SceneRecord { id:string|null; name:string; manifest:SceneManifest }
  interface Preview {
    workflow:{id:string;name:string;promptProfile:string};
    composition:string;
    characters:{role:string;characterId:string;name:string;revision:number;variant:{id:string;name:string;revision:number}|null}[];
    jobCharacters:{role:string;characterId:string;variantId:string|null}[];
    prompts:Record<string,{nodeId:string;inputName:string;before:string;text:string;changed:boolean}>;
    edits:{section:string|null;nodeId:string;inputName:string;value:string}[];
    changes:{section:string|null;nodeId:string;inputName:string;before:string;after:string}[];
    mask:SceneManifest['mask'];
    zones?:{P1?:SceneZone|null;P2?:SceneZone|null};
    overlaps?:{a:string;b:string;aZ:number;bZ:number;visible:string;area:number}[];
    warnings:string[];
    scope:string;
  }
  type Prepared = PreparedJob;
  let scenes=$state<SceneSummary[]>([]), characters=$state<Character[]>([]), workflows=$state<WorkflowItem[]>([]);
  let editing=$state<SceneRecord|null>(null), inspection=$state<Inspection|null>(null);
  let personVariants=$state<Variant[][]>([[],[]]);
  let preview=$state<Preview|null>(null), prepared=$state<Prepared|null>(null);
  let busy=$state(false), error=$state(''), note=$state(''), deleteArmed=$state('');
  const personSerial=[0,0];
  // Snapshot of the saved/loaded state, to detect unsaved changes.
  let savedSnap=$state('');
  function snap(){return editing?JSON.stringify($state.snapshot(editing)):'';}
  const dirty=$derived(!!editing&&snap()!==savedSnap);
  function confirmDiscard(){return !dirty||confirm($t("Des modifications non enregistrées seront perdues. Continuer ?"));}
  function closeEditor(){
    if(!confirmDiscard())return;
    editing=null;resetWorkflowState();error='';note='';
  }
  const composition=$derived(editing?compositionForProfile(inspection?.promptProfile??'unconfigured'):null);
  const coupled=$derived(composition==='couple');
  const workflowOptions=$derived(workflows.filter(w=>w.promptProfile!=='unconfigured'));
  const mappedShared=$derived(new Set((inspection?.roles??[]).map(r=>r.section)));
  const sharedSections=['ACTION','BG','QUALITY','NEG'] as const;
  type Shared=(typeof sharedSections)[number];
  function spec(section:Shared):SceneText|null|undefined{
    if(!editing)return null;
    if(section==='ACTION')return editing.manifest.action;
    if(section==='BG')return editing.manifest.bg;
    if(section==='QUALITY')return editing.manifest.quality;
    return editing.manifest.negative;
  }
  function sourceOf(section:Shared):string{return spec(section)?.source??'';}
  function changed(){preview=null;prepared=null;}
  function resetWorkflowState(){inspection=null;personVariants=[[],[]];preview=null;prepared=null;}
  async function refresh(){try{[scenes,characters,workflows]=await Promise.all([call<SceneSummary[]>('scenes'),call<Character[]>('characters'),call<WorkflowItem[]>('workflows')]);}catch(e){error=message(e);}}
  async function chooseWorkflow(id:string){
    if(!editing)return;
    changed();resetWorkflowState();
    editing.manifest.workflowId=id;
    error='';
    if(!id)return;
    busy=true;
    const ticket=editing.manifest.workflowId;
    try{
      const value=await call<Inspection>('couple_inspection',{id});
      if(!editing||editing.manifest.workflowId!==ticket)return;
      inspection=value;
      if(!compositionForProfile(value.promptProfile))error=$t("Ce workflow n'a aucun mapping P1 enregistré : configurez son mapping dans Workflows avant de composer une scène.");
    }catch(e){if(editing&&editing.manifest.workflowId===ticket)error=message(e);}finally{busy=false;}
  }
  function personId(index:number):string{
    if(!editing)return'';
    return index===0?editing.manifest.p1.characterId:editing.manifest.p2?.characterId??'';
  }
  async function choosePerson(index:number){
    if(!editing)return;changed();
    if(index===0)editing.manifest.p1.variantId=null;else if(editing.manifest.p2)editing.manifest.p2.variantId=null;
    personVariants[index]=[];
    const selected=personId(index),ticket=++personSerial[index];
    error='';
    try{
      if(selected){const variants=await call<Variant[]>('variants',{id:selected});if(ticket===personSerial[index]&&personId(index)===selected)personVariants[index]=variants;}
    }catch(e){if(ticket===personSerial[index])error=message(e);}
  }
  let zoneGesture=$state<null|{index:number;mode:'move'|'resize';sx:number;sy:number;zone:SceneZone}>(null);
  function zoneFor(index:number):SceneZone|null{
    if(!editing)return null;
    return (index===0?editing.manifest.p1.zone:editing.manifest.p2?.zone)??null;
  }
  function setZone(index:number,zone:SceneZone|null){
    if(!editing)return;changed();
    if(index===0)editing.manifest.p1.zone=zone;
    else if(editing.manifest.p2)editing.manifest.p2.zone=zone;
  }
  function toggleZone(index:number,on:boolean){
    const current=zoneFor(index);
    if(on){setZone(index,current??{x:index===0?0.05:0.55,y:0.1,width:0.4,height:0.8,feather:0.1,enabled:true,z:index});}
    else if(current){setZone(index,{...current,enabled:false});}
  }
  function zoneDown(e:PointerEvent,index:number,mode:'move'|'resize'){
    if(e.button!==0)return;const zone=zoneFor(index);if(!zone)return;
    e.stopPropagation();zoneGesture={index,mode,sx:e.clientX,sy:e.clientY,zone:{...zone}};
  }
  function zoneMove(e:PointerEvent){
    const g=zoneGesture;if(!g)return;const stage=e.currentTarget as HTMLElement;const r=stage.getBoundingClientRect();
    if(r.width<=0||r.height<=0)return;
    const dx=(e.clientX-g.sx)/r.width,dy=(e.clientY-g.sy)/r.height;
    let next={...g.zone};
    if(g.mode==='move'){next.x=Math.min(Math.max(0,g.zone.x+dx),1-g.zone.width);next.y=Math.min(Math.max(0,g.zone.y+dy),1-g.zone.height);}
    else {next.width=Math.min(Math.max(0.02,g.zone.width+dx),1-g.zone.x);next.height=Math.min(Math.max(0.02,g.zone.height+dy),1-g.zone.y);}
    setZone(g.index,next);
  }
  function zoneUp(){zoneGesture=null;}
  function zoneNumber(index:number,key:'x'|'y'|'width'|'height'|'feather'|'z',value:number){
    const zone=zoneFor(index);if(!zone)return;
    const next={...zone,[key]:value};
    if(key==='x')next.x=Math.min(Math.max(0,value),1-zone.width);
    if(key==='y')next.y=Math.min(Math.max(0,value),1-zone.height);
    if(key==='width')next.width=Math.min(Math.max(0.02,value),1-zone.x);
    if(key==='height')next.height=Math.min(Math.max(0.02,value),1-zone.y);
    if(key==='feather')next.feather=Math.min(Math.max(0,value),1);
    if(key==='z')next.z=Math.min(Math.max(-1000,Math.trunc(value)),1000);
    setZone(index,next);
  }
  function maskIds(){return inspection?.masks.filter(m=>m.enabled)??[];}
  function setMaskNode(nodeId:string){
    if(!editing)return;changed();
    const inputs=inspection?.masks.find(m=>m.nodeId===nodeId)?.inputs as Record<string,unknown>|undefined;
    if(nodeId&&inputs){
      editing.manifest.mask={nodeId,direction:String(inputs.direction)==='horizontal'?'horizontal':'vertical',split:Number(inputs.split)||50,blur:Number(inputs.blur)||0,swap:inputs.swap===true};
    }else editing.manifest.mask=null;
  }
  function currentManifest():SceneManifest|null{
    if(!editing||!composition)return null;
    return buildManifest({
      grokSource:editing.manifest.grokSource,
      workflowId:editing.manifest.workflowId,composition,
      p1:editing.manifest.p1,
      p2:coupled?editing.manifest.p2??{characterId:'',variantId:null}:null,
      action:editing.manifest.action,bg:editing.manifest.bg,
      quality:editing.manifest.quality??null,negative:editing.manifest.negative??null,
      mask:editing.manifest.mask??null,
    });
  }
  function check(manifest:SceneManifest|null):manifest is SceneManifest{
    if(!manifest)return false;
    const problems=validateManifest(manifest);
    if(problems.length){error=problems.join(' ');return false;}
    return true;
  }
  async function assemble(){
    error='';note='';
    const manifest=currentManifest();
    if(!check(manifest))return;
    if(busy)return;busy=true;
    try{preview=await call<Preview>('preview_scene',{manifest});}
    catch(e){error=message(e);preview=null;}
    finally{busy=false;}
  }
  async function save(asCopy:boolean){
    error='';note='';
    const manifest=currentManifest();
    if(!check(manifest))return;
    if(busy)return;busy=true;
    try{
      const base=editing?.name.trim()||$t("Scène sans nom");
      const name=asCopy?`${base} (copie)`:base;
      // A copy always creates a new manifest, even from unsaved edits.
      const id=await call<string>('save_scene',{id:asCopy?null:editing?.id??null,name,manifest});
      if(editing&&!asCopy){editing.id=id;editing.name=name;savedSnap=snap();}
      await refresh();
      note=asCopy?$t("Copie enregistrée comme nouvelle scène ; l'édition courante reste inchangée."):$t("Scène enregistrée. Les mappings sont revérifiés à chaque aperçu.");
    }catch(e){error=message(e);}finally{busy=false;}
  }
  async function copyFromList(id:string){
    error='';note='';if(busy)return;busy=true;
    try{await call('duplicate_scene',{id});await refresh();note=$t("Copie enregistrée depuis la scène stockée.");}
    catch(e){error=message(e);}finally{busy=false;}
  }
  async function remove(id:string){
    if(deleteArmed!==id){deleteArmed=id;return;}
    deleteArmed='';if(busy)return;busy=true;error='';
    try{
      await call('delete_scene',{id});
      if(editing?.id===id)editing=null;
      preview=null;prepared=null;
      await refresh();note=$t("Scène supprimée.");
    }catch(e){error=message(e);}finally{busy=false;}
  }
  async function open(id:string){
    if(busy)return;busy=true;error='';note='';preview=null;prepared=null;deleteArmed='';
    try{
      const record=await call<SceneRecord>('scene',{id});
      editing=record;
      resetWorkflowState();
      await chooseWorkflow(record.manifest.workflowId);
      for(const index of [0,1]){
        const selected=personId(index);
        if(selected){try{const variants=await call<Variant[]>('variants',{id:selected});personVariants[index]=variants;}catch(e){error=message(e);}}
      }
      savedSnap=snap();
    }catch(e){error=message(e);}finally{busy=false;}
  }
  function newScene(){
    if(!confirmDiscard())return false;
    editing={id:null,name:'',manifest:{workflowId:'',composition:'solo',p1:{characterId:'',variantId:null},action:sceneText('custom',null,''),bg:sceneText('workflow',null,'')}};
    resetWorkflowState();error='';note='';preview=null;prepared=null;savedSnap=snap();return true;
  }
  function useImporter(id:string,name:string,index:number,blocks:Record<string,string>){
    if(!newScene()||!editing)return;
    editing.name=name;editing.manifest.grokSource={sceneId:id,variantIndex:index};
    for(const [section,field] of [['ACTION','action'],['BG','bg'],['QUALITY','quality'],['NEG','negative']] as const){
      if(section in blocks)editing.manifest[field]=sceneText('custom',null,blocks[section]);
    }
    note=$t("Prompts Importer chargés sans modification. Sélectionnez le workflow et les personnages ; les mappings seront vérifiés dans l’aperçu.");
  }
  function setSource(section:Shared,source:string){
    if(!editing)return;changed();
    const value=source===''?null:sceneText(source as SceneTextSource,source==='preset'?'P1':null,'');
    if(section==='ACTION')editing.manifest.action=value??sceneText('custom',null,'');
    else if(section==='BG')editing.manifest.bg=value??sceneText('workflow',null,'');
    else if(section==='QUALITY')editing.manifest.quality=value;
    else editing.manifest.negative=value;
  }
  async function prepare(){
    if(!preview)return;error='';note='';
    if(busy)return;busy=true;
    try{
      prepared=await call<Prepared>('prepare',{id:preview.workflow.id,edits:preview.edits,characters:preview.jobCharacters,...(editing?.id?{origin:{kind:'scene',id:editing.id}}:{})});
      note=$t("Requête préparée comme brouillon. Aucun envoi sans votre confirmation ci-dessous.");
    }catch(e){error=message(e);prepared=null;}
    finally{busy=false;}
  }
  onMount(()=>{if(native)refresh();});
</script>
<div class="wb sc" data-scenes-page>
<aside class="wb-panel left">
 <header class="wb-head"><h2>{$t("Scènes")}</h2><span class="chip num">{scenes.length}</span><button disabled={busy} onclick={newScene}>{$t("Nouvelle scène")}</button></header>
 <div class="wb-scroll">
  <section class="lib" data-scene-library>
   {#if !native}<p class="note pad">{$t("Aperçu navigateur : ouvrez le binaire Saya Studio pour lire les scènes locales.")}</p>{/if}
   {#each scenes as scene (scene.id)}<div class="srow" class:on={editing?.id===scene.id} data-scene-entry={scene.id}>
    <button class="row-open" disabled={busy} onclick={()=>open(scene.id)} title={$t("Ouvrir charge une copie de travail : enregistrer ne met à jour la scène que de façon explicite, « Copier » duplique la version stockée.")}>{scene.name}</button>
    <span class="chip">{scene.composition}</span>
    {#if deleteArmed===scene.id}<button class="danger small" disabled={busy||!native} onclick={()=>remove(scene.id)}>{$t("Confirmer la suppression")}</button>
    {:else}<button class="icon-btn quiet" disabled={busy||!native} onclick={()=>copyFromList(scene.id)} title={$t("Dupliquer la scène stockée")} aria-label={$t("Copier")}><Icon name="copy" size={14} /></button><button class="icon-btn quiet" disabled={busy||!native} onclick={()=>remove(scene.id)} title={$t("Supprimer")} aria-label={$t("Supprimer")}><Icon name="trash" size={14} /></button>{/if}
   </div>{:else}<p class="note pad">{$t("Aucune scène enregistrée. Composez la première ci-dessous.")}</p>{/each}
  </section>
  <GrokScenes onuse={useImporter}/>
 </div>
</aside>

<section class="wb-center">
 <div class="wb-bar">
  <span class="bar-title">{$t("Positions et zones")}</span>
  {#if editing&&editing.manifest.workflowId&&inspection}
   <fieldset class="plain inline" disabled={busy}>{#each [0,1] as index}{#if index===0||coupled}<label class="check"><input type="checkbox" data-scene-zone-enable={index} checked={zoneFor(index)?.enabled??false} onchange={(e)=>toggleZone(index,e.currentTarget.checked)} />{$t("Activer la zone P{n}",{n:index+1})}</label>{/if}{/each}</fieldset>
  {/if}
  <span class="grow"></span>
  {#if busy}<span class="muted" role="status" aria-live="polite">{$t("Traitement en cours… Les commandes seront disponibles à la fin.")}</span>{/if}
 </div>
 {#if error||note}<div class="msgs">{#if error}<p class="error" role="alert">{error}</p>{/if}{#if note}<p class="note" role="status">{note}</p>{/if}</div>{/if}
 <div class="wb-scroll canvas">
 {#if editing&&editing.manifest.workflowId&&inspection}
  <fieldset class="plain stage-wrap" disabled={busy} title={$t("Coordonnées normalisées (0..1), ordre z croissant. La zone est une donnée de composition ; elle ne crée aucun node et ne modifie aucun prompt. Le masque SayaSplitMask reste contrôlé séparément ci-dessous.")}>
   <div class="scene-stage" role="application" aria-label={$t("Positions des personnages")} data-scene-stage onpointermove={zoneMove} onpointerup={zoneUp} onpointercancel={zoneUp}>
   {#each [0,1] as index}{#if (index===0||coupled)&&zoneFor(index)?.enabled}
   {@const zone=zoneFor(index)!}
   <div class="scene-zone" role="button" tabindex="-1" class:scene-zone-p2={index===1} data-scene-zone={index} style="left:{zone.x*100}%;top:{zone.y*100}%;width:{zone.width*100}%;height:{zone.height*100}%;z-index:{zone.z+1000}"
    onpointerdown={(e)=>zoneDown(e,index,'move')}>
   <span>P{index+1}</span><span class="scene-zone-resize" role="button" tabindex="-1" data-scene-zone-resize={index} onpointerdown={(e)=>{e.stopPropagation();zoneDown(e,index,'resize')}}></span>
   </div>
   {/if}{/each}
   </div>
   {#each [0,1] as index}{#if (index===0||coupled)&&zoneFor(index)}
   <div class="zone-fields" class:p2={index===1}>
    <span class="zone-tag">P{index+1}</span>
    <label>X<input class="num" data-scene-zone-field="{index}-x" type="number" min="0" max="1" step="0.01" value={zoneFor(index)!.x} oninput={(e)=>zoneNumber(index,'x',Number(e.currentTarget.value))} /></label>
    <label>Y<input class="num" data-scene-zone-field="{index}-y" type="number" min="0" max="1" step="0.01" value={zoneFor(index)!.y} oninput={(e)=>zoneNumber(index,'y',Number(e.currentTarget.value))} /></label>
    <label>{$t("Largeur")}<input class="num" data-scene-zone-field="{index}-width" type="number" min="0.02" max="1" step="0.01" value={zoneFor(index)!.width} oninput={(e)=>zoneNumber(index,'width',Number(e.currentTarget.value))} /></label>
    <label>{$t("Hauteur")}<input class="num" data-scene-zone-field="{index}-height" type="number" min="0.02" max="1" step="0.01" value={zoneFor(index)!.height} oninput={(e)=>zoneNumber(index,'height',Number(e.currentTarget.value))} /></label>
    <label>{$t("Feather")}<input class="num" data-scene-zone-field="{index}-feather" type="number" min="0" max="1" step="0.01" value={zoneFor(index)!.feather} oninput={(e)=>zoneNumber(index,'feather',Number(e.currentTarget.value))} /></label>
    <label>{$t("Ordre z")}<input class="num" data-scene-zone-field="{index}-z" type="number" min="-1000" max="1000" step="1" value={zoneFor(index)!.z} oninput={(e)=>zoneNumber(index,'z',Number(e.currentTarget.value))} /></label>
   </div>
   {/if}{/each}
  </fieldset>
 {:else}<p class="empty">{editing?$t("Choisissez un workflow dont le mapping est enregistré."):$t("Ouvrez une scène ou créez-en une nouvelle.")}</p>{/if}

 {#if preview}
 <section class="result" data-scene-preview-result>
  <h3>{$t("Aperçu complet avant application")}</h3>
  <p class="note">{preview.scope}</p>
  {#each preview.warnings as warning}<p class="warn">{warning}</p>{/each}
  <h4>{$t("Personnages prévus")}</h4>
  <ul>{#each preview.characters as person}<li data-scene-preview-person={person.role}>{$t("{role} : {name} (rév. {revision})",{role:person.role,name:person.name,revision:person.revision})}{person.variant?` · ${$t("variante {name}",{name:person.variant.name})}`:''}</li>{/each}</ul>
  {#each Object.keys(preview.prompts) as section}
  <div class="prompt" data-scene-prompt={section}>
   <h4>{section} · node {preview.prompts[section].nodeId}.{preview.prompts[section].inputName}{preview.prompts[section].changed?' · '+$t("modifié"):' · '+$t("conservé")}</h4>
   {#if preview.prompts[section].changed}<p class="lbl">{$t("Valeur actuelle du workflow :")}</p><pre class="before">{preview.prompts[section].before}</pre>{/if}
   <p class="lbl" title={$t("Texte intégral qui sera écrit (espaces, poids et retours ligne préservés) :")}>{$t("Résultat")}</p>
   <pre>{preview.prompts[section].text}</pre>
  </div>
  {/each}
  {#if preview.zones}<h4>{$t("Zones")}</h4>{#each [{role:'P1',zone:preview.zones.P1},{role:'P2',zone:preview.zones.P2}] as item}{#if item.zone}<p class="num" data-scene-preview-zone={item.role}>{item.role} · x {item.zone.x} · y {item.zone.y} · {item.zone.width}×{item.zone.height} · feather {item.zone.feather} · z {item.zone.z} · {item.zone.enabled?$t("active"):$t("désactivée")}</p>{/if}{/each}{#if preview.overlaps?.length}{#each preview.overlaps as overlap}<p class="warn" data-scene-overlap>{$t("{a} et {b} se chevauchent · z {za}/{zb} · visible {visible} · aire {area}",{a:overlap.a,b:overlap.b,za:overlap.aZ,zb:overlap.bZ,visible:String(overlap.visible),area:Math.round(overlap.area*1000)/1000})}</p>{/each}{/if}{/if}
  {#if preview.mask}<h4>{$t("Masque")}</h4><p data-scene-preview-mask>{preview.mask.direction==='vertical'?$t("Gauche / droite"):$t("Haut / bas")} · {$t("séparation {split} % · flou {blur} · inversion {swap} · node {node}",{split:preview.mask.split,blur:preview.mask.blur,swap:preview.mask.swap?$t("oui"):$t("non"),node:preview.mask.nodeId})}</p>{/if}
  <h4>{$t("Modifications prévues")}</h4>
  {#if preview.changes.length}<ul>{#each preview.changes as change}<li>{change.section??$t("masque")} · {change.nodeId}.{change.inputName} : « {change.before} » → « {change.after} »</li>{/each}</ul>{:else}<p class="note">{$t("Aucune différence avec les valeurs actuelles du workflow.")}</p>{/if}
  <div class="row end"><button class="primary" data-scene-prepare disabled={busy} onclick={prepare}>{$t("Préparer la requête du runner (sans envoi)")}</button></div>
 </section>
 {/if}
 {#if prepared}
 <section class="result" data-scene-prepared>
  <h3>{$t("Requête préparée")}</h3>
  <PreparedDraft {prepared} onsubmitted={job=>note=$t("Job {job} transmis à ComfyUI.",{job})} />
 </section>
 {/if}
 </div>
</section>

<aside class="wb-panel right">
{#if editing}
<section class="editor" data-scene-editor>
 <header class="wb-head"><h2>{editing.id?$t("Édition"):$t("Nouvelle scène")}{editing.name?` · ${editing.name}`:''}</h2>{#if dirty}<span class="chip">{$t("modifié")}</span>{/if}<button class="icon-btn quiet" disabled={busy} onclick={closeEditor} title={$t("Fermer l’édition")} aria-label={$t("Fermer l’édition")}><Icon name="close" size={14} /></button></header>
 <div class="wb-scroll">
  <div class="sec-body top">
   <label class="stack">{$t("Nom de la scène")}<input maxlength="300" bind:value={editing.name} oninput={changed} /></label>
   <label class="stack">{$t("Workflow cible")}<select data-scene-workflow disabled={busy} value={editing.manifest.workflowId} onchange={(e)=>chooseWorkflow(e.currentTarget.value)}><option value="">{$t("Choisir un workflow mappé")}</option>{#each workflowOptions as item (item.id)}<option value={item.id}>{item.name} · {item.promptProfile==='solo'?'SOLO':'COUPLE'}</option>{/each}</select></label>
   {#if editing.manifest.workflowId&&inspection}<p class="note" title={$t("Composition imposée par le profil de mapping enregistré : {profile}. Aucun mode interne n'est créé ni modifié.",{profile:inspection.promptProfile==='solo'?$t("SOLO (P1 seul)"):$t("COUPLE (P1 et P2)")})}><span class="chip">{inspection.promptProfile==='solo'?$t("SOLO (P1 seul)"):$t("COUPLE (P1 et P2)")}</span></p>{/if}
  </div>
  {#if editing.manifest.workflowId&&inspection}
  <details class="sec" open><summary><span class="sec-title">{$t("Personnages")}</span></summary>
  <fieldset class="plain sec-body" disabled={busy}>
  {#each [0,1] as index}{#if index===0||coupled}<section class="person" data-scene-person={index}><h3>P{index+1}{index===1?' · '+$t("obligatoire en COUPLE"):''}</h3>
  <label class="field-row"><span>{$t("Fiche")}</span><select data-scene-person-select={index} disabled={busy} value={personId(index)} onchange={(e)=>{const value=e.currentTarget.value;if(index===0)editing!.manifest.p1.characterId=value;else{if(editing!.manifest.p2)editing!.manifest.p2.characterId=value;else editing!.manifest.p2={characterId:value,variantId:null};}choosePerson(index);}}><option value="">{$t("Aucune fiche")}</option>{#each characters as character (character.id)}<option value={character.id}>{character.name}</option>{/each}</select></label>
  <label class="field-row" title={$t("Le prompt P{n} vient exactement de la section P1 de la variante choisie, sinon du texte d'identité de la fiche.",{n:index+1})}><span>{$t("Variante")}</span><select disabled={busy} value={(index===0?editing.manifest.p1.variantId:editing.manifest.p2?.variantId)??''} onchange={(e)=>{const value=e.currentTarget.value;if(index===0)editing!.manifest.p1.variantId=value||null;else if(editing!.manifest.p2)editing!.manifest.p2.variantId=value||null;changed();}}><option value="">{$t("Identité de la fiche")}</option>{#each personVariants[index] as variant (variant.id)}<option value={variant.id}>{variant.name} · {$t("section P1")}</option>{/each}</select></label>
  {#if index===0}{#each characters.filter(c=>c.id===editing?.manifest.p1.characterId) as character (character.id)}<CharacterContext {character} variant={editing?.manifest.p1.variantId??''} />{/each}{/if}
  {#if index===1}{#each characters.filter(c=>c.id===editing?.manifest.p2?.characterId) as character (character.id)}<CharacterContext {character} variant={editing?.manifest.p2?.variantId??''} />{/each}{/if}
  </section>{/if}{/each}
  </fieldset></details>
  <details class="sec" open><summary title={$t("Seules les sections mappées dans le workflow sont proposées. Une source « preset » sans bloc enregistré dans la variante est refusée avec un diagnostic ; rien n'est inventé.")}><span class="sec-title">{$t("Sections partagées")}</span></summary>
  <fieldset class="plain sec-body" disabled={busy}>
  {#each sharedSections as section}
  {#if section==='ACTION'||section==='BG'||mappedShared.has(section)}
  <div class="scene-section" data-scene-section={section}>
  <label class="field-row"><span>{section}</span><select disabled={busy} aria-label={$t("{section} · source",{section})} value={sourceOf(section)} onchange={(e)=>setSource(section,e.currentTarget.value)}>
  {#if section==='ACTION'}<option value="custom">{$t("Prompt personnalisé")}</option><option value="preset">{$t("Action enregistrée (preset de variante)")}</option>
  {:else if section==='BG'}<option value="workflow">{$t("Conserver le BG du workflow")}</option><option value="preset">{$t("Décor enregistré (preset de variante)")}</option><option value="custom">{$t("Description personnalisée")}</option>
  {:else}<option value="">{$t("Ne pas inclure")}</option><option value="workflow">{$t("Conserver la valeur du workflow")}</option><option value="preset">{$t("Preset de variante")}</option><option value="custom">{$t("Texte personnalisé")}</option>{/if}
  </select></label>
  {#if sourceOf(section)==='preset'&&coupled}<label class="field-row"><span>{$t("Rôle du preset")}</span><select disabled={busy} value={spec(section)?.from??'P1'} onchange={(e)=>{const value=spec(section);if(value)value.from=e.currentTarget.value;changed();}}><option value="P1">{$t("Variante de P1")}</option><option value="P2">{$t("Variante de P2")}</option></select></label>{/if}
  {#if sourceOf(section)==='custom'}<label class="stack">{$t("Texte exact {section}",{section})}<textarea rows="3" disabled={busy} value={spec(section)?.text??''} oninput={(e)=>{const value=spec(section);if(value)value.text=e.currentTarget.value;changed();}}></textarea></label>{/if}
  </div>
  {/if}
  {/each}
  </fieldset></details>
  <details class="sec" open={!!editing.manifest.mask}><summary><span class="sec-title">{$t("Masque SayaSplitMask")}</span>{#if editing.manifest.mask}<span class="chip">{$t("Node {id}",{id:editing.manifest.mask.nodeId})}</span>{/if}</summary>
  <fieldset class="plain sec-body" disabled={busy}>
  {#if !maskIds().length}<p class="note">{$t("Aucun masque mappé dans ce workflow : les paramètres du masque restent ceux du workflow.")}</p>{:else}
  <label class="field-row"><span>{$t("Node")}</span><select data-scene-mask disabled={busy} value={editing.manifest.mask?.nodeId??''} onchange={(e)=>setMaskNode(e.currentTarget.value)}><option value="">{$t("Conserver les masques du workflow")}</option>{#each maskIds() as item (item.nodeId)}<option value={item.nodeId}>{$t("Node {id}",{id:item.nodeId})}</option>{/each}</select></label>
  {#if editing.manifest.mask}<label class="field-row"><span>{$t("Répartition")}</span><select data-scene-mask-direction disabled={busy} value={editing.manifest.mask.direction} onchange={(e)=>{editing!.manifest.mask!.direction=e.currentTarget.value==='horizontal'?'horizontal':'vertical';changed();}}><option value="vertical">{$t("Gauche / droite")}</option><option value="horizontal">{$t("Haut / bas")}</option></select></label>
  <label class="field-row"><span>{$t("Séparation (%)")}</span><input class="num" type="number" min="0" max="100" step="1" disabled={busy} value={editing.manifest.mask.split} oninput={(e)=>{editing!.manifest.mask!.split=Number(e.currentTarget.value);changed();}} /></label>
  <label class="field-row"><span>{$t("Flou (sigma pixels)")}</span><input class="num" type="number" min="0" step="0.1" disabled={busy} value={editing.manifest.mask.blur} oninput={(e)=>{editing!.manifest.mask!.blur=Number(e.currentTarget.value);changed();}} /></label>
  <label class="check"><input type="checkbox" disabled={busy} checked={editing.manifest.mask.swap} onchange={(e)=>{editing!.manifest.mask!.swap=e.currentTarget.checked;changed();}} />{$t("Inverser les deux masques (les prompts ne sont pas permutés)")}</label>{/if}
  {/if}
  </fieldset></details>
  {/if}
 </div>
 {#if editing.manifest.workflowId&&inspection}
 <div class="wb-actions">
  <button class:primary={!preview} data-scene-assemble disabled={busy} onclick={assemble}>{$t("Assembler l'aperçu complet")}</button>
  <div class="row two"><button disabled={busy} onclick={()=>save(false)}>{editing.id?$t("Enregistrer la scène"):$t("Enregistrer la nouvelle scène")}</button><button disabled={busy} onclick={()=>save(true)}>{$t("Enregistrer une copie")}</button></div>
 </div>
 {/if}
</section>
{:else}<p class="empty">{$t("Aucune scène ouverte.")}</p>{/if}
</aside>
</div>

<style>
.sc{--wb-left:300px;--wb-right:340px;height:100%;min-height:520px;overflow:hidden}
h2{font-size:13px;font-weight:600;margin:0}
h3{font-size:12.5px;font-weight:600;margin:0}
h4{font-size:11px;font-weight:600;color:var(--muted);margin:10px 0 4px;text-transform:uppercase;letter-spacing:.04em}
label{color:var(--muted);font-size:12px}
.stack{display:flex;flex-direction:column;gap:4px}
.check{display:flex;flex-direction:row;align-items:center;gap:6px;color:var(--text);white-space:nowrap}
.field-row{display:grid;grid-template-columns:90px minmax(0,1fr);align-items:center;gap:8px}
.field-row>*{min-width:0}
.muted{color:var(--muted)}.grow{flex:1}
.note{color:var(--muted);margin:0;overflow-wrap:anywhere}.note.pad{padding:12px}
.warn{color:var(--warning);margin:0;overflow-wrap:anywhere}
.error{color:var(--danger);margin:0;overflow-wrap:anywhere}
.empty{color:var(--muted);text-align:center;padding:48px 16px;margin:0}
.msgs{display:grid;gap:4px;padding:8px 12px;border-bottom:1px solid var(--line);background:var(--bg)}
fieldset.plain{border:0;margin:0;min-width:0}
fieldset.inline{display:flex;gap:14px;padding:0}
.row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}.row.end{justify-content:flex-end}
.row.two>button{flex:1}
.bar-title{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);margin-inline-end:6px}
.srow{display:flex;align-items:center;gap:4px;padding:0 8px 0 12px;min-height:34px;border-bottom:1px solid var(--line)}
.srow:hover{background:var(--hover)}
.srow.on{background:var(--raised);box-shadow:inset 2px 0 0 var(--accent)}
.row-open{flex:1;min-width:0;background:transparent;border:0;padding:0;text-align:start;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--text)}
.row-open:hover:not(:disabled){background:transparent}
.small{min-height:24px;font-size:11.5px;padding:0 8px}
.canvas{background:var(--canvas)}
.stage-wrap{padding:16px;display:grid;justify-items:center;gap:6px}
.scene-stage{position:relative;width:min(100%,calc(100vh - 290px));aspect-ratio:1/1;min-width:240px;border:1px solid var(--line-strong);border-radius:var(--r-img);background-color:var(--bg);background-image:linear-gradient(var(--line) 1px,transparent 1px),linear-gradient(90deg,var(--line) 1px,transparent 1px);background-size:10% 10%;overflow:hidden;touch-action:none;user-select:none}
.scene-zone{position:absolute;border:1.5px solid var(--sel);border-radius:3px;cursor:move;background:color-mix(in srgb,var(--sel) 8%,transparent)}
.scene-zone-p2{border-color:var(--accent);background:color-mix(in srgb,var(--accent) 8%,transparent)}
.scene-zone span:first-child{position:absolute;inset-inline-start:6px;top:4px;font-size:11px;font-weight:600;color:var(--text)}
.scene-zone-resize{position:absolute;right:-5px;bottom:-5px;width:10px;height:10px;border-radius:2px;background:var(--sel);cursor:nwse-resize}
.scene-zone-p2 .scene-zone-resize{background:var(--accent)}
.zone-fields{display:grid;grid-template-columns:28px repeat(6,minmax(0,1fr));gap:6px;align-items:end;width:min(100%,calc(100vh - 290px));min-width:240px}
.zone-fields label{display:flex;flex-direction:column;gap:2px;font-size:10.5px;color:var(--faint)}
.zone-fields input{min-width:0;width:100%;min-height:26px;padding:0 4px}
.zone-tag{font-size:11px;font-weight:600;color:var(--sel);align-self:center}
.zone-fields.p2 .zone-tag{color:var(--accent)}
.result{margin:0 16px 16px;padding:12px;border:1px solid var(--line);border-radius:var(--radius-lg);background:var(--bg);display:grid;gap:4px}
.result ul{margin:0;padding-inline-start:18px}
.result p{margin:0}
.prompt{border-top:1px solid var(--line);padding-top:4px}
.lbl{font-size:11px;color:var(--faint)}
pre{font-size:11.5px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:6px 8px;margin:0 0 4px;max-height:220px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}
pre.before{color:var(--muted)}
.editor{display:flex;flex-direction:column;min-height:0;flex:1}
.top{padding-top:10px;border-bottom:1px solid var(--line)}
.person{display:grid;gap:6px}
.person+.person{border-top:1px solid var(--line);padding-top:8px}
.scene-section{display:grid;gap:6px}
.wb-actions{border-top:1px solid var(--line)}
@media (max-width:1200px){.sc{--wb-left:260px;--wb-right:300px}}
@media (max-width:980px){.sc{grid-template-columns:minmax(0,1fr);height:auto}}
</style>
