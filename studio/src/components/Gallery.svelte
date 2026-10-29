<script lang="ts">
  import { t, tn, locale } from '../lib/i18n';
  import GenerationMetadata from './GenerationMetadata.svelte';
  import AssetActions from './AssetActions.svelte';
  import LibrarySources from './LibrarySources.svelte';
  import { onMount, onDestroy, tick } from 'svelte';
  import { call, message, native, imageSource, type Asset, type Character } from '../lib/api';
  import { restoreFilters, gallerySorts, reviewLabels, captureLabel, defaultCaptureRoot } from '../lib/gallery';
  import { thumbCache, previewCache } from '../lib/stream';
  import Thumbnail from './Thumbnail.svelte';
  import ImageComparison from './ImageComparison.svelte';
  import StreamGrid from './StreamGrid.svelte';
  import VisualSearch from './VisualSearch.svelte';
  import Icon from './Icon.svelte';
  let {target=null,oncharacter=()=>{},onedit=()=>{},ontree=()=>{},oncompare=()=>{}}:{target?:{character:string;asset:string|null}|null;oncharacter?:(id:string)=>void;onedit?:(asset:Asset)=>void;ontree?:(job:string)=>void;oncompare?:(job:string)=>void}=$props();
  let images = $state<Asset[]>([]), characters = $state<Character[]>([]);
  let total = $state(0), busy = $state(false), loading = $state(false), error = $state(''), note = $state('');
  let root = $state(''), initializing=$state(true);
  interface IndexRun {id:number;root:string;state:string;completed:number;total:number;indexed:number;unchanged:number;errorCount:number;lastPath:string|null;updatedAt:string}
  let progress = $state({ running: false, cancel_requested:false, completed: 0, total: 0, error: null as string | null,lastRun:null as IndexRun|null });
  let indexErrors=$state<{id:number;path:string;message:string}[]>([]),indexErrorRun=$state(0),indexErrorsBusy=$state(false),indexErrorsMore=$state(false);
  async function showIndexErrors(reset=false){
    const run=progress.lastRun?.id;if(!run||indexErrorsBusy)return;
    indexErrorsBusy=true;
    try{const offset=reset||indexErrorRun!==run?0:indexErrors.length;const page=await call<typeof indexErrors>('index_errors',{run,offset});if(progress.lastRun?.id===run){indexErrors=offset?[...indexErrors,...page]:page;indexErrorRun=run;indexErrorsMore=page.length===100;}}
    catch(e){error=message(e);}finally{indexErrorsBusy=false;}
  }
  async function resumeIndex(){if(progress.lastRun){root=progress.lastRun.root;await index();}}
  interface Choice { id:string; name:string }
  let facets = $state<{ checkpoint:string[]; lora:string[]; variant:Choice[]; workflow:Choice[]; job:Choice[] }>({checkpoint:[],lora:[],variant:[],workflow:[],job:[]});
  let characterChoice=$state('');
  let filters = $state(restoreFilters(null)), loraChoice = $state('');
  // The capture root is set in Settings (data-settings-capture-root);
  // the gallery only keeps initializing the default on first open.
  let captureRoot = $state(''), captureRootDefault = $state('');
  let activeFilterCount = $derived((filters.metadataOnly?1:0)+(filters.availability?1:0)+filters.checkpoints.length+(filters.loras.length+(filters.lora?1:0))+(filters.loraExact?1:0)+(filters.character?1:0)+filters.characters.length+(filters.search?1:0)+(filters.metadataState?1:0)+(filters.review?1:0)+(filters.favorite?1:0)+(filters.duplicates?1:0)+(filters.variant?1:0)+(filters.workflow?1:0)+(filters.job?1:0)+(filters.capture==='all'?1:0));
  function toggleCheckpoint(value:string){filters.checkpoints=filters.checkpoints.includes(value)?filters.checkpoints.filter(v=>v!==value):[...filters.checkpoints,value].slice(0,32);load(true);}
  let selected = $state<Asset | null>(null), detail = $state<unknown>(null), associateId = $state(''), annotationBusy = $state(false);
  let comparison=$state<Asset[]>([]),comparing=$state(false);
  let selectionMode=$state(false),selection=$state<Asset[]>([]);
  let selectionTimer:ReturnType<typeof setTimeout>|undefined,selectionWrite=Promise.resolve();
  function persistSelection(value:{mode:boolean;ids:string[]}){selectionWrite=selectionWrite.then(()=>call('set_preference',{key:'gallerySelection',value})).then(()=>{}).catch(e=>{error=message(e)});}
  $effect(()=>{const value={mode:selectionMode,ids:selection.map(a=>a.id)};if(initializing)return;clearTimeout(selectionTimer);selectionTimer=setTimeout(()=>persistSelection(value),150)});
  onDestroy(()=>{clearTimeout(selectionTimer);if(!initializing)persistSelection({mode:selectionMode,ids:selection.map(a=>a.id)});});
  function galleryClick(asset:Asset){if(!selectionMode){open(asset);return;}if(selection.some(a=>a.id===asset.id))selection=selection.filter(a=>a.id!==asset.id);else if(selection.length<1000)selection=[...selection,asset];else error=$t("Maximum 1000 images par opération.");}
  async function assetsChanged(){await lists();await load();streamRefresh++;}
  const availabilityLabels:Record<string,string>={present:'Présente',missing:'Manquante',unreadable:'Fichier inaccessible',source_inaccessible:'Source inaccessible',outside_roots:'Hors racines actives',removed:'Retirée du catalogue'};
  let similar=$state<Asset|null>(null);
  // Every active filter is visible and removable; nothing narrows the gallery silently.
  const metadataLabels:Record<string,string>={read:'présentes',absent:'absentes',invalid:'invalides',unknown:'inconnues'};
  const short=(v:string)=>v.split('/').pop()?.replace(/\.(safetensors|ckpt|pt)$/i,'')??v;
  const who=(id:string)=>characters.find(c=>c.id===id)?.name??$t("personnage inconnu");
  const chips=$derived.by(()=>{const f=filters,c:{key:string;label:string;clear:()=>void}[]=[];
    if(f.search)c.push({key:'search',label:$t("Recherche « {query} »",{query:f.search}),clear:()=>{filters.search=''}});
    if(f.character)c.push({key:'character',label:$t("Personnage : {name}",{name:who(f.character)}),clear:()=>{filters.character=''}});
    for(const id of f.characters)c.push({key:'c:'+id,label:$t("Personnage : {name}",{name:who(id)}),clear:()=>{filters.characters=filters.characters.filter(x=>x!==id)}});
    for(const v of f.checkpoints)c.push({key:'k:'+v,label:$t("Checkpoint : {name}",{name:short(v)}),clear:()=>{filters.checkpoints=filters.checkpoints.filter(x=>x!==v)}});
    for(const v of [...f.loras,...(f.lora?[f.lora]:[])])c.push({key:'l:'+v,label:$t("LoRA : {name}",{name:short(v)}),clear:()=>{filters.loras=filters.loras.filter(x=>x!==v);if(filters.lora===v)filters.lora=''}});
    if(f.loraExact)c.push({key:'exact',label:$t("Groupe exact de LoRA"),clear:()=>{filters.loraExact=false}});
    if(f.favorite)c.push({key:'fav',label:'★ '+$t("Favoris"),clear:()=>{filters.favorite=false}});
    if(f.review)c.push({key:'review',label:$t("Validation : {state}",{state:$t(reviewLabels[f.review]??f.review)}),clear:()=>{filters.review=''}});
    if(f.metadataState)c.push({key:'meta',label:$t("Métadonnées {state}",{state:$t(metadataLabels[f.metadataState]??f.metadataState)}),clear:()=>{filters.metadataState=''}});
    if(f.metadataOnly)c.push({key:'metaonly',label:$t("Avec métadonnées"),clear:()=>{filters.metadataOnly=false}});
    if(f.availability)c.push({key:'avail',label:$t("Fichiers : {state}",{state:$t(availabilityLabels[f.availability]??f.availability)}),clear:()=>{filters.availability=''}});
    if(f.duplicates)c.push({key:'dup',label:$t("Doublons"),clear:()=>{filters.duplicates=false}});
    if(f.variant)c.push({key:'variant',label:$t("Variante : {name}",{name:facets.variant.find(v=>v.id===f.variant)?.name??f.variant}),clear:()=>{filters.variant=''}});
    if(f.workflow)c.push({key:'workflow',label:$t("Workflow : {name}",{name:facets.workflow.find(v=>v.id===f.workflow)?.name??f.workflow}),clear:()=>{filters.workflow=''}});
    if(f.job)c.push({key:'job',label:$t("Génération précise"),clear:()=>{filters.job=''}});
    if(f.capture==='all')c.push({key:'capture',label:$t("Passes intermédiaires affichées"),clear:()=>{filters.capture=''}});
    return c});
  // "Kept" view = main view (PASS 1 or FINAL), "All" = with intermediate passes.
  function setView(all:boolean){filters.capture=all?'all':'main';load(true);}
  // Thumbnail size: per-machine comfort (localStorage), no server preference.
  let thumbSize=$state(184);
  try{const v=Number(localStorage.getItem('saya.gallery.thumbSize'));if(v>=96&&v<=420)thumbSize=v;}catch{/* stockage indisponible */}
  function saveThumbSize(){try{localStorage.setItem('saya.gallery.thumbSize',String(thumbSize));}catch{/* stockage indisponible */}}
  const fileName=(path:string)=>path.split(/[\\/]/).pop()??path;
  function clearAll(){filters=restoreFilters({sort:filters.sort});load(true);}
  // Continuous scrolling is the default view; pagination stays one click away.
  // The choice persists in the `galleryView` preference ('stream' | 'pages').
  // streamRefresh reloads the stream in place after data changes.
  let streaming = $state(false), streamRefresh = $state(0);
  function addComparison(){if(!selected||comparison.length>=2||comparison.some(a=>a.id===selected!.id))return;comparison=[...comparison,{...selected}];}
  let selectedCharacters = $derived(characters.filter(c => selected && c.imageIds.includes(selected.id)));
  let serial = 0, detailSerial = 0, searchTimer: ReturnType<typeof setTimeout> | undefined;
  async function load(reset = false) {
    const ticket = ++serial; if (reset) filters.offset = 0; error = ''; loading = true;
    const snapshot = $state.snapshot(filters);
    try {
      const page = await call<{ items: Asset[]; total: number }>('gallery', { filters: snapshot });
      if (ticket !== serial) return;
      if (snapshot.offset >= page.total && snapshot.offset > 0) { filters.offset = Math.max(0, Math.floor((page.total - 1) / 48) * 48); await load(); return; }
      images = page.items; total = page.total;
      await call('set_preference', { key: 'galleryFilters', value: snapshot });
    } catch (e) { if (ticket === serial) error = message(e); }
    finally { if (ticket === serial) loading = false; }
  }
  function search() { clearTimeout(searchTimer); searchTimer = setTimeout(() => load(true), 200); }
  async function lists() { [facets, characters] = await Promise.all([call<typeof facets>('facets'), call<Character[]>('characters',{include_archived:true})]); }
  async function index(path=root):Promise<boolean> {
    if(initializing||!path.trim())return true;
    root=path;
    if (busy) return false; busy = true; error = ''; note = '';
    try {
      const r = await call<{ cancelled:boolean; indexed: number; unchanged: number; errors: string[]; errorCount:number; characters: number }>('index', { root });
      note = `${r.cancelled?$t("Indexation interrompue proprement")+' · ':''}${$t("{indexed} images indexées · {unchanged} inchangées · {characters} personnages reliés · {errors} erreurs.",{indexed:r.indexed,unchanged:r.unchanged,characters:r.characters,errors:r.errorCount})}${r.errors.length ? '\n' + $t("Erreurs : {list}",{list:r.errors.join('\n')}) : ''}`;
      await lists(); await load(); streamRefresh++;
      return r.cancelled;
    } catch (e) { error = message(e); return true; } finally { busy = false; }
  }
  interface Neighbors {previous:Asset|null;current:Asset|null;next:Asset|null;position:number;total:number;found:boolean}
  let neighbors=$state<Neighbors|null>(null),navBusy=$state(false),navSerial=0;
  let prefetchController=new AbortController(),originRefresh=0;
  let viewerFilters=restoreFilters(null),originScroll:{node:Element;top:number;left:number}[]=[],originFocus:HTMLElement|null=null;
  // V1: a click in the grid shows the image in the inspector (`selected`);
  // the fullscreen viewer (`viewerOpen`) opens from the inspector preview.
  let viewerOpen=$state(false);
  function showViewer(){if(!selected||viewerOpen)return;originRefresh=streamRefresh;originFocus=document.activeElement instanceof HTMLElement?document.activeElement:null;originScroll=[...document.querySelectorAll('#main-content,.stream-viewport'),document.scrollingElement!].filter(Boolean).map(node=>({node,top:node.scrollTop,left:node.scrollLeft}));viewerOpen=true;}
  async function closeViewer(){if(!viewerOpen)return;viewerOpen=false;await tick();for(const v of originScroll)if(!v.node.classList.contains("stream-viewport")||originRefresh===streamRefresh)v.node.scrollTo({top:v.top,left:v.left,behavior:'instant'});originFocus?.focus({preventScroll:true});}
  function clearInspector(){viewerOpen=false;selected=null;++detailSerial;++navSerial;neighbors=null;prefetchController.abort();}
  async function open(asset: Asset) {
    if(!viewerOpen)viewerFilters=restoreFilters($state.snapshot(filters));
    prefetchController.abort();prefetchController=new AbortController();
    const ticket=++detailSerial;const navTicket=++navSerial;selected={...asset};detail=null;error='';navBusy=true;
    const fallback=neighbors?.position??0;
    try{const [result,context]=await Promise.all([call('detail',{id:asset.id}),call<Neighbors>('gallery_neighbors',{filters:viewerFilters,id:asset.id,fallback})]);if(ticket!==detailSerial)return;detail=result;neighbors=context;
      if(!context.found)error=$t("Cette image ne fait plus partie des résultats. La navigation utilise la position la plus proche.");
      const signal=prefetchController.signal;
      for(const neighbor of [context.previous,context.next])if(neighbor&&!thumbCache.get(neighbor.id))imageSource(neighbor.id,false,signal).then(value=>{if(navTicket===navSerial)thumbCache.set(neighbor.id,value)}).catch(()=>{});
      // 1600 px preview of the next neighbour: sequential navigation
      // swaps the image without waiting for its decoding. Bounded RAM cache (LRU 8).
      for(const upcoming of [context.previous,context.next])
        if(upcoming&&!previewCache.get('p:'+upcoming.id))imageSource(upcoming.id,true,signal).then(value=>{if(navTicket===navSerial)previewCache.set('p:'+upcoming.id,value)}).catch(()=>{});
    }catch(e){if(ticket===detailSerial)error=message(e);}finally{if(ticket===detailSerial)navBusy=false;}
  }
  async function navigate(direction:'previous'|'next'){
    if(navBusy||!selected)return;
    // Point 6: the viewer context is already anchored on the opened
    // image (neighbors.previous/next) — navigation uses it directly
    // and open() re-anchors: a single gallery_neighbors per navigation instead
    // of two. The fallback path covers an id removed from the results.
    const target=neighbors?.found?neighbors[direction]:null;
    if(target){await open(target);return;}
    navBusy=true;const id=selected.id,ticket=detailSerial;
    try{const context=await call<Neighbors>('gallery_neighbors',{filters:viewerFilters,id,fallback:neighbors?.position??0});if(ticket!==detailSerial)return;neighbors=context;const fallbackTarget=context.found?context[direction]:context.current;if(fallbackTarget)await open(fallbackTarget);}
    catch(e){if(ticket===detailSerial)error=message(e);}finally{if(ticket===detailSerial)navBusy=false;}
  }
  function viewerKey(event:KeyboardEvent){
    if(!selected||!viewerOpen||event.altKey||event.ctrlKey||event.metaKey||event.shiftKey||event.defaultPrevented)return;
    const target=event.target;if(target instanceof Element&&target.closest('input,textarea,select,[contenteditable="true"],[role="textbox"],[role="slider"]'))return;
    if(event.key==='ArrowLeft'||event.key==='ArrowRight'){event.preventDefault();navigate(event.key==='ArrowLeft'?'previous':'next');}
  }
  let annotationNote = $state<{id:string;text:string}|null>(null);
  async function annotate() { if (!selected) return; annotationBusy = true; error = ''; annotationNote = null; try { const { id, favorite, note: annotation, review } = selected; await call('annotate', { id, favorite, note: annotation, review }); await load(); streamRefresh++; annotationNote = { id, text: $t("Annotations enregistrées") }; } catch (e) { error = message(e); } finally { annotationBusy = false; } }
  async function associate(id: string, add: boolean) { if (!selected || !id) return; try { await call('associate', { character: id, asset: selected.id, add }); await lists(); await load(); streamRefresh++; } catch (e) { error = message(e); } }
  function dialog(node: HTMLDialogElement) { node.showModal(); }
  // Locate the file (brief point 5): displayed path, clipboard copy
  // (execCommand fallback as in GenerationMetadata) and file manager.
  interface DetailPayload { availability: string; path: string; reason?: string | null }
  let pathCopied = $state(false);
  async function copyPath() {
    if (!detail) return;
    const text = (detail as DetailPayload).path ?? '';
    try {
      if (navigator.clipboard) await navigator.clipboard.writeText(text);
      else {
        const input = document.createElement('textarea'); input.value = text; document.body.append(input); input.select();
        const ok = document.execCommand('copy'); input.remove(); if (!ok) throw Error('Presse-papiers indisponible');
      }
      pathCopied = true; setTimeout(() => { pathCopied = false; }, 2000);
    } catch (e) { error = $t("Copie impossible : {error}", { error: message(e) }); }
  }
  async function openInManager() {
    if (!selected) return;
    try { await call('open_asset_directory', { id: selected.id }); } catch (e) { error = message(e); }
  }
  async function refreshPresence() {
    if (!selected) return;
    try { await call('refresh_asset_presence', { id: selected.id }); await open(selected); await assetsChanged(); } catch (e) { error = message(e); }
  }
  onMount(() => {
    if (!native) return;
    let alive = true, polling = false;
    (async () => { try { const prefs = await call<Record<string, unknown>>('preferences'); if (!alive) return; filters = restoreFilters(target?{character:target.character}:prefs.galleryFilters); filters.capture='main'; root=typeof prefs.libraryRoot==='string'?prefs.libraryRoot:''; if(typeof prefs.captureRoot==='string'&&prefs.captureRoot)captureRoot=prefs.captureRoot;
      // First open: the default (brief point 4) becomes the effective
      // preference; the Rust library never writes this choice itself.
      captureRootDefault=typeof prefs.defaultCaptureRoot==='string'?prefs.defaultCaptureRoot:defaultCaptureRoot;if(!captureRoot&&captureRootDefault){captureRoot=captureRootDefault;call('set_preference',{key:'captureRoot',value:captureRoot}).catch(e=>{if(alive)error=message(e)});}
      streaming=prefs.galleryView!=='pages'; const saved=prefs.gallerySelection as {mode?:boolean;ids?:string[]}|undefined;
      if(Array.isArray(saved?.ids)){const restored=await Promise.allSettled(saved.ids.slice(0,1000).filter(id=>typeof id==='string').map(id=>call<{asset:Asset}>('detail',{id})));if(!alive)return;selection=restored.flatMap(r=>r.status==='fulfilled'?[r.value.asset]:[]);selectionMode=!!saved.mode;}
      await lists(); await load(); void call<{removed:number}>('gallery_cleanup').then(async r=>{if(alive&&r.removed){await load();streamRefresh++;}}).catch(e=>{if(alive)error=message(e)}); if(target?.asset && alive){const value=await call<{asset:Asset}>('detail',{id:target.asset});if(alive){await open(value.asset);showViewer();}} } catch (e) { error = message(e); } finally {if(alive)initializing=false;} })();
    const timer = setInterval(async () => {
      if (polling) return; polling = true;
      try { const state = await call<typeof progress>('index_status'); if (!alive) return; const finished = progress.running && !state.running; progress = state; if (finished) { await call('gallery_cleanup'); await lists(); await load(); streamRefresh++; } }
      catch(e) { if (alive) error = message(e); } finally { polling = false; }
    }, 750);
    return () => { alive = false; ++serial; ++detailSerial; clearInterval(timer); clearTimeout(searchTimer); };
  });
</script><svelte:window onkeydown={viewerKey}/>
<div class="gallery wb">
<aside class="wb-panel left" aria-label={$t("Filtres")}>
  <div class="wb-head"><h1 class="vh">{$t("Galerie")}</h1><input class="search" aria-label={$t("Rechercher une image")} placeholder={$t("Rechercher…")} title={$t("Nom de fichier ou dossier")} bind:value={filters.search} oninput={search} /></div>
  <div class="wb-scroll">
    <div class="group"><span class="sec-title">{$t("Vue")}</span>
      <div class="seg" title={$t("Vue principale : une génération multipasse affiche PASSE 1 ou son image FINALE ; les passes du milieu restent dans les détails et comparaisons.")}><button aria-pressed={filters.capture!=='all'} onclick={()=>setView(false)}>{$t("Gardées")}</button><button aria-pressed={filters.capture==='all'} onclick={()=>setView(true)}>{$t("Tout")}</button></div>
    </div>
    <details class="sec" open><summary><span class="sec-title">{$t("Personnages")}</span>{#if filters.character||filters.characters.length}<span class="chip">{(filters.character?1:0)+filters.characters.length}</span>{/if}</summary><div class="sec-body">
      <label class="field"><span>{$t("Personnage")}</span><select bind:value={filters.character} onchange={() => load(true)}><option value="">{$t("Tous")}</option>{#each characters as c}<option value={c.id}>{c.name}{c.archived?' · '+$t("archivé"):''}</option>{/each}</select></label>
      <label class="field"><span>{$t("Ajouter un personnage (ET)")}</span><select bind:value={characterChoice} onchange={()=>{if(characterChoice&&!filters.characters.includes(characterChoice))filters.characters=[...filters.characters,characterChoice];characterChoice='';load(true);}}><option value="">{$t("Choisir…")}</option>{#each characters as c}<option value={c.id}>{c.name}{c.archived?' · '+$t("archivé"):''}</option>{/each}</select></label>
    </div></details>
    <details class="sec filter-drawer" open><summary><span class="sec-title">{$t("Modèles")}</span>{#if filters.checkpoints.length+filters.loras.length}<span class="chip">{filters.checkpoints.length+filters.loras.length}</span>{/if}</summary><div class="sec-body">
      <div class="list" data-checkpoint-filter title={$t("Plusieurs checkpoints = union ; les LoRA filtrent ensuite cet ensemble.")}>{#each facets.checkpoint as value}<label class="check"><input type="checkbox" checked={filters.checkpoints.includes(value)} onchange={()=>toggleCheckpoint(value)} />{value}</label>{/each}{#if !facets.checkpoint.length}<span class="muted">{$t("Aucun checkpoint indexé.")}</span>{/if}</div>
      <label class="field"><span>{$t("Ajouter une LoRA (ET)")}</span><select bind:value={loraChoice} onchange={() => { if (loraChoice && !filters.loras.includes(loraChoice)) filters.loras = [...filters.loras, loraChoice]; filters.lora = ''; loraChoice = ''; load(true); }}><option value="">{$t("Choisir…")}</option>{#each facets.lora as value}<option value={value}>{value}</option>{/each}</select></label>
      <label class="check"><input data-lora-exact type="checkbox" bind:checked={filters.loraExact} onchange={() => load(true)} />{$t("Groupe exact de LoRA indexées")}</label>
      {#if filters.loraExact}<p class="hint">{$t("Uniquement les noms sélectionnés, sans autre LoRA indexée. Sans sélection : aucune LoRA indexée. Les métadonnées absentes ou les nodes non reconnus ne prouvent pas l’absence réelle de LoRA ; ce filtre ne compare pas les poids.")}</p>{/if}
    </div></details>
    <details class="sec" open><summary><span class="sec-title">{$t("Validation")}</span>{#if filters.review||filters.favorite||filters.duplicates}<span class="chip">{(filters.review?1:0)+(filters.favorite?1:0)+(filters.duplicates?1:0)}</span>{/if}</summary><div class="sec-body">
      <label class="check"><input type="checkbox" bind:checked={filters.favorite} onchange={() => load(true)} />{$t("Favoris")}</label>
      <select aria-label={$t("Validation")} bind:value={filters.review} onchange={() => load(true)}><option value="">{$t("Toutes")}</option><option value="unreviewed">{$t("À examiner")}</option><option value="accepted">{$t("Validées")}</option><option value="rejected">{$t("Rejetées")}</option></select>
      <label class="check"><input type="checkbox" bind:checked={filters.duplicates} onchange={() => load(true)} />{$t("Doublons SHA-256")}</label>
    </div></details>
    <details class="sec" open={!!filters.availability}><summary><span class="sec-title">{$t("État des fichiers")}</span>{#if filters.availability}<span class="chip">1</span>{/if}</summary><div class="sec-body">
      <select aria-label={$t("État des fichiers")} data-availability-filter bind:value={filters.availability} onchange={()=>load(true)}><option value="">{$t("Présentes")}</option>{#each Object.entries(availabilityLabels).filter(([state])=>state!=='present') as [state,label]}<option value={state}>{$t(label)}</option>{/each}</select>
    </div></details>
    <details class="sec" open={filters.metadataOnly||!!filters.metadataState}><summary><span class="sec-title">{$t("Métadonnées")}</span>{#if filters.metadataOnly||filters.metadataState}<span class="chip">{(filters.metadataOnly?1:0)+(filters.metadataState?1:0)}</span>{/if}</summary><div class="sec-body">
      <label class="check"><input data-metadata-only type="checkbox" bind:checked={filters.metadataOnly} onchange={()=>load(true)}/>{$t("Avec métadonnées uniquement")}</label>
      <select aria-label={$t("Métadonnées")} bind:value={filters.metadataState} onchange={() => load(true)}><option value="">{$t("Toutes")}</option><option value="read">{$t("Présentes")}</option><option value="absent">{$t("Absentes")}</option><option value="invalid">{$t("Invalides")}</option></select>
    </div></details>
    <details class="sec" open={!!(filters.variant||filters.workflow||filters.job)}><summary title={$t("Associations issues des requêtes enregistrées, sans reconnaissance visuelle. Les variantes historiques restent accessibles après modification des fiches.")}><span class="sec-title">{$t("Provenance")}</span>{#if filters.variant||filters.workflow||filters.job}<span class="chip">{(filters.variant?1:0)+(filters.workflow?1:0)+(filters.job?1:0)}</span>{/if}</summary><div class="sec-body">
      <label class="field"><span>{$t("Variante historique")}</span><select bind:value={filters.variant} onchange={()=>load(true)}><option value="">{$t("Toutes")}</option>{#each facets.variant as v}<option value={v.id}>{v.name}</option>{/each}</select></label>
      <label class="field"><span>{$t("Workflow")}</span><select bind:value={filters.workflow} onchange={()=>load(true)}><option value="">{$t("Tous")}</option>{#each facets.workflow as w}<option value={w.id}>{w.name}</option>{/each}</select></label>
      <label class="field"><span>{$t("Identifiant de génération")}</span><input list="generation-options" bind:value={filters.job} oninput={search} placeholder={$t("Tous les jobs")} /></label><datalist id="generation-options">{#each facets.job as j}<option value={j.id}>{j.name}</option>{/each}</datalist>
    </div></details>
    <LibrarySources bind:root refreshKey={streamRefresh} busy={initializing||busy||progress.running} onindex={index} onchanged={async()=>{await lists();await load();streamRefresh++;}}/>
    {#if progress.lastRun}<details class="sec" data-index-run={progress.lastRun.id}><summary><span class="sec-title">{$t("Dernière indexation")}</span><span class="chip">{progress.lastRun.completed} / {progress.lastRun.total}{progress.lastRun.errorCount?` · ${progress.lastRun.errorCount} ⚠`:''}</span></summary><div class="sec-body run">
      <p class="muted" title={progress.lastRun.root}>{$t("Dernière indexation · {done} / {total} · {errors} erreurs",{done:progress.lastRun.completed,total:progress.lastRun.total,errors:progress.lastRun.errorCount})}</p>
      <p class="path">{progress.lastRun.root}</p><p class="muted num">{$t("{indexed} indexées · {unchanged} inchangées · {date}",{indexed:progress.lastRun.indexed,unchanged:progress.lastRun.unchanged,date:progress.lastRun.updatedAt})}</p>
      {#if ['discovering','indexing','interrupted','failed'].includes(progress.lastRun.state)&&!progress.running}<p class="muted">{$t("Exécution non terminée. La reprise vérifie le dossier et conserve les images déjà indexées.")}</p><button disabled={initializing||busy} onclick={resumeIndex}>{$t("Reprendre ce dossier")}</button>{:else if progress.lastRun.state==='completed'}<p class="muted">{progress.lastRun.errorCount?$t("Parcours terminé avec erreurs."):$t("Parcours terminé.")}</p>{/if}
      {#if progress.lastRun.lastPath}<p class="muted path">{$t("Dernier fichier traité : {path}",{path:progress.lastRun.lastPath})}</p>{/if}
      {#if progress.lastRun.errorCount}<button class="quiet" disabled={indexErrorsBusy} onclick={()=>showIndexErrors(true)}>{$t("Lire les erreurs enregistrées")}</button>{/if}
      {#if indexErrorRun===progress.lastRun.id}{#each indexErrors as item}<p class="error path">{item.path} : {item.message}</p>{/each}{#if indexErrorsMore}<button class="quiet" disabled={indexErrorsBusy} onclick={()=>showIndexErrors()}>{$t("Erreurs suivantes")}</button>{/if}{/if}
    </div></details>{/if}
    <div class="group"><button class="quiet reset" onclick={() => { filters = restoreFilters(null); load(); }}>{$t("Réinitialiser les filtres")}</button></div>
  </div>
</aside>
<main class="wb-center">
  <div class="wb-bar">
    <span class="count num" data-gallery-count title={$t("originaux en lecture seule")}><strong>{total.toLocaleString($locale)}</strong> {$tn("image","images",total)}</span>
    {#if chips.length}<div class="active-filters" data-active-filters>{#if filters.checkpoints.length}<span class="muted" data-checkpoint-chips>{$t("Checkpoints ({n})",{n:filters.checkpoints.length})}</span>{/if}{#each chips as chip (chip.key)}<button class="fchip" title={$t("Retirer ce filtre")} onclick={()=>{chip.clear();load(true);}}>{chip.label} <span aria-hidden="true">×</span></button>{/each}<button class="fchip clear" data-clear-filters onclick={clearAll}>{$t("Tout effacer")}</button></div>{/if}
    <span class="grow"></span>
    <select aria-label={$t("Trier par")} title={$t("Trier par")} data-gallery-sort bind:value={filters.sort} onchange={() => load(true)}>{#each gallerySorts as [id,label]}<option value={id}>{$t(label)}</option>{/each}</select>
    <input class="size" type="range" min="96" max="420" step="8" aria-label={$t("Taille des vignettes")} title={$t("Taille des vignettes")} bind:value={thumbSize} onchange={saveThumbSize} />
    <label class="toggle" class:on={selectionMode} title={$t("Sélectionner plusieurs images")}><input data-selection-mode type="checkbox" bind:checked={selectionMode}/>{$t("Sélection multiple")}</label>
    <label class="toggle" class:on={streaming} title={$t("Défilement continu")}><input data-stream-toggle type="checkbox" bind:checked={streaming} onchange={() => { if (!streaming) load(); if (native) call('set_preference', { key: 'galleryView', value: streaming ? 'stream' : 'pages' }).catch(e => error = message(e)); }} />{$t("Défilement continu")}</label>
    <button class="index" data-gallery-index disabled={!native || initializing || !root.trim() || busy || progress.running} onclick={()=>index()} title={root}><Icon name="redo" size={14}/>{busy || progress.running ? $t("Indexation en cours…") : $t("Indexer / actualiser")}</button>
  </div>
  {#if busy || progress.running}<div class="progress-row"><span role="status" class="num">{$t("Indexation : {done} / {total}",{done:progress.completed,total:progress.total || $t("découverte des fichiers…")})}</span>{#if progress.total&&progress.completed>=progress.total}<span class="muted">{$t("Finalisation du catalogue…")}</span><progress></progress>{:else}<progress max={progress.total || 1} value={progress.completed}></progress>{/if}<button class="quiet" disabled={!progress.running||progress.cancel_requested} onclick={()=>call('cancel_index').catch(e=>error=message(e))}>{progress.cancel_requested?$t("Arrêt après le fichier courant…"):$t("Interrompre proprement")}</button></div>{/if}
  <AssetActions assets={selection} onchanged={assetsChanged} onclear={()=>selection=[]}/>
  {#if comparison.length}<div class="compare-row" data-comparison-selection><span class="muted">{$t("Comparaison {n}/2",{n:comparison.length})}</span>{#each comparison as asset}<button class="fchip" onclick={()=>comparison=comparison.filter(a=>a.id!==asset.id)}>{$t("Retirer {name}",{name:fileName(asset.path)})}</button>{/each}<span class="grow"></span><button class="primary" disabled={comparison.length!==2} onclick={()=>comparing=true}>{$t("Comparer les deux images")}</button></div>{/if}
  {#if error && !viewerOpen}<p class="msg error" role="alert">{error}</p>{/if}{#if note}<p class="msg notice" role="status">{note}</p>{/if}
  <div class="grid-area">
  {#if streaming}
  <StreamGrid {filters} onopen={galleryClick} selectedIds={selection.map(a=>a.id)} {selectionMode} refresh={streamRefresh} size={thumbSize} currentId={selected?.id ?? null} />
  {:else}
  <div class="pages">
  <div class="image-grid" style="--thumb:{thumbSize}px" aria-busy={loading}>{#each images as asset (asset.id)}<button class="image-card" class:selected={selection.some(a=>a.id===asset.id)} class:current={selected?.id===asset.id} aria-pressed={selectionMode?selection.some(a=>a.id===asset.id):undefined} onclick={() => galleryClick(asset)} title={asset.path}><Thumbnail id={asset.id} alt={asset.path} /><div class="caption"><span>{asset.favorite ? '★ ' : ''}{asset.path.split(/[\\/]/).pop()?.replace(/\.(png|jpe?g|webp)$/i,'')}</span><small class="num">{asset.width ?? '?'} × {asset.height ?? '?'}{asset.review!=='unreviewed'?` · ${$t(reviewLabels[asset.review]??asset.review)}`:''}{asset.metadataState === 'absent' ? ' · '+$t("sans métadonnées") : ''}</small></div></button>{/each}</div>
  {#if !images.length}<div class="empty-state">{loading ? $t("Chargement…") : $t("Aucune image dans cette sélection. Indexez le dossier ou ajustez les filtres.")}</div>{/if}
  </div>
  <div class="pagination"><button class="quiet" disabled={filters.offset === 0 || loading} onclick={() => { filters.offset = Math.max(0, filters.offset - 48); load(); }}>{$t("← Précédentes")}</button><span class="num muted">{total ? filters.offset + 1 : 0}–{Math.min(filters.offset + 48, total)} / {total}</span><button class="quiet" disabled={filters.offset + 48 >= total || loading} onclick={() => { filters.offset += 48; load(); }}>{$t("Suivantes →")}</button></div>
  {/if}
  </div>
</main>
<aside class="wb-panel right inspector" data-inspector data-inspector-path={selected?.path ?? ''} aria-label={$t("Inspecteur")}>
  {#if selected}
  <div class="wb-head"><h2 title={selected.path}>{fileName(selected.path)}</h2><button class="icon-btn quiet" onclick={clearInspector} title={$t("Fermer l’inspecteur")} aria-label={$t("Fermer l’inspecteur")}><Icon name="close"/></button></div>
  <div class="iconrow" data-file-actions>
    <button class="icon-btn quiet" data-open-manager disabled={!selected} onclick={openInManager} title={$t("« Ouvrir dans le gestionnaire » montre le dossier conteneur (xdg-open) ; c’est ainsi que se retrouvent les références à réutiliser.")}><Icon name="folder"/><span class="vh">{$t("Ouvrir dans le gestionnaire")}</span></button>
    <button class="icon-btn quiet" class:done={pathCopied} data-copy-path disabled={!detail} onclick={copyPath} title={pathCopied?$t("Chemin copié ✓"):$t("Copier le chemin")}><Icon name={pathCopied?'check':'copy'}/><span class="vh">{pathCopied?$t("Chemin copié ✓"):$t("Copier le chemin")}</span></button>
    <button class="icon-btn quiet" disabled={comparison.length>=2||comparison.some(a=>a.id===selected?.id)} onclick={addComparison} title={$t("Deux images maximum ; retirez une sélection pour la remplacer.")}><Icon name="compare"/><span class="vh">{$t("Ajouter à la comparaison")}</span></button>
    <button class="icon-btn quiet" data-find-similar onclick={() => { if (selected) similar = { ...selected }; }} title={$t("Similarité visuelle approximative, calculée localement ; jamais une identification de personnage.")}><Icon name="casting"/><span class="vh">{$t("Trouver des images similaires")}</span></button>
    <button class="icon-btn quiet" onclick={() => { const asset = selected; if (asset) { selected = null; onedit(asset); } }} title={$t("Ouvre l’éditeur non destructif ; l’original reste en lecture seule.")}><Icon name="edit"/><span class="vh">{$t("Retoucher (flou localisé)")}</span></button>
    <button class="icon-btn quiet" disabled={!selected} onclick={refreshPresence} title={$t("Actualiser l’état du fichier")}><Icon name="redo"/><span class="vh">{$t("Actualiser l’état du fichier")}</span></button>
    <span class="grow"></span>
    <AssetActions variant="trash" assets={[selected]} onchanged={async()=>{await assetsChanged();if(selected)await open(selected);}} onclear={()=>{}}/>
  </div>
  <div class="wb-scroll">
    <div class="inspector-image"><button class="preview-btn" data-inspector-open onclick={showViewer} title={$t("Ouvrir en plein écran")}><Thumbnail id={selected.id} preview alt={selected.path} /></button></div>
    <div class="details">
      {#if selected.captureRole || selected.capturePhase}<p data-capture-badge class="badge"><strong>{$t(captureLabel(selected.captureRole, selected.capturePhase, null))}</strong>{selected.captureDay?` · ${$t("journée {day}",{day:selected.captureDay})}`:''}{selected.captureFamille&&selected.captureRole==='pull'?` · ${selected.captureFamille==='sans-famille'?$t("Sans famille"):$t("famille {name}",{name:selected.captureFamille})}`:''}</p>{/if}
      <dl class="kv">
        <dt>{$t("Taille")}</dt><dd class="num">{selected.width ?? '?'} × {selected.height ?? '?'}</dd>
        {#if selectedCharacters.length}<dt>{$t("Personnage")}</dt><dd>{#each selectedCharacters as c}<span class="assoc"><button class="link" onclick={()=>oncharacter(c.id)}>{c.name}</button><button class="icon-btn quiet tiny" onclick={() => associate(c.id, false)} title={$t("Retirer le lien")} aria-label={$t("Retirer le lien")}><Icon name="close" size={12}/></button></span>{/each}</dd>{/if}
        <dt>{$t("Fichier")}</dt><dd>{#if detail}<span data-asset-path class="path">{(detail as DetailPayload).path}</span>{#if (detail as DetailPayload).reason}<span class="error">{(detail as DetailPayload).reason}</span>{/if}{:else}<span class="muted">{$t("Chargement du chemin…")}</span>{/if}</dd>
        <dt>{$t("SHA-256")}</dt><dd class="path muted" title={$t("Les données brutes restent dans le fichier et la base.")}>{selected.contentSha256??'—'}</dd>
      </dl>
      <GenerationMetadata id={selected.id}/>
    </div>
    <details class="sec" open><summary><span class="sec-title">{$t("Annotations")}</span>{#if selected.favorite}<span class="chip">★</span>{/if}</summary><div class="sec-body">
      <label class="check"><input data-annotation-favorite type="checkbox" bind:checked={selected.favorite} />{$t("Favori")}</label>
      <select aria-label={$t("Validation")} bind:value={selected.review}><option value="unreviewed">{$t("À examiner")}</option><option value="accepted">{$t("Validée")}</option><option value="rejected">{$t("Rejetée")}</option></select>
      <textarea data-annotation-note aria-label={$t("Notes")} placeholder={$t("Notes")} rows="3" maxlength="20000" bind:value={selected.note}></textarea>
      <button disabled={annotationBusy} onclick={annotate}>{annotationBusy ? $t("Enregistrement…") : $t("Enregistrer les annotations")}</button>{#if annotationNote && annotationNote.id === selected.id}<p class="muted" role="status">{annotationNote.text}</p>{/if}
    </div></details>
    <details class="sec"><summary><span class="sec-title">{$t("Personnages associés")}</span><span class="chip">{selectedCharacters.length}</span></summary><div class="sec-body">
      <select aria-label={$t("Associer une fiche")} bind:value={associateId}><option value="">{$t("Associer une fiche")}…</option>{#each characters as c}<option value={c.id}>{c.name}{c.archived?' · '+$t("archivé"):''}</option>{/each}</select>
      <button disabled={!associateId} onclick={() => associate(associateId, true)}>{$t("Associer cette image")}</button>
    </div></details>
  </div>
  {:else}
  <div class="wb-head"><h2 class="muted">{$t("Inspecteur")}</h2></div>
  <p class="empty">{$t("Sélectionnez une image.")}</p>
  {/if}
</aside>
</div>
{#if comparing&&comparison.length===2}<ImageComparison assets={comparison} onclose={()=>comparing=false} />{/if}
{#if similar}<VisualSearch asset={similar} onclose={()=>similar=null} />{/if}
{#if selected && viewerOpen}<dialog class="image-dialog viewer" use:dialog onclose={closeViewer} aria-label={$t("Image et métadonnées")}>
  <div class="dialog-toolbar"><strong title={selected.path}>{selected.path}</strong><span class="num muted" data-viewer-position>{neighbors?.total?`${neighbors.position+1} / ${neighbors.total}`:$t("Aucun résultat")}{navBusy?' · '+$t("Chargement…"):''}</span>{#if error}<span class="error" role="alert">{error}</span>{/if}<span class="grow"></span><button class="quiet" data-viewer-close onclick={closeViewer}>{$t("Fermer ×")}</button></div>
  <div class="stage" data-viewer-navigation>
    <button class="edge prev" data-viewer-previous disabled={navBusy||!neighbors?.previous} onclick={()=>navigate('previous')} title={$t("Image précédente")}><Icon name="left" size={22}/><span class="vh">{$t("← Précédente")}</span></button>
    <div class="preview-pane"><Thumbnail id={selected.id} preview alt={selected.path} /></div>
    <button class="edge next" data-viewer-next disabled={navBusy||!neighbors?.next} onclick={()=>navigate('next')} title={$t("Image suivante")}><Icon name="right" size={22}/><span class="vh">{$t("Suivante →")}</span></button>
  </div>
</dialog>{/if}
<style>
  .gallery{--wb-left:240px;--wb-right:320px;height:100%}
  .vh{position:absolute!important;width:1px;height:1px;margin:-1px;padding:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;border:0}
  p{margin:0}
  details{margin:0}
  label{display:flex;flex-direction:row;font-size:inherit;color:inherit}
  .grow{flex:1}
  .search{flex:1;min-width:0}
  .group{display:grid;gap:6px;padding:10px 12px;border-bottom:1px solid var(--line)}
  .group .sec-title{flex:none}
  .field{display:grid;gap:3px}
  .field>span{font-size:11.5px;color:var(--muted)}
  .sec-body select,.sec-body input:not([type=checkbox]),.sec-body textarea,.field select,.field input{width:100%}
  .check{display:flex;align-items:center;gap:8px;min-height:22px;cursor:pointer;overflow-wrap:anywhere;line-height:1.3}
  .check input{margin:0;flex:none;accent-color:var(--sel)}
  .list{display:grid;gap:2px;max-height:180px;overflow:auto}
  .hint{font-size:11.5px;color:var(--muted)}
  .reset{width:100%}
  .run .path,.path{overflow-wrap:anywhere;font-size:11.5px}
  /* center */
  .wb-bar{flex-wrap:nowrap;gap:6px;min-width:0}
  .count{white-space:nowrap}
  .count strong{font-weight:600}
  .active-filters{display:flex;gap:4px;min-width:0;overflow-x:auto;scrollbar-width:none;margin:0;flex-wrap:nowrap;align-items:center}
  .fchip{min-height:22px;padding:0 7px;font-size:11.5px;border-radius:3px;background:var(--raised);border:1px solid var(--line-strong);color:var(--text);white-space:nowrap}
  .fchip span{color:var(--muted);margin-inline-start:2px}
  .fchip.clear{background:transparent;border-style:dashed;color:var(--muted)}
  .active-filters .muted{font-size:11.5px;white-space:nowrap}
  .wb-bar select{max-width:170px}
  .size{width:96px;min-height:0;padding:0;border:0;background:none;accent-color:var(--muted)}
  .toggle{display:inline-flex;align-items:center;gap:5px;min-height:26px;padding:0 8px;border-radius:var(--radius);color:var(--muted);font-size:12px;white-space:nowrap;cursor:pointer}
  .toggle:hover{background:var(--raised);color:var(--text)}
  .toggle.on{color:var(--text);background:var(--raised)}
  .toggle input{margin:0;accent-color:var(--accent)}
  .index{display:inline-flex;align-items:center;gap:6px;white-space:nowrap}
  .progress-row,.compare-row{display:flex;align-items:center;gap:8px;min-height:32px;padding:0 10px;border-bottom:1px solid var(--line);background:var(--bg);font-size:12px}
  .progress-row progress{flex:1;max-width:360px;height:3px;accent-color:var(--accent)}
  .progress-row button,.compare-row button{min-height:24px;font-size:12px}
  .msg{padding:6px 10px;border-bottom:1px solid var(--line);background:var(--bg);font-size:12px;white-space:pre-wrap;border-inline:0;border-top:0}
  .grid-area{flex:1;min-height:0;display:flex;flex-direction:column;position:relative}
  .pages{flex:1;min-height:0;overflow:auto;padding:12px}
  .image-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(var(--thumb,184px),1fr));gap:8px;margin:0}
  .image-card{display:flex;flex-direction:column;padding:0;min-height:0;border:0;background:transparent;text-align:start;overflow:visible;border-radius:var(--r-img)}
  .image-card:hover:not(:disabled){background:transparent}
  .image-card :global(img),.image-card :global(.image-placeholder){width:100%;aspect-ratio:4/5;height:auto;max-height:none;object-fit:contain;background:var(--canvas);border-radius:var(--r-img);display:block}
  .image-card.current :global(img){outline:1px solid var(--sel);outline-offset:2px}
  .image-card.selected{outline:0}
  .image-card.selected :global(img){outline:2px solid var(--accent);outline-offset:2px}
  .caption{display:flex;align-items:baseline;gap:6px;padding:4px 2px 0;min-width:0;color:var(--muted);font-size:11px;line-height:16px}
  .caption span{flex:1;min-width:0;padding:0;font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .caption small{display:none;flex:0 1 auto;max-width:60%;min-width:0;padding:0;font-size:10.5px;color:var(--faint);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .image-card:hover .caption{color:var(--text)}.image-card:hover .caption small,.image-card:focus-visible .caption small{display:block}
  .empty-state{padding:80px 20px;text-align:center;color:var(--muted)}
  .pagination{display:flex;align-items:center;justify-content:center;gap:12px;min-height:36px;margin:0;border-top:1px solid var(--line);background:var(--bg);font-size:12px}
  /* inspector */
  .inspector h2{font-variant-numeric:tabular-nums}
  .iconrow{display:flex;align-items:center;gap:2px;flex-wrap:wrap;padding:8px 10px;border-bottom:1px solid var(--line);flex:none}
  .iconrow .icon-btn.done{color:var(--ok)}
  .preview-btn{display:block;width:100%;padding:0;min-height:0;border:0;background:transparent;cursor:zoom-in}
  .preview-btn:hover:not(:disabled){background:transparent}
  .preview-btn :global(img){width:100%;max-height:60vh;object-fit:contain;border-radius:var(--r-img);display:block;background:var(--canvas)}
  .preview-btn :global(.image-placeholder){min-height:240px;font-size:11px;color:var(--faint)}
  .details{display:grid;gap:12px;padding:12px;border-bottom:1px solid var(--line)}
  .badge{font-size:12px;color:var(--muted)}.badge strong{color:var(--text);font-weight:600}
  .kv{font-size:12px}
  .kv dd .error{display:block}
  .assoc{display:inline-flex;align-items:center;gap:2px;margin-inline-end:6px}
  .link{min-height:0;padding:0;border:0;background:none;color:var(--text);text-decoration:underline;text-decoration-color:var(--line-strong);text-underline-offset:2px}
  .link:hover:not(:disabled){background:none;text-decoration-color:var(--text)}
  .icon-btn.tiny{width:18px;min-height:18px}
  .empty{padding:24px 12px;color:var(--muted);text-align:center}
  /* viewer */
  .viewer{width:100vw;height:100vh;max-width:none;max-height:none;margin:0;padding:0;border:0;border-radius:0;background:var(--canvas);color:var(--text)}
  .viewer[open]{display:flex;flex-direction:column}
  .viewer::backdrop{background:#000}
  .viewer .dialog-toolbar{display:flex;align-items:center;gap:12px;height:40px;padding:0 10px 0 14px;border-bottom:1px solid var(--line);background:var(--bg);flex:none}
  .viewer .dialog-toolbar strong{min-width:0;font-weight:500;font-size:12.5px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .viewer .dialog-toolbar .num{white-space:nowrap;font-size:12px}
  .viewer .dialog-toolbar .error{font-size:12px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}
  .stage{position:relative;flex:1;min-height:0;display:flex;margin:0;padding:0;border:0}
  .preview-pane{flex:1;min-width:0;display:flex;align-items:center;justify-content:center;padding:16px 64px;background:var(--canvas)}
  .preview-pane :global(img){max-width:100%;max-height:calc(100vh - 72px);width:auto;height:auto;object-fit:contain;border-radius:var(--r-img)}
  .edge{position:absolute;top:50%;transform:translateY(-50%);z-index:1;width:44px;height:64px;padding:0;display:grid;place-items:center;border:0;border-radius:6px;background:transparent;color:var(--muted)}
  .edge:hover:not(:disabled){background:var(--raised);color:var(--text)}
  .edge:disabled{opacity:.2}
  .edge.prev{inset-inline-start:10px}.edge.next{inset-inline-end:10px}
  /* Primary maintenance action of the gallery: always readable, never a faint ghost link. */
  .index{display:inline-flex;align-items:center;gap:6px;color:var(--text);border-color:var(--line);background:var(--raised);font-weight:500}
  .index:hover:not(:disabled){border-color:var(--accent)}
</style>
