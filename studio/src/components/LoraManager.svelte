<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { call, message } from '../lib/api';
  import ModelThumb, { modelThumb, civitaiThumb } from './ModelThumb.svelte';
  import Icon from './Icon.svelte';

  // Library over the engine's own model folders, read from LoraManager-compatible sidecars.
  interface Model { path:string; root:string; folder:string; file:string; stem:string; name:string; ext:string; bytes:number; modified:number; base:string; tags:string[]; triggers:string[]; versionName:string; modelId:number|null; versionId:number|null; favorite:boolean; sha256:string; nsfw:number; fromCivitai:boolean; hasMetadata:boolean; preview:string|null }
  type Kind = 'loras'|'checkpoints'|'embeddings'|'others';
  const kinds: [Kind,string][] = [['loras','LoRA'],['checkpoints','Checkpoints'],['embeddings','Embeddings'],['others','Autres']];
  const typeBadge: Record<Kind,string> = { loras:'LoRA', checkpoints:'CKPT', embeddings:'EMB', others:'MODÈLE' };
  const sorts: [string,string][] = [['name','Nom A → Z'],['name_desc','Nom Z → A'],['newest','Plus récents'],['oldest','Plus anciens'],['size','Plus lourds'],['base','Modèle de base']];
  const baseShort = (b:string) => ({Illustrious:'IL',NoobAI:'NAI',Pony:'PONY','SDXL 1.0':'XL','SD 1.5':'1.5',HiDream:'HID',Anima:'ANI','Flux.1 D':'FLUX','Flux.1 S':'FLUX'} as Record<string,string>)[b] ?? (b.startsWith('Wan') ? 'WAN' : b.slice(0,5).toUpperCase());
  const fold = (v:string) => v.normalize('NFD').replace(/\p{M}/gu,'').toLocaleLowerCase('fr');
  const fmtBytes = (n:number) => { const u=['o','Ko','Mo','Go']; let i=0,v=n; while(v>=1024&&i<u.length-1){v/=1024;i++} return `${i===0||v>=10?Math.round(v):v.toFixed(1)} ${u[i]}`; };

  let tab = $state<Kind|'civitai'|'stats'>('loras'), libs = $state<Partial<Record<Kind,{items:Model[];roots:{key:string;path:string}[]}>>>({}), loading = $state<Kind|null>(null);
  let folder = $state(''), open = $state<Record<string,boolean>>({}), search = $state(''), sort = $state('name'), base = $state(''), favoritesOnly = $state(false), duplicatesOnly = $state(false), missingOnly = $state(false);
  let legacyFavorites = $state<string[]>([]), error = $state(''), note = $state(''), busy = $state(false), detail = $state<Model|null>(null), detailMeta = $state<any>(null), metaText = $state(''), metaEditing = $state(false);
  let stacks = $state<{name:string;selections:any[]}[]>([]), stackName = $state(''), selections = $state<any[]>([]), slots = $state<{session:string;slots:{id:string;title:string}[]}|null>(null), slot = $state('');
  let fetching = $state<{done:number;total:number;found:number;stop:boolean}|null>(null), plan = $state<{moves:any[];skipped:number}|null>(null), chosen = $state<Record<string,boolean>>({}), journal = $state(''), organizeAll = $state(false);
  // By default only loose files (library root) are filed; personal folders stay unless asked.
  const planMoves = $derived((plan?.moves ?? []).filter(m => organizeAll || !m.fromFolder));
  let root = $state('');

  const kind = $derived<Kind>(tab === 'civitai' || tab === 'stats' ? 'loras' : tab);
  const items = $derived(libs[kind]?.items ?? []);
  const relPath = (m:Model) => m.folder && !kinds.some(([k])=>k===m.folder) ? `${m.folder}/${m.file}` : m.file;
  const isFav = (m:Model) => m.favorite || legacyFavorites.includes(relPath(m));
  const dupes = $derived.by(() => { const c = new Map<string,number>(); for (const m of items) if (m.sha256) c.set(m.sha256,(c.get(m.sha256)??0)+1); return c; });
  const bases = $derived([...new Set(items.map(m=>m.base).filter(Boolean))].sort((a,b)=>a.localeCompare(b,'fr')));
  // Folder tree: every ancestor of every model folder, with recursive counts.
  const tree = $derived.by(() => { const counts = new Map<string,number>(); for (const m of items) { const parts = m.folder ? m.folder.split('/') : []; for (let i=1;i<=parts.length;i++) { const p = parts.slice(0,i).join('/'); counts.set(p,(counts.get(p)??0)+1); } } return counts; });
  const children = (parent:string) => [...tree.keys()].filter(p => parent ? p.startsWith(parent+'/') && !p.slice(parent.length+1).includes('/') : !p.includes('/')).sort((a,b)=>a.localeCompare(b,'fr',{sensitivity:'base'}));
  const shown = $derived.by(() => { const q = fold(search.trim());
    return items.filter(m => (!folder || m.folder===folder || m.folder.startsWith(folder+'/')) && (!base || m.base===base) && (!favoritesOnly || isFav(m)) && (!duplicatesOnly || (dupes.get(m.sha256)??0)>1) && (!missingOnly || !m.hasMetadata)
      && (!q || fold(`${m.name} ${m.file} ${m.folder} ${m.tags.join(' ')} ${m.triggers.join(' ')} ${m.base}`).includes(q)))
      .sort((a,b) => sort==='name_desc' ? b.name.localeCompare(a.name,'fr',{sensitivity:'base'}) : sort==='newest' ? b.modified-a.modified : sort==='oldest' ? a.modified-b.modified : sort==='size' ? b.bytes-a.bytes
        : sort==='base' ? a.base.localeCompare(b.base,'fr')||a.name.localeCompare(b.name,'fr',{sensitivity:'base'}) : a.name.localeCompare(b.name,'fr',{sensitivity:'base'})); });
  let limit = $state(120);
  $effect(() => { void folder; void search; void sort; void base; void favoritesOnly; void duplicatesOnly; void missingOnly; void tab; limit = 120; });

  async function load(k:Kind, force=false) {
    if (libs[k] && !force) return; loading = k; error = '';
    try { const r = await call<{items:Model[];roots:{key:string;path:string}[]}>('model_library',{kind:k}); libs = {...libs,[k]:r}; }
    catch (e) { error = message(e); } finally { loading = null; }
  }
  async function save(k:string,v:unknown){try{await call('set_preference',{key:k,value:v})}catch(e){error=message(e)}}
  async function toggleFavorite(m:Model) {
    const next = !isFav(m);
    try { await call('model_favorite',{kind,path:m.path,favorite:next}); m.favorite = next;
      const rel = relPath(m); if (kind==='loras') { legacyFavorites = next ? [...new Set([...legacyFavorites,rel])] : legacyFavorites.filter(v=>v!==rel); await save('loraFavorites',legacyFavorites); }
      libs = {...libs}; } catch (e) { error = message(e); }
  }
  function add(m:Model){ const rel = relPath(m); if (selections.some(s=>s.relativePath===rel)) return; selections = [...selections,{name:m.name,relativePath:rel,modelWeight:1,clipWeight:1,enabled:true}]; }
  async function copy(text:string,what:string){ try { await navigator.clipboard.writeText(text); note = what; } catch (e) { error = $t("Copie impossible : {detail}",{detail:message(e)}); } }
  const civitaiUrl = (m:Model) => m.modelId ? `https://civitai.com/models/${m.modelId}${m.versionId?`?modelVersionId=${m.versionId}`:''}` : '';
  const plainText = (html:string) => new DOMParser().parseFromString(html||'','text/html').body.textContent?.trim() ?? '';
  async function openDetail(m:Model){ detail = m; detailMeta = null; metaEditing = false; try { detailMeta = (await call<any>('model_detail',{kind,path:m.path})).metadata; metaText = JSON.stringify(detailMeta??{},null,2); } catch (e) { error = message(e); } }
  async function fetchOne(m:Model){ if (busy) return; busy = true; error = ''; try { const r = await call<any>('model_fetch_metadata',{kind,path:m.path}); note = r.found ? $t("Métadonnées Civitai récupérées : {name}.",{name:r.name ?? m.name}) : $t("Ce fichier est inconnu de Civitai (empreinte introuvable)."); await load(kind,true); const again = libs[kind]?.items.find(x=>x.path===m.path); if (again && detail) await openDetail(again); } catch (e) { error = message(e); } finally { busy = false; } }
  async function fetchMissing(){
    const todo = items.filter(m=>!m.hasMetadata); if (!todo.length) { note = $t("Tous les modèles ont déjà leurs métadonnées."); return; }
    fetching = {done:0,total:todo.length,found:0,stop:false};
    for (const m of todo) { if (fetching.stop) break; try { const r = await call<any>('model_fetch_metadata',{kind,path:m.path}); if (r.found) fetching.found++; } catch (e) { error = message(e); } fetching.done++; }
    note = $t("Récupération terminée : {found} trouvé(s) sur Civitai, {unknown} inconnu(s).",{found:fetching.found,unknown:fetching.done-fetching.found}); fetching = null; await load(kind,true);
  }
  async function saveMeta(){ if (!detail) return; try { await call('lora_metadata_save',{path:detail.path,metadata:JSON.parse(metaText)}); note = $t("Métadonnées enregistrées."); metaEditing = false; await load(kind,true); } catch (e) { error = message(e); } }
  async function planOrganize(){ if (busy) return; busy = true; error = ''; try { plan = await call('model_organize_plan',{kind}); chosen = Object.fromEntries(plan!.moves.map(m=>[m.from,!m.conflict])); } catch (e) { error = message(e); } finally { busy = false; } }
  async function applyOrganize(){ if (!plan) return; const moves = planMoves.filter(m=>chosen[m.from]).map(m=>[m.from,m.to]); if (!moves.length) return; if (busy) return; busy = true;
    try { const r = await call<any>('model_organize_apply',{kind,moves}); journal = r.journal; note = $t("{n} fichier(s) déplacé(s)",{n:r.moved})+(r.errors.length?' · '+$t("{n} refus",{n:r.errors.length}):'')+'. '+$t("Annulation possible ci-dessous."); plan = null; await load(kind,true); } catch (e) { error = message(e); } finally { busy = false; } }
  async function undoOrganize(){ if (busy) return; busy = true; try { const r = await call<any>('model_organize_undo',{journal}); note = $t("{n} fichier(s) remis à leur place.",{n:r.restored}); journal = ''; await load(kind,true); } catch (e) { error = message(e); } finally { busy = false; } }

  // Stack and hand-off to Generation (unchanged behaviour).
  async function saveStack(){const name=stackName.trim()||`Stack ${stacks.length+1}`;stacks=[...stacks.filter(s=>s.name!==name),{name,selections:JSON.parse(JSON.stringify(selections))}];stackName='';await save('loraStacks',stacks)}
  async function sendToGeneration(){if(busy)return;busy=true;try{await save('loraActiveStack',JSON.parse(JSON.stringify(selections)));note=$t("Stack envoyé à Créer. Ouvre Créer → LoRA du workflow → Ajouter le stack du Manager.")}finally{busy=false}}
  // Stages the model in the stack, then sends the whole stack (it replaces loraActiveStack).
  async function sendLoraToCreate(m:Model){if(busy)return;add(m);await sendToGeneration()}
  // Queues one model for a workflow slot through the same inbox as the stack's slot button.
  async function sendLoraToSlot(m:Model,slotId:string){if(busy||!slots)return;busy=true;error='';
    try{const p=await call<Record<string,unknown>>('preferences');const inbox=Array.isArray(p.generationLoraInbox)?p.generationLoraInbox as any[]:[];
      await call('set_preference',{key:'generationLoraInbox',value:[...inbox,{session:slots.session,slot:slotId,name:relPath(m).replace(/\.(safetensors|ckpt|pt|bin)$/i,''),modelWeight:1,clipWeight:1}]});
      note=$t("{n} LoRA envoyée(s) vers {slot}. Elles apparaîtront à l’ouverture de Génération.",{n:1,slot:slots.slots.find(x=>x.id===slotId)?.title??''})}catch(e){error=message(e)}finally{busy=false}}
  // Context menu (right click) on a LoRA, card or detail panel.
  let menu = $state<{m:Model;x:number;y:number}|null>(null), menuEl = $state<HTMLElement|null>(null);
  function openMenu(e:MouseEvent,m:Model){ if(kind!=='loras') return; e.preventDefault(); e.stopPropagation(); menu={m,x:e.clientX,y:e.clientY}; }
  const closeMenu = () => { menu = null; };
  $effect(() => { if(!menu||!menuEl) return; const r=menuEl.getBoundingClientRect(); const x=Math.max(4,Math.min(menu.x,window.innerWidth-r.width-4)), y=Math.max(4,Math.min(menu.y,window.innerHeight-r.height-4)); menuEl.style.left=`${x}px`; menuEl.style.top=`${y}px`; menuEl.querySelector<HTMLElement>('button:not(:disabled)')?.focus(); });
  function menuKey(e:KeyboardEvent){ if(!menu) return; if(e.key==='Escape'){ e.preventDefault(); closeMenu(); return; }
    if(e.key==='ArrowDown'||e.key==='ArrowUp'||e.key==='Home'||e.key==='End'){ const bs=[...menuEl?.querySelectorAll<HTMLElement>('button:not(:disabled)')??[]]; if(!bs.length) return; e.preventDefault(); const i=bs.indexOf(document.activeElement as HTMLElement); bs[e.key==='Home'?0:e.key==='End'?bs.length-1:(i+(e.key==='ArrowDown'?1:-1)+bs.length)%bs.length].focus(); } }
  function menuOutside(e:Event){ if(menu&&menuEl&&!menuEl.contains(e.target as Node)) closeMenu(); }
  async function menuRun(fn:()=>unknown){ closeMenu(); await fn(); }
  function loadStack(name:string){const s=stacks.find(x=>x.name===name);if(s)selections=JSON.parse(JSON.stringify(s.selections))}
  async function sendToSlot(){if(busy||!slots||!slot)return;const picks=selections.filter(x=>x.enabled!==false);if(!picks.length)return;
    try{const p=await call<Record<string,unknown>>('preferences');const inbox=Array.isArray(p.generationLoraInbox)?p.generationLoraInbox as any[]:[];
      await call('set_preference',{key:'generationLoraInbox',value:[...inbox,...picks.map(x=>({session:slots!.session,slot,name:x.relativePath.replace(/\.(safetensors|ckpt|pt|bin)$/i,''),modelWeight:x.modelWeight,clipWeight:x.clipWeight}))]});
      note=$t("{n} LoRA envoyée(s) vers {slot}. Elles apparaîtront à l’ouverture de Génération.",{n:picks.length,slot:slots.slots.find(x=>x.id===slot)?.title??''})}catch(e){error=message(e)}}
  async function pickRoot(){const p=await invoke<string[]>('pick_directories');if(p?.length){root=p[0];await save('loraRoot',root);await load('loras',true)}}

  // Civitai: search, previews through the app (CSP), download via the bridge, then file and fetch metadata.
  let remoteQuery=$state(''),remoteType=$state('LORA'),remoteBase=$state(''),remoteResults=$state<any[]>([]),remoteBusy=$state(false),remoteError=$state(''),versionOf=$state<Record<number,number>>({});
  let jobState=$state<Record<string,any>>({}),jobsError=$state(''),jobsSyncError=$state(''),jobBusy=$state<Record<string,boolean>>({}),downloadsOpen=$state(false);
  const typeKind = (t:string):Kind|null => ['LORA','LoCon','DoRA','LYCORIS'].includes(t) ? 'loras' : t==='Checkpoint' ? 'checkpoints' : t==='TextualInversion' ? 'embeddings' : null;
  const JOBS_KEY='saya.civitai.imports';
  function jobMap():Record<string,{kind:Kind|null;imported?:boolean}>{try{return JSON.parse(localStorage.getItem(JOBS_KEY)||'{}')}catch{return {}}}
  function setJobMap(v:Record<string,any>){try{localStorage.setItem(JOBS_KEY,JSON.stringify(v))}catch{}}
  async function remoteSearch(){remoteBusy=true;remoteError='';try{const r=await call<{items:any[]}>('civitai_search',{query:remoteQuery,types:remoteType,base_models:remoteBase,limit:24});remoteResults=r.items||[];versionOf={};if(!remoteResults.length)remoteError=$t("Aucun résultat.")}catch(e){remoteError=message(e);remoteResults=[]}finally{remoteBusy=false}}
  async function remoteDownload(model:any,version:any){
    const file=version.files?.find((f:any)=>f.primary)??version.files?.[0];remoteBusy=true;
    try{const r=await call<any>('civitai_download',{version_id:version.id,file_id:file?.id??null,destination:''});const map=jobMap();if(r?.job)map[r.job]={kind:typeKind(model.type)};setJobMap(map);note=$t("Téléchargement lancé : {name}. Il sera rangé automatiquement à la fin.",{name:model.name});downloadsOpen=true;await pollJobs()}catch(e){remoteError=message(e)}finally{remoteBusy=false}}
  async function pollJobs(){
    try{const r=await call<{jobs:Record<string,any>}>('civitai_jobs');jobState=r.jobs||{};jobsSyncError='';
      const map=jobMap();
      for(const [id,j] of Object.entries(jobState)){const entry=map[id];if(j.state!=='done'||!j.path||!entry?.kind||entry.imported)continue;
        entry.imported=true;setJobMap(map);
        try{const done=await call<any>('model_import_download',{kind:entry.kind,path:j.path});note=$t("Téléchargement rangé : {path}",{path:done.path})+(done.metadata?.found?' · '+$t("métadonnées Civitai récupérées"):'')+'.';await load(entry.kind,true)}catch(e){entry.imported=false;setJobMap(map);error=$t("Rangement du téléchargement impossible : {detail}",{detail:message(e)})}}
    }catch(e){jobsSyncError=$t("Jobs Civitai indisponibles : {detail}",{detail:message(e)})}}
  async function jobControl(verb:'cancel'|'pause'|'resume',id:string){if(jobBusy[id])return;jobBusy={...jobBusy,[id]:true};jobsError='';
    try{await call(verb==='cancel'?'civitai_cancel':verb==='pause'?'civitai_pause':'civitai_resume',{job:id})}catch(e){jobsError=$t("Action impossible : {detail}",{detail:message(e)})}
    finally{const next={...jobBusy};delete next[id];jobBusy=next;await pollJobs()}}
  const jobLabel=(s:string)=>({queued:'En attente',downloading:'Téléchargement',paused:'En pause',done:'Terminé',cancelled:'Annulé',error:'Erreur'} as Record<string,string>)[s]??'Inconnu';
  const percent=(j:any)=>typeof j.total==='number'&&j.total>0&&typeof j.bytes==='number'?Math.max(0,Math.min(100,Math.round(j.bytes/j.total*100))):null;
  const activeJobs=$derived(Object.values(jobState).filter((j:any)=>['queued','downloading','paused'].includes(j.state)).length);

  // Statistics over every loaded tab.
  const stats = $derived.by(() => { const all = Object.values(libs).flatMap(l=>l?.items??[]); const per = new Map<string,{n:number;bytes:number}>();
    for (const m of all) { const k = m.base || 'Inconnu'; const v = per.get(k) ?? {n:0,bytes:0}; v.n++; v.bytes += m.bytes; per.set(k,v); }
    return { total: all.length, bytes: all.reduce((s,m)=>s+m.bytes,0), missing: all.filter(m=>!m.hasMetadata).length, favorites: all.filter(isFav).length, bases: [...per.entries()].sort((a,b)=>b[1].n-a[1].n) }; });

  const UI_KEY='saya.lora.ui';let uiReady=$state(false),timer:ReturnType<typeof setInterval>|null=null,alive=true;
  function saveUi(){try{localStorage.setItem(UI_KEY,JSON.stringify({tab,folder,search,sort,base,favoritesOnly,selections,stackName}))}catch{}}
  function restoreUi(){try{const v=JSON.parse(localStorage.getItem(UI_KEY)||'{}');if([...kinds.map(k=>k[0]),'civitai','stats'].includes(v.tab))tab=v.tab;if(typeof v.folder==='string')folder=v.folder;if(typeof v.search==='string')search=v.search;if(sorts.some(s=>s[0]===v.sort))sort=v.sort;if(typeof v.base==='string')base=v.base;if(typeof v.favoritesOnly==='boolean')favoritesOnly=v.favoritesOnly;if(Array.isArray(v.selections))selections=v.selections;if(typeof v.stackName==='string')stackName=v.stackName}catch{}}
  $effect(()=>{if(uiReady)saveUi()});
  $effect(()=>{ if (tab==='stats') for (const [k] of kinds) void load(k); else void load(kind); });
  onMount(async()=>{restoreUi();uiReady=true;
    try{const p=await call<Record<string,any>>('preferences');legacyFavorites=Array.isArray(p.loraFavorites)?p.loraFavorites:[];stacks=Array.isArray(p.loraStacks)?p.loraStacks:[];slots=p.generationLoraSlots??null;slot=slots?.slots[0]?.id??'';root=String(p.loraRoot??'');journal=typeof p.modelOrganizeJournal==='string'?p.modelOrganizeJournal:''}catch{}
    await pollJobs();if(alive)timer=setInterval(pollJobs,3000)});
  onDestroy(()=>{alive=false;if(uiReady)saveUi();if(timer)clearInterval(timer)});
  // Presentation helpers (V1 workbench): Civitai and statistics open as alternate center views.
  let libKind = $state<Kind>('loras');
  $effect(() => { if (tab !== 'civitai' && tab !== 'stats') libKind = tab; });
  const toggleView = (v:'civitai'|'stats') => { tab = tab === v ? libKind : v; };
  const missingCount = $derived(items.filter(m=>!m.hasMetadata).length);
  const favCount = $derived(items.filter(isFav).length);
  const dupCount = $derived(items.filter(m=>(dupes.get(m.sha256)??0)>1).length);
  // LoraManager sidecars keep the creator's advice as usage_tips = '{"strength": 0.8}'.
  const tipWeight = (meta:any) => { try { const u = typeof meta?.usage_tips==='string' ? JSON.parse(meta.usage_tips) : meta?.usage_tips; const v = u?.strength ?? meta?.preferred_weight; return typeof v==='number' ? String(v) : ''; } catch { return ''; } };
</script>

{#snippet branch(parent:string, depth:number)}
  {#each children(parent) as p (p)}
    {@const sub=children(p).length}
    <div class="tree-row" style="--d:{depth+1}">
      {#if sub}<button class="twisty" aria-label={open[p]?$t("Replier"):$t("Déplier")} onclick={()=>open={...open,[p]:!open[p]}}>{open[p]?'▾':'▸'}</button>{:else}<span class="twisty"></span>{/if}
      <button class="folder" class:chosen={folder===p} onclick={()=>{folder=p;if(sub)open={...open,[p]:true}}}><span>{p.split('/').pop()}</span><em class="num">{tree.get(p)}</em></button>
    </div>
    {#if sub&&open[p]}{@render branch(p,depth+1)}{/if}
  {/each}
{/snippet}

{#snippet card(m:Model)}
  <article class="model-card lora-item" data-lora-item={relPath(m)} class:dup={(dupes.get(m.sha256)??0)>1} class:cur={detail?.path===m.path} title={(dupes.get(m.sha256)??0)>1?$t("Doublons"):undefined} oncontextmenu={e=>openMenu(e,m)}>
    <button class="cover" aria-label={$t("Détails de {name}",{name:m.name})} onclick={()=>openDetail(m)}><ModelThumb load={m.preview?()=>modelThumb(kind,m.preview!):null} alt={m.name}/></button>
    <div class="card-acts">
      <button class="act fav" class:on={isFav(m)} title={$t("Favori")} onclick={()=>toggleFavorite(m)}>{isFav(m)?'★':'☆'}</button>
      {#if m.triggers.length}<button class="act" title={$t("Copier les triggers")} aria-label={$t("Copier les triggers")} onclick={()=>copy(m.triggers.join(', '),$t("Triggers copiés."))}><Icon name="copy" size={13}/></button>{/if}
      {#if kind==='embeddings'}<button class="act" title={$t("Copier pour un prompt")} aria-label={$t("Copier pour un prompt")} onclick={()=>copy(`embedding:${m.stem}`,$t("Embedding copié."))}><Icon name="copy" size={13}/></button>{/if}
    </div>
    <div class="meta" title={m.file}>{m.name}</div>
    <div class="card-foot"><small class="sub">{m.base||'—'}{m.versionName?` · ${m.versionName}`:''}</small>{#if !m.hasMetadata}<span class="chip warn">{$t("sans métadonnées")}</span>{/if}{#if kind==='loras'}<button class="add-btn" title={$t("Ajouter au stack")} onclick={()=>add(m)}>{$t("Ajouter")}</button>{/if}</div>
  </article>
{/snippet}

<div class="wb lora-manager">
  <aside class="wb-panel left">
    <div class="wb-head"><h2>{$t("Modèles")}</h2></div>
    <div class="kind-list" role="tablist" aria-label={$t("Types de modèles")}>{#each kinds as [k,label]}<button role="tab" aria-selected={tab===k} class:on={tab===k} onclick={()=>{tab=k;folder=''}}><span>{$t(label)}</span><em class="num">{libs[k]?.items.length??''}</em></button>{/each}</div>
    <div class="wb-scroll">
      <div class="tree lora-tree">
        <div class="tree-row" style="--d:0"><span class="twisty"></span><button class="folder" class:chosen={!folder} onclick={()=>folder=''}><span>{$t("Toutes")}</span><em class="num">{items.length}</em></button></div>
        {@render branch('',0)}
      </div>
      <details class="sec" open><summary><span class="sec-title">{$t("Filtres")}</span></summary><div class="sec-body lora-filter-list">
        <label class="check"><input type="checkbox" bind:checked={favoritesOnly}/><span>{$t("Favoris")}</span><em class="num">{favCount}</em></label>
        <label class="check"><input type="checkbox" bind:checked={missingOnly}/><span>{$t("sans métadonnées")}</span><em class="num">{missingCount}</em></label>
        <label class="check"><input type="checkbox" bind:checked={duplicatesOnly}/><span>{$t("Doublons")}</span><em class="num">{dupCount}</em></label>
        {#if fetching}<div class="fetch-row"><span class="muted num">{$t("Civitai : {done}/{total}",{done:fetching.done,total:fetching.total})}</span><button class="quiet" onclick={()=>fetching&&(fetching.stop=true)}>{$t("Arrêter")}</button></div>
        {:else}<button class="start" data-fetch-missing disabled={busy||!missingCount} onclick={fetchMissing}>{$t("Récupérer les métadonnées")}</button>{/if}
      </div></details>
      <details class="sec"><summary><span class="sec-title">{$t("Dossiers")}</span><span class="chip">{libs[kind]?.roots.length??0}</span></summary><div class="sec-body">
        {#each libs[kind]?.roots??[] as r}<p class="muted path" title={r.key}>{r.path}</p>{/each}
        <button data-lora-pick title={$t("Utilisé seulement si ComfyUI n’a jamais été joignable.")} onclick={pickRoot}>{$t("Dossier LoRA de secours…")}</button>
      </div></details>
    </div>
  </aside>

  <main class="wb-center">
    <div class="wb-bar lora-top">
      {#if tab==='civitai'}
        <input class="search" aria-label={$t("Rechercher sur Civitai…")} bind:value={remoteQuery} placeholder={$t("Rechercher sur Civitai…")} onkeydown={e=>e.key==='Enter'&&remoteSearch()} />
        <select aria-label={$t("Type")} bind:value={remoteType}><option value="LORA">{$t("LoRA")}</option><option value="Checkpoint">{$t("Checkpoint")}</option><option value="TextualInversion">{$t("Embedding")}</option><option value="LoCon">{$t("LyCORIS")}</option></select>
        <select aria-label={$t("Modèle de base")} bind:value={remoteBase}><option value="">{$t("Tous les modèles de base")}</option>{#each ['Illustrious','NoobAI','Pony','SDXL 1.0','SD 1.5','Flux.1 D','HiDream'] as b}<option value={b}>{b}</option>{/each}</select>
        <button class="primary" disabled={remoteBusy||!remoteQuery.trim()} onclick={remoteSearch}>{remoteBusy?$t("Recherche…"):$t("Rechercher")}</button>
        <span class="hint" title={$t("Les téléchargements arrivent dans le dossier du pont Civitai puis sont rangés automatiquement dans le dossier du moteur ({modèle de base}/{catégorie}), avec leurs métadonnées et leur aperçu.")}><Icon name="info" size={14}/></span>
      {:else}
        <input class="search" aria-label={$t("Rechercher un modèle")} placeholder={$t("Rechercher un nom, un tag, un déclencheur…")} title={$t("Nom, fichier, tag, trigger, modèle de base")} bind:value={search} disabled={tab==='stats'} />
        <select data-model-sort aria-label={$t("Trier")} bind:value={sort} disabled={tab==='stats'}>{#each sorts as [id,l]}<option value={id}>{$t(l)}</option>{/each}</select>
        <select data-model-base aria-label={$t("Base")} bind:value={base} disabled={tab==='stats'}><option value="">{$t("Base : toutes")}</option>{#each bases as b}<option value={b}>{b}</option>{/each}</select>
      {/if}
      <span class="grow"></span>
      <button class:on={tab==='civitai'} data-civitai-toggle aria-pressed={tab==='civitai'} onclick={()=>toggleView('civitai')}>{$t("Civitai")}{#if activeJobs}<span class="num jobs"> · {activeJobs} ↓</span>{/if}</button>
      <button class:on={tab==='stats'} aria-pressed={tab==='stats'} onclick={()=>toggleView('stats')}>{$t("Statistiques")}</button>
      {#if kind!=='others'&&tab!=='civitai'&&tab!=='stats'}<button data-organize disabled={busy} onclick={planOrganize}>{$t("Ranger automatiquement…")}</button>{/if}
      <button data-lora-refresh disabled={!!loading||tab==='civitai'} onclick={()=>tab==='stats'?kinds.forEach(([k])=>load(k,true)):load(kind,true)}><Icon name="redo" size={14}/><span>{$t("Actualiser")}</span></button>
    </div>
    {#if error||note||journal}<div class="msgline">
      {#if error}<span class="msg err" role="alert" title={error}>{error}</span><button class="icon-btn quiet x" aria-label={$t("Fermer")} onclick={()=>error=''}><Icon name="close" size={12}/></button>{/if}
      {#if note}<span class="msg" role="status" title={note}>{note}</span><button class="icon-btn quiet x" aria-label={$t("Fermer")} onclick={()=>note=''}><Icon name="close" size={12}/></button>{/if}
      {#if journal}<span class="msg journal">{$t("Dernier rangement automatique enregistré.")}</span><button class="quiet" disabled={busy} onclick={undoOrganize}>{$t("Annuler ce rangement")}</button>{/if}
    </div>{/if}

    <div class="wb-scroll content">
    {#if tab==='civitai'}
      <section class="civitai-panel" data-civitai-panel>
        {#if Object.keys(jobState).length}<details class="sec downloads" data-civitai-downloads bind:open={downloadsOpen}><summary><span class="sec-title">{$t("Téléchargements Civitai ({n})",{n:Object.keys(jobState).length})}</span>{#if activeJobs}<span class="chip">{$t("{n} actif(s)",{n:activeJobs})}</span>{/if}</summary>
          <div class="sec-body">
            {#if jobsSyncError}<p class="msg err">{jobsSyncError}</p>{/if}{#if jobsError}<p class="msg err">{jobsError}</p>{/if}
            {#each Object.entries(jobState) as [id,j] (id)}{@const p=percent(j)}
              <div class="job" data-civitai-job={id} data-job-state={j.state??'unknown'}>
                <span class="job-name" title={j.name}>{j.name??$t("Tâche {id}",{id:id.slice(0,8)})}</span>
                <span class="muted job-state">{$t(jobLabel(j.state))}{jobMap()[id]?.imported?' · '+$t("rangé"):''}{p!==null?` · ${p} %`:''}</span>
                <span class="job-acts">{#if j.state!=='done'&&j.state!=='cancelled'}<button class="quiet" data-civitai-cancel={id} disabled={jobBusy[id]} onclick={()=>jobControl('cancel',id)}>{$t("Annuler")}</button>{/if}{#if j.state==='queued'||j.state==='downloading'}<button class="quiet" data-civitai-pause={id} disabled={jobBusy[id]} onclick={()=>jobControl('pause',id)}>{$t("Pause")}</button>{/if}{#if ['paused','cancelled','error'].includes(j.state)}<button class="quiet" data-civitai-resume={id} disabled={jobBusy[id]} onclick={()=>jobControl('resume',id)}>{$t("Reprendre")}</button>{/if}</span>
                {#if p!==null}<div class="progress" role="progressbar" aria-valuenow={p}><span style:width={`${p}%`}></span></div>{/if}
                {#if j.error}<small class="msg err" title={j.error}>{j.error}</small>{/if}
              </div>
            {/each}
          </div></details>{/if}
        {#if remoteError}<p class="inline-msg muted">{remoteError}</p>{/if}
        <div class="model-grid">{#each remoteResults as m (m.id)}{@const versions=m.modelVersions??[]}{@const v=versions.find((x:any)=>x.id===versionOf[m.id])??versions[0]}{@const img=v?.images?.find((i:any)=>i.type!=='video')?.url}
          <article class="model-card" data-civitai-model={m.id}>
            <div class="cover"><ModelThumb load={img?()=>civitaiThumb(img):null} alt={m.name} label={$t("Sans aperçu")}/></div>
            <div class="meta" title={m.name}>{m.name}</div>
            <small class="sub">{m.type}{v?.baseModel?` · ${v.baseModel}`:''} · {m.creator?.username??'Civitai'} · {$t("{n} téléch.",{n:(m.stats?.downloadCount??0).toLocaleString('fr-FR')})}</small>
            <div class="remote-acts">
              {#if versions.length>1}<select aria-label={$t("Version")} value={v?.id} onchange={e=>versionOf={...versionOf,[m.id]:Number(e.currentTarget.value)}}>{#each versions as x (x.id)}<option value={x.id}>{x.name} · {x.baseModel}</option>{/each}</select>{:else if v}<span class="chip" title={v.name}>{v.name}</span>{/if}
              <button disabled={remoteBusy||!v||!typeKind(m.type)} title={typeKind(m.type)?'':$t("Type non rangé automatiquement")} onclick={()=>remoteDownload(m,v)}>{$t("Télécharger")}</button>
            </div>
          </article>{/each}</div>
      </section>
    {:else if tab==='stats'}
      <section class="stats" data-model-stats>
        <dl class="stat-row"><div><dt>{$t("modèles")}</dt><dd class="num">{stats.total}</dd></div><div><dt>{$t("sur le disque")}</dt><dd class="num">{fmtBytes(stats.bytes)}</dd></div><div><dt>{$t("sans métadonnées")}</dt><dd class="num">{stats.missing}</dd></div><div><dt>{$t("favoris")}</dt><dd class="num">{stats.favorites}</dd></div></dl>
        <div class="sec-title stats-title">{$t("Par modèle de base")}</div>
        <div class="base-bars">{#each stats.bases as [name,v] (name)}<div class="base-bar"><span title={name}>{$t(name)}</span><div><i style:width={`${Math.max(2,v.n/stats.total*100)}%`}></i></div><small class="num">{v.n} · {fmtBytes(v.bytes)}</small></div>{/each}</div>
        <p class="muted">{kinds.map(([k,l])=>`${$t(l)} : ${libs[k]?.items.length??'…'}`).join(' · ')}</p>
      </section>
    {:else if plan}
      <section class="organize" data-organize-plan>
        <div class="org-head"><strong>{$t("Rangement proposé · {n} modèle(s)",{n:planMoves.length})}</strong><span class="muted">{$t("{n} sans métadonnées ignoré(s).",{n:plan.skipped})}</span>
          <span class="hint" title={$t("Règle LoraManager : {modèle de base}/{catégorie} (catégorie = premier tag parmi character, concept, clothing, style, poses…). Le modèle se déplace avec ses métadonnées et son aperçu ; rien n’est écrasé ; un journal permet d’annuler.")}><Icon name="info" size={14}/></span>
          <span class="grow"></span><button class="quiet" onclick={()=>plan=null}>{$t("Fermer")}</button></div>
        <p class="muted warn-line">{$t("Attention :")} {$t("un workflow qui cite l’ancien chemin d’un checkpoint ou d’une LoRA en liste ComfyUI devra être mis à jour (les loaders LoraManager retrouvent les LoRA par leur nom).")}</p>
        <div class="organize-scope"><label class="check"><input type="radio" name="organize-scope" checked={!organizeAll} onchange={()=>organizeAll=false}/><span>{$t("Seulement les modèles posés à la racine ({n})",{n:plan.moves.filter(m=>!m.fromFolder).length})}</span></label><label class="check"><input type="radio" name="organize-scope" checked={organizeAll} onchange={()=>organizeAll=true}/><span>{$t("Tout ranger comme LoraManager, dossiers perso compris ({n})",{n:plan.moves.length})}</span></label></div>
        {#if planMoves.length}<div class="org-acts"><button class="quiet" onclick={()=>chosen=Object.fromEntries(plan!.moves.map(m=>[m.from,!m.conflict]))}>{$t("Tout cocher")}</button><button class="quiet" onclick={()=>chosen={}}>{$t("Tout décocher")}</button><span class="grow"></span><button class="primary" disabled={busy||!planMoves.some(m=>chosen[m.from])} onclick={applyOrganize}>{$t("Déplacer {n} modèle(s)",{n:planMoves.filter(m=>chosen[m.from]).length})}</button></div>
        <div class="organize-list">{#each planMoves as m (m.from)}<label class="org-row" class:conflict={m.conflict}><input type="checkbox" disabled={m.conflict} checked={!!chosen[m.from]} onchange={e=>chosen={...chosen,[m.from]:e.currentTarget.checked}}/><span title={m.from}>{m.from.split('/').pop()}</span><small class="muted">{m.fromFolder||$t("Racine")} → {m.toFolder}{m.conflict?' · '+$t("nom déjà pris"):''}</small></label>{/each}</div>
        {:else}<p class="muted">{$t("Tout est déjà rangé.")}</p>{/if}
      </section>
    {:else}
      <div class="crumbs"><button class="quiet" onclick={()=>folder=''}>{$t(kinds.find(k=>k[0]===kind)?.[1]??'')}</button>{#each folder?folder.split('/'):[] as part,i}<span class="sep">/</span><button class="quiet" onclick={()=>folder=folder.split('/').slice(0,i+1).join('/')}>{part}</button>{/each}<span class="muted num">· {$tn("{n} modèle","{n} modèles",shown.length)}</span></div>
      {#if loading===kind&&!items.length}<p class="inline-msg muted">{$t("Lecture des dossiers…")}</p>{/if}
      <div class="model-grid">{#each shown.slice(0,limit) as m (m.path)}{@render card(m)}{/each}</div>
      {#if shown.length>limit}<div class="more"><button class="quiet" onclick={()=>limit+=240}>{$t("Afficher plus ({n} restants)",{n:shown.length-limit})}</button></div>{/if}
      {#if !shown.length&&!loading}<p class="inline-msg muted empty">{items.length?$t("Aucun modèle ne correspond."):$t("Aucun modèle trouvé dans les dossiers du moteur.")}</p>{/if}
    {/if}
    </div>
  </main>

  <aside class="wb-panel right">
    <div class="wb-head">
      {#if detail}<h2 title={detail.file}>{detail.name}</h2>
        <button class="icon-btn quiet fav" class:on={isFav(detail)} aria-pressed={isFav(detail)} aria-label={$t("Favori")} title={$t("Favori")} onclick={()=>toggleFavorite(detail!)}><Icon name="star" size={15}/></button>
        <button class="icon-btn quiet" aria-label={$t("Fermer")} title={$t("Fermer")} onclick={()=>detail=null}><Icon name="close" size={14}/></button>
      {:else}<h2 class="muted">{$t(kinds.find(k=>k[0]===kind)?.[1]??'')}</h2>{/if}
    </div>
    <div class="wb-scroll">
      {#if detail}<section data-model-detail role="group" aria-label={detail.name} oncontextmenu={e=>openMenu(e,detail!)}>
        {#if kind==='loras'}<div class="send-top"><button class="primary" disabled={busy} onclick={()=>sendLoraToCreate(detail!)}>{$t("Envoyer à Créer")}</button><button class="quiet" onclick={()=>add(detail!)}>{$t("Ajouter au stack")}</button></div>{/if}
        <div class="inspector-image"><div class="cover big"><ModelThumb load={detail.preview?()=>modelThumb(kind,detail!.preview!):null} alt={detail.name}/></div></div>
        <div class="ins-block">
          <dl class="kv">
            <dt>{$t("Base")}</dt><dd>{detail.base||'—'}</dd>
            {#if detail.versionName}<dt>{$t("Version")}</dt><dd>{detail.versionName}</dd>{/if}
            <dt>{$t("Triggers")}</dt><dd>{#if detail.triggers.length}<span class="chips">{#each detail.triggers as trg}<button class="tag" title={$t("Copier")} onclick={()=>copy(trg,$t("Trigger copié."))}>{trg}</button>{/each}<button class="tag quiet-tag" title={$t("Tout copier")} aria-label={$t("Tout copier")} onclick={()=>copy(detail!.triggers.join(', '),$t("Triggers copiés."))}><Icon name="copy" size={12}/></button></span>{:else}<span class="muted">—</span>{/if}</dd>
            {#if tipWeight(detailMeta)}<dt>{$t("Poids conseillé")}</dt><dd class="num">{tipWeight(detailMeta)}</dd>{/if}
            <dt>{$t("Fichier")}</dt><dd title={detail.path}>{detail.folder?`${detail.folder}/`:''}{detail.file}</dd>
            <dt>{$t("Taille")}</dt><dd class="num">{fmtBytes(detail.bytes)}</dd>
            <dt>{$t("Modifié")}</dt><dd class="num">{new Date(detail.modified*1000).toLocaleString('fr-FR')}</dd>
          </dl>
          {#if detail.tags.length}<div class="chips tags">{#each detail.tags as tg}<button class="tag" title={$t("Rechercher")} onclick={()=>{search=tg;detail=null}}>{tg}</button>{/each}</div>{/if}
        </div>
        <div class="wb-actions two">
          {#if kind==='embeddings'}<button onclick={()=>copy(`embedding:${detail!.stem}`,$t("Embedding copié."))}>{$t("Copier pour un prompt")}</button>{/if}
          <button disabled={busy} onclick={()=>fetchOne(detail!)}>{detail.hasMetadata?$t("Actualiser depuis Civitai"):$t("Récupérer depuis Civitai")}</button>
          {#if civitaiUrl(detail)}<button class="quiet" onclick={()=>copy(civitaiUrl(detail!),$t("Lien Civitai copié."))}>{$t("Copier le lien Civitai")}</button>{/if}
          <button class="quiet" onclick={()=>copy(detail!.path,$t("Chemin copié."))}>{$t("Copier le chemin")}</button>
        </div>
        {#if detailMeta?.modelDescription}<details class="sec"><summary><span class="sec-title">{$t("Description")}</span></summary><div class="sec-body"><p class="description">{plainText(detailMeta.modelDescription)}</p></div></details>{/if}
        {#if detail.sha256}<details class="sec"><summary><span class="sec-title">{$t("SHA-256")}</span></summary><div class="sec-body"><code class="hash">{detail.sha256}</code></div></details>{/if}
        {#if detail.ext==='safetensors'}<details class="sec" bind:open={metaEditing}><summary><span class="sec-title">{$t("Modifier les métadonnées (JSON)")}</span></summary><div class="sec-body"><textarea rows="12" spellcheck="false" bind:value={metaText}></textarea><button onclick={saveMeta}><Icon name="save" size={14}/> {$t("Enregistrer")}</button></div></details>{/if}
      </section>{:else}<p class="inline-msg muted">{$t("Sélectionne un modèle pour voir ses détails.")}</p>{/if}

      {#if kind==='loras'}<section class="lora-stack">
        <div class="stack-head"><span class="sec-title">{$t("Stack")}</span>{#if selections.length}<span class="muted cols">{$t("MODEL")}</span><span class="muted cols">{$t("CLIP")}</span><span class="cols-pad"></span>{:else}<span class="chip num">0</span>{/if}</div>
        {#each selections as s,i}<div class="lora-sel" class:off={s.enabled===false}>
          <input type="checkbox" aria-label={$t("actif")} title={$t("actif")} bind:checked={s.enabled}/>
          <span class="sel-name" title={s.relativePath}>{s.name}</span>
          <input type="number" class="num" step="0.05" aria-label={$t("MODEL")} bind:value={s.modelWeight}/>
          <input type="number" class="num" step="0.05" aria-label={$t("CLIP")} bind:value={s.clipWeight}/>
          <button class="icon-btn quiet" aria-label={$t("Retirer")} title={$t("Retirer")} onclick={()=>selections=selections.filter((_,j)=>j!==i)}><Icon name="close" size={12}/></button>
        </div>{:else}<p class="muted">{$t("Aucune LoRA sélectionnée.")}</p>{/each}
        <div class="stack-save"><input bind:value={stackName} placeholder={$t("Nom du stack")} aria-label={$t("Nom du stack")} /><button onclick={saveStack}>{$t("Enregistrer")}</button></div>
        {#if stacks.length}<select aria-label={$t("Charger un stack…")} value="" onchange={e=>{loadStack(e.currentTarget.value);e.currentTarget.value=''}}><option value="">{$t("Charger un stack…")}</option>{#each stacks as s (s.name)}<option value={s.name}>{s.name} · {s.selections.length}</option>{/each}</select>{/if}
        <button class="primary" onclick={sendToGeneration}>{$t("Envoyer à Créer")}</button>
        {#if slots?.slots.length}<div class="slot-send" data-lora-slot-send><select aria-label={$t("Slot du workflow")} title={$t("Slot du workflow")} bind:value={slot}>{#each slots.slots as x (x.id)}<option value={x.id}>{x.title}</option>{/each}</select><button disabled={!selections.some(x=>x.enabled!==false)} onclick={sendToSlot}>{$t("Envoyer dans ce slot")}</button></div>
        {:else}<p class="muted small">{$t("Ouvre un workflow dans Génération pour choisir un slot.")}</p>{/if}
      </section>{/if}
    </div>
  </aside>
</div>

<svelte:window onkeydown={menuKey} onpointerdown={menuOutside} onscrollcapture={closeMenu} onresize={closeMenu} onblur={closeMenu}/>
{#if menu}<div class="ctx-menu" role="menu" tabindex="-1" aria-label={$t("Actions de {name}",{name:menu.m.name})} bind:this={menuEl} oncontextmenu={e=>e.preventDefault()}>
  <button role="menuitem" disabled={busy} onclick={()=>menuRun(()=>sendLoraToCreate(menu!.m))}>{$t("Envoyer à Créer")}</button>
  <button role="menuitem" onclick={()=>menuRun(()=>add(menu!.m))}>{$t("Ajouter au stack")}</button>
  {#each slots?.slots??[] as x (x.id)}<button role="menuitem" disabled={busy} onclick={()=>menuRun(()=>sendLoraToSlot(menu!.m,x.id))}>{$t("Envoyer vers {slot}",{slot:x.title})}</button>{/each}
</div>{/if}

<style>
  .lora-manager{--wb-left:240px;--wb-right:320px}
  /* Left: kind switch, folder tree, filters */

  .tree{padding:6px 0;border-bottom:1px solid var(--line)}
  .tree-row{display:flex;align-items:center;padding-inline-start:calc(4px + var(--d,0)*14px)}
  .twisty{width:18px;min-height:24px;flex:none;padding:0;border:0;background:none;color:var(--faint);font-size:10px;display:inline-grid;place-items:center}
  .folder{flex:1;min-width:0;min-height:26px;display:flex;align-items:center;gap:6px;text-align:start;border:0;border-radius:0;background:none;color:var(--muted);padding:0 12px 0 2px}
  .folder span{flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .folder em,.check em{font-style:normal;color:var(--faint);font-size:11.5px}
  .folder:hover:not(:disabled){background:none;color:var(--text)}
  .tree-row:has(.folder.chosen){background:var(--raised)}.folder.chosen{color:var(--text)}
  .lora-filter-list{gap:4px}
  .check{display:flex;flex-direction:row;align-items:center;gap:8px;color:var(--text);font-size:12.5px;min-height:24px;cursor:pointer}
  .check input{margin:0;min-height:0;width:13px;height:13px;accent-color:var(--sel)}.check span{flex:1}
  .fetch-row{display:flex;align-items:center;justify-content:space-between;gap:6px}
  .start{justify-self:start;padding:0 6px;margin-inline-start:-6px}
  .path{margin:0;font-size:11.5px;overflow-wrap:anywhere}
  /* Center: bar, message line, grid */
  .search{width:min(340px,40%)}
  .wb-bar button.on{color:var(--text);background:var(--raised)}
  .jobs{color:var(--accent)}
  .hint{display:inline-grid;place-items:center;color:var(--faint);cursor:help;width:22px}
  .msgline{display:flex;align-items:center;gap:6px;min-height:26px;padding:0 10px;border-bottom:1px solid var(--line);background:var(--bg);font-size:12px;flex:none}
  .msg{min-width:0;flex:0 1 auto;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .msg.err::before{content:"";display:inline-block;width:6px;height:6px;border-radius:50%;background:var(--danger);margin-inline-end:6px;vertical-align:middle}
  .msgline .x{width:20px;min-height:20px}.msgline .quiet{min-height:20px;font-size:12px}
  .journal{margin-inline-start:auto}
  .content{padding:12px 14px}
  .crumbs{display:flex;align-items:center;gap:2px;margin:-4px 0 8px;font-size:12px}.crumbs button{min-height:22px;padding:0 4px}.crumbs .sep{color:var(--faint)}.crumbs .muted{margin-inline-start:4px}
  .model-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:14px 12px;align-content:start}
  .model-card{position:relative;min-width:0}
  .cover{display:block;width:100%;aspect-ratio:3/4;padding:0;border:0;border-radius:var(--r-img);overflow:hidden;background:var(--raised)}
  button.cover:hover:not(:disabled){background:var(--raised)}
  .model-card.cur .cover{outline:2px solid var(--sel);outline-offset:2px}
  .model-card.dup .cover{outline:1px dashed var(--warning);outline-offset:2px}
  .model-card.dup.cur .cover{outline:2px solid var(--sel)}
  .meta{font-size:12px;margin-top:6px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .sub{display:flex;align-items:center;gap:5px;color:var(--muted);font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  
  .card-acts{position:absolute;top:6px;inset-inline:6px;display:flex;gap:4px;justify-content:flex-end;pointer-events:none}
  .act{pointer-events:auto;min-height:22px;height:22px;padding:0 6px;font-size:12px;line-height:1;background:#000000b3;border-color:transparent;color:var(--text);display:inline-grid;place-items:center;opacity:0}
  .act:hover:not(:disabled){background:#000000e0}
  .act.fav{order:-1;margin-inline-end:auto;font-size:13px}.act.fav.on{opacity:1;color:var(--accent-warm)}
  .model-card:hover .act,.model-card:focus-within .act{opacity:1}
  .remote-acts{display:flex;gap:4px;align-items:center;margin-top:4px}.remote-acts select{flex:1;min-width:0;min-height:24px;font-size:11px}.remote-acts .chip{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis}.remote-acts button{min-height:24px;font-size:11.5px}
  .more{display:flex;justify-content:center;padding:14px 0}
  .inline-msg{margin:0;padding:12px}.empty{text-align:center;padding:40px 12px}
  /* Civitai downloads */
  .downloads{margin:-12px -14px 12px;background:var(--bg)}
  .job{display:grid;grid-template-columns:minmax(0,1fr) auto auto;gap:2px 8px;align-items:center;padding:4px 0;border-top:1px solid var(--line)}
  .job-name{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.job-state{font-size:11.5px}.job-acts{display:flex;gap:2px}.job-acts button{min-height:22px;font-size:11.5px}
  .progress{grid-column:1/-1;height:3px;background:var(--raised);border-radius:2px;overflow:hidden}.progress span{display:block;height:100%;background:var(--accent)}
  .job small{grid-column:1/-1}
  /* Statistics */
  .stats{display:grid;gap:12px;max-width:900px}
  .stat-row{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));margin:0;border:1px solid var(--line);border-radius:var(--radius)}
  .stat-row div{padding:10px 12px}.stat-row div+div{border-inline-start:1px solid var(--line)}
  .stat-row dt{color:var(--muted);font-size:11.5px}.stat-row dd{margin:2px 0 0;font-size:18px;font-weight:600}
  .stats-title{margin-top:6px}
  .base-bars{display:grid;gap:4px}.base-bar{display:grid;grid-template-columns:180px minmax(0,1fr) 130px;gap:10px;align-items:center}
  .base-bar span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.base-bar div{height:6px;background:var(--raised);border-radius:3px;overflow:hidden}.base-bar i{display:block;height:100%;background:var(--muted)}.base-bar small{color:var(--muted)}
  /* Organize plan */
  .organize{display:grid;gap:8px;max-width:1100px}
  .org-head,.org-acts{display:flex;align-items:center;gap:10px}.grow{flex:1}
  .warn-line{margin:0;font-size:12px}
  .organize-scope{display:flex;gap:18px;flex-wrap:wrap}
  .organize-list{display:grid;border-top:1px solid var(--line)}
  .org-row{display:grid;grid-template-columns:18px minmax(0,1fr) minmax(0,1.2fr);gap:8px;align-items:center;min-height:26px;border-bottom:1px solid var(--line);font-size:12px}
  .org-row span,.org-row small{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.org-row input{margin:0;min-height:0}.org-row.conflict{opacity:.5}
  /* Right: inspector + stack */
  .fav.on{color:var(--accent-warm)}
  .cover.big{aspect-ratio:3/4}
  .ins-block{padding:12px;border-bottom:1px solid var(--line);display:grid;gap:10px}
  .chips{display:flex;flex-wrap:wrap;gap:4px}
  .tag{min-height:20px;padding:0 6px;font-size:11.5px;border-color:var(--line);background:var(--raised);border-radius:3px;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .quiet-tag{background:transparent;border-color:transparent;color:var(--muted);display:inline-grid;place-items:center}
  .wb-actions.two{grid-template-columns:1fr 1fr;border-bottom:1px solid var(--line)}
  .wb-actions button{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .description{margin:0;white-space:pre-wrap;color:var(--muted);max-height:260px;overflow:auto}
  .hash{font-size:11px;overflow-wrap:anywhere;color:var(--muted)}
  .sec-body textarea{width:100%;font-family:ui-monospace,monospace;font-size:11.5px}
  .sec-body button{justify-self:start;display:inline-flex;align-items:center;gap:6px}
  .lora-stack{padding:12px;display:grid;gap:8px}
  .stack-head,.lora-sel{display:grid;grid-template-columns:16px minmax(0,1fr) 52px 52px 22px;gap:6px;align-items:center}
  .stack-head .sec-title{grid-column:1/3}.stack-head .chip{grid-column:3/-1;justify-self:end}.cols{font-size:10.5px;text-align:center}
  .lora-sel input[type=number]{min-height:24px;padding:0 4px;text-align:end}.lora-sel input[type=checkbox]{margin:0;min-height:0;accent-color:var(--sel)}
  .lora-sel .icon-btn{width:22px;min-height:22px}
  .lora-sel.off .sel-name{color:var(--faint)}
  .sel-name{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .stack-save{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:6px}
  .slot-send{display:grid;grid-template-columns:minmax(0,1fr);gap:6px}
  .kind-list{display:flex;flex-direction:column;padding:4px 8px;border-bottom:1px solid var(--line)}
  .kind-list button{display:flex;justify-content:space-between;align-items:center;background:transparent;border-color:transparent;text-align:start}
  .kind-list button.on{background:var(--raised);color:var(--text)}
  .kind-list em{font-style:normal;color:var(--muted);font-size:11px}
  .card-foot{display:flex;flex-wrap:wrap;align-items:center;gap:4px 6px;min-width:0;padding:0 8px 8px}
  [data-lora-refresh]{display:inline-flex;align-items:center;gap:6px}
  .card-foot .sub{flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .card-foot .chip.warn{font-size:10px;color:var(--warn);border-color:var(--warn)}
  .add-btn{min-height:24px;height:24px;padding:0 8px;font-size:12px}
  .send-top{display:grid;grid-template-columns:1fr auto;gap:6px;padding:12px 12px 0}
  .send-top button{min-width:0}
  .ctx-menu{position:fixed;left:0;top:0;z-index:1000;min-width:180px;max-width:min(320px,calc(100vw - 8px));max-height:calc(100vh - 8px);overflow:auto;display:grid;padding:4px;background:var(--panel);border:1px solid var(--line);border-radius:6px;box-shadow:0 6px 24px rgb(0 0 0 / .35)}
  .ctx-menu button{background:transparent;border-color:transparent;color:var(--text);text-align:start;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .ctx-menu button:hover:not(:disabled),.ctx-menu button:focus-visible{background:var(--hover);outline:1px solid var(--accent);outline-offset:-1px}
  .small{font-size:11.5px;margin:0}
  .lora-stack p{margin:0}
</style>
