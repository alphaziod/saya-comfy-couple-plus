<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount, onDestroy } from 'svelte';
  import { call, message, native, type CharacterCard, type PreparedJob } from '../lib/api';
  import { invoke } from '@tauri-apps/api/core';
  import {resolveSeed} from '../lib/generation-state';
  import PreparedDraft from './PreparedDraft.svelte';
  import Thumbnail from './Thumbnail.svelte';

  type Field = { nodeId: string; inputName: string; classType: string; valueType: string; value: string; enabled: boolean; section?: string | null };
  type FullCharacter = { id:string; name:string; identity:any };
  type Sheet = { id: string; name: string; identity?:any; description?:string; traits?:string; identityPrompt: string; anatomyPrompt: string; backgrounds: {variantId: string; variantName: string; bg: string}[]; variants: {id: string; name: string}[] };
  type Profile = { id: string; name: string; workflowId: string; workflowName: string; mode: 'solo'|'duo'; config: any; changed?: boolean };
  type Workflow = { id: string; name: string; fields: Field[]; nodeCount: number };
  let { embedded = false, restoredImage = null }: { embedded?: boolean; restoredImage?: any } = $props();

  const commands = [
    ['P1','Personnage P1'],['ANATOMY_P1','Anatomie P1'],['P2','Personnage P2'],['ANATOMY_P2','Anatomie P2'],
    ['BG','Décor BG'],['ACTION','Action'],['QUALITY','Qualité'],['ADDITIONAL','Additionnel'],['NEG','Négatif']
  ];
  let draftReady=$state(false),lastQueued=$state('');
  let draftTimer:ReturnType<typeof setTimeout>|undefined;
  let draftWrite=Promise.resolve();
  let profiles = $state<Profile[]>([]), workflows = $state<{id:string;name:string}[]>([]), cards = $state<CharacterCard[]>([]);
  let workflow = $state<Workflow|null>(null), sheet = $state<Sheet|null>(null), sheetRole = $state<'p1'|'p2'>('p1'), fullCharacters = $state<FullCharacter[]>([]);
  let rolePrompts = $state<{p1:string;p2:string}>({p1:'',p2:''}), roleAnatomy = $state<{p1:string;p2:string}>({p1:'',p2:''}), pickerRole = $state<'p1'|'p2'|null>(null);
  let selected = $state<Profile|null>(null), error = $state(''), note = $state(''), busy = $state(false);
  let preview = $state<any>(null), search = $state(''), favoritesOnly = $state(false), importText = $state(''), audit = $state<any>(null);
  let categoryFilter = $state('');
  let selectedProfile = $state(''), workflowId = $state(''), name = $state('Nouveau panneau'), mode = $state<'solo'|'duo'>('solo');
  let config = $state<any>(blank());
  let backgroundPool = $state<{variantId:string;variantName:string;bg:string;enabled:boolean}[]>([]);
  let actionText = $state('');
  let loraDirectory = $state(''), loraItems = $state<any[]>([]), loraSearch = $state(''), loraError = $state(''), loraLoaders = $state<any[]>([]), loraLoader = $state(''), loraPreview = $state<any>(null);
  let activeLoraStack = $state<any[]>([]), loraCovers = $state<Record<string,string>>({}), loraCategory = $state(''), loraSort = $state('name'), loraFavoritesOnly = $state(false), loraLimit = $state(48), stackName = $state(''), activeStack = $state(''), loraRoot = $state(''), loraMetaItem = $state<any>(null), loraMetaText = $state(''), loraMetaError = $state('');

  function blank() {
    return {
      p1:{characterId:'',variantId:null}, p2:{characterId:'',variantId:null},
      bg:{source:'fixed',text:'',selected:null,enabledByVariant:[]}, quality:'', additional:'', masterSeed:'',
      action:{source:'fixed',text:'',selected:null,candidates:[]},
      seedMode:'fixed', promptOverrides:{}, negative:null,
      mappings:{}, quick:[], favorites:[], categories:{}
    };
  }
  async function load() {
    if (busy) return; busy = true; error = '';
    try {
      [profiles, workflows, cards, fullCharacters] = await Promise.all([call<Profile[]>('prepare_profiles'), call<{id:string;name:string}[]>('workflows'), call<CharacterCard[]>('character_cards'), call<FullCharacter[]>('characters')]);
      if (profiles.length && !selectedProfile) await loadProfile(profiles[0].id);
      else if (!workflowId && workflows.length) { workflowId = workflows[0].id; await loadWorkflow(); }
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function loadWorkflow() {
    if (!workflowId) return;
    workflow = await call<Workflow>('workflow', { id: workflowId });
    lastQueued=await call<string|null>('last_queued_seed',{workflow:workflowId})??'';
    loraLoaders=await call<any[]>('lora_loaders',{workflow:workflowId});
    if(!loraLoaders.some(l=>l.nodeId===loraLoader))loraLoader=loraLoaders.length===1?loraLoaders[0].nodeId:'';
    const quick = config.quick ?? [];
    const additions:any[] = [];
    for (const f of workflow.fields.filter(f=>f.classType==='SayaMainPromptFR' && !['seed','control_after_generate'].includes(f.inputName))) {
      if (!quick.some((q:any)=>q.nodeId===f.nodeId&&q.inputName===f.inputName)) additions.push({nodeId:f.nodeId,inputName:f.inputName,value:f.value,visible:true,label:f.inputName,bgOption:true});
    }
    if (additions.length) config.quick=[...quick,...additions];
  }
  async function loadProfile(id: string) {
    const profile = await call<Profile>('prepare_profile', { id });
    selected = profile; selectedProfile = profile.id; name = profile.name; workflowId = profile.workflowId; mode = profile.mode; config = { ...blank(), ...profile.config };
    await loadWorkflow();
    await refreshBackgrounds();
    await refreshRolePrompts();
  }
  async function saveProfile() {
    if (!workflowId) throw new Error($t("Choisir un workflow"));
    const saved = await call<Profile>('save_prepare_profile', { id: selected?.id ?? null, name, workflow: workflowId, mode, config });
    selected = saved; selectedProfile = saved.id;
    profiles = await call<Profile[]>('prepare_profiles');
    note = $t("Profil enregistré.");
  }
  async function newProfile() { rolePrompts={p1:'',p2:''};roleAnatomy={p1:'',p2:''};actionText=''; selected = null; selectedProfile=''; config = blank(); name='Nouveau panneau'; preview=null; backgroundPool=[]; if (workflowId) await loadWorkflow(); }
  function isFavorite(id:string) { return config.favorites?.includes(id); }
  function toggleFavorite(id:string) {
    config.favorites = isFavorite(id) ? config.favorites.filter((v:string)=>v!==id) : [...(config.favorites??[]), id];
  }
  const categories = $derived([...new Set(Object.values(config.categories ?? {}).filter(Boolean))] as string[]);
  const filtered = $derived(cards.filter(c => (!favoritesOnly || isFavorite(c.id)) && c.name.toLowerCase().includes(search.toLowerCase()) && (!categoryFilter || config.categories?.[c.id] === categoryFilter)));
  const roleName = (role:'p1'|'p2') => cards.find(c=>c.id===config[role]?.characterId)?.name ?? '—';
  async function choose(role:'p1'|'p2', id:string) {
    config[role] = { ...config[role], characterId:id,variantId:null };
    delete config.promptOverrides?.[role.toUpperCase()];delete config.promptOverrides?.['ANATOMY_'+role.toUpperCase()];
    if(!id){rolePrompts[role]='';roleAnatomy[role]='';sheet=null;return;}
    sheet = null; sheetRole = role;
    await refreshBackgrounds();
    await refreshRolePrompts();
  }
  async function refreshRolePrompts() {
    for (const role of ['p1','p2'] as const) {
      const id=config[role]?.characterId; if(!id){rolePrompts[role]='';roleAnatomy[role]='';continue}
      const data=await call<any>('character_sheet',{id}); rolePrompts[role]=config.promptOverrides?.[role.toUpperCase()]??data.identityPrompt??'';roleAnatomy[role]=config.promptOverrides?.['ANATOMY_'+role.toUpperCase()]??data.anatomyPrompt??'';
    }
  }
  async function openSheet(role:'p1'|'p2') {
    const id = config[role]?.characterId; if (!id) return;
    const data = await call<any>('character_sheet', { id });
    const full = fullCharacters.find(c=>c.id===id);
    sheet = { ...data, identity: full?.identity ?? {}, description:full?.identity?.description ?? '', traits:full?.identity?.studioTraits ?? full?.identity?.traits ?? '' }; sheetRole=role;
  }
  async function saveSheet() {
    if (!sheet) return;
    await call('save_character', { id:sheet.id, name:sheet.name, identity:{ ...(sheet.identity??{}), identity_prompt:sheet.identityPrompt, anatomy_prompt:sheet.anatomyPrompt, description:sheet.description ?? '', studioTraits:sheet.traits ?? '', traits:sheet.traits ?? '' } });
    note=$t("Fiche enregistrée ; la génération utilisera ces prompts.");
    await load();
    sheet=null;
  }
  async function refreshBackgrounds() {
    const roles: ('p1'|'p2')[] = config.bg?.source === 'combined' ? ['p1','p2'] : ['p1'];
    const all: {variantId:string;variantName:string;bg:string;enabled:boolean}[] = [];
    for (const role of roles) {
      const id = config[role]?.characterId; if (!id) continue;
      const data = await call<Sheet>('character_sheet', { id });
      for (const bg of data.backgrounds) all.push({...bg, enabled: !config.bg.enabledByVariant?.length || config.bg.enabledByVariant.includes(bg.variantId)});
    }
    backgroundPool = all;
    if (config.bg.source !== 'character' && config.bg.source !== 'combined') backgroundPool = [];
  }
  function toggleBg(variantId:string) {
    const disabled: string[] = config.bg.enabledByVariant ?? [];
    const allIds = [...new Set(backgroundPool.map(b=>b.variantId))];
    const next = disabled.length ? (disabled.includes(variantId) ? disabled.filter(v=>v!==variantId) : [...disabled, variantId]) : allIds.filter(v=>v!==variantId);
    config.bg.enabledByVariant = next;
    backgroundPool = backgroundPool.map(b=>({...b, enabled: !next.length || next.includes(b.variantId)}));
  }
  async function setBgSource(source:string) { config.bg.source=source; config.bg.selected=null; await refreshBackgrounds(); }
  async function chooseBg(bg:string) { config.bg.selected = config.bg.selected === bg ? null : bg; }
  async function applyActions() {
    config.action.candidates = actionText.split('\n').map(line=>line.trim()).filter(Boolean).map(line=>{
      const [text, modes] = line.split('|');
      return { text:text.trim(), modes: modes ? modes.split(',').map(m=>m.trim().toLowerCase()).filter(Boolean) : ['solo','duo'] };
    });
  }
  const compatActions = $derived((config.action?.candidates ?? []).filter((a:any)=>!a.modes?.length || a.modes.includes(mode)));
  async function mapCommand(command:string, target:string) {
    const [nodeId,inputName] = target.split('::');
    if (!nodeId || !inputName) return;
    await call('save_semantic_binding', { id: workflowId, key:command, node:nodeId, input:inputName });
    config.mappings[command] = { nodeId, inputName };
    await loadWorkflow();
    note = $t("Mapping {command} enregistré.",{command});
  }
  async function addQuick(field:Field, target:string) {
    const [nodeId,inputName] = target.split('::');
    const key = `QUICK_${config.quick.length + 1}`;
    await call('save_semantic_binding', { id:workflowId, key, node:nodeId, input:inputName });
    config.quick = [...(config.quick ?? []), { nodeId, inputName, value:field.value, visible:true, label:`${field.classType}.${field.inputName}`, group:field.section ?? 'simple' }];
    await loadWorkflow();
  }
  async function exportProfile() {
    if (!selected) return;
    const text = JSON.stringify(await call('export_prepare_profile', { id: selected.id }), null, 2);
    await navigator.clipboard?.writeText(text); note = $t("Profil exporté dans le presse-papiers.");
  }
  async function importProfile() {
    if (!workflowId || !importText.trim()) return;
    try { const profile = await call<{id:string}>('import_prepare_profile', { text: importText, workflow: workflowId }); profiles = await call('prepare_profiles'); await loadProfile(profile.id); note=$t("Profil importé."); }
    catch (e) { error = message(e); }
  }
  async function auditProfile() {
    if (!selected || !workflowId) return;
    try { audit = await call('audit_prepare_profile', { id: selected.id, workflow: workflowId }); note=$t("Audit de profil calculé."); }
    catch (e) { error = message(e); }
  }
  async function chooseLoraDir(){const picked=await invoke<string[]>('pick_directories');if(picked?.length)loraDirectory=picked[0];}
  async function loadActiveStack(){try{const p=await call<Record<string,unknown>>('preferences');activeLoraStack=Array.isArray(p.loraActiveStack)?p.loraActiveStack as any[]:[];loraDirectory=String(p.loraRoot??'')}catch{}}
  async function applyActiveStack(){if(!workflowId||!activeLoraStack.length)return;const loaders=await call<any[]>('lora_loaders',{workflow:workflowId});if(!loaders.length){loraError=$t("Aucun loader LoRA dans ce workflow");return}const loader=loaders.find(l=>l.nodeId===loraLoader);if(!loader){loraError=$t("Choisir explicitement le loader cible du stack.");return}config.lora=config.lora??{selections:[],associations:{}};config.lora.selections=activeLoraStack.map(s=>({...s,loaderNode:loader.nodeId,mode:loader.mode,enabled:s.enabled!==false}));note=$t("Stack actif appliqué : {n} LoRA.",{n:config.lora.selections.length});await previewLora()}
  async function loadLoras() {
    loraError=''; loraItems=[];
    try { const report = await call<{items:any[];root:string}>('lora_inventory', { directory: loraDirectory }); loraItems = report.items; loraRoot=String(report.root??''); if (workflowId) loraLoaders = await call<any[]>('lora_loaders', { workflow: workflowId }); if (!loraLoader && loraLoaders.length) loraLoader = loraLoaders[0].nodeId; note = $t("{n} LoRA(s) locale(s) en lecture seule.",{n:report.items.length}); setTimeout(loadLoraCovers,0); }
    catch (e) { loraError = message(e); }
  }
  const loraCategories = $derived([...new Set(loraItems.map(i=>loraCategoryOf(i)))]);
  function loraCategoryOf(i:any){const parts=String(i.relativePath??'').split('/');return parts.length>1?parts[0]:'Sans catégorie'}
  function isLoraFavorite(i:any){return (config.loraFavorites??[]).includes(i.relativePath)}
  function toggleLoraFavorite(i:any){const list=config.loraFavorites??[];config.loraFavorites=list.includes(i.relativePath)?list.filter((v:string)=>v!==i.relativePath):[...list,i.relativePath]}
  const filteredLoras = $derived(loraItems.filter(i => (!loraFavoritesOnly||isLoraFavorite(i)) && (!loraCategory||loraCategoryOf(i)===loraCategory) && `${i.name} ${i.relativePath}`.toLowerCase().includes(loraSearch.toLowerCase())).sort((a,b)=>loraSort==='size'?b.bytes-a.bytes:loraSort==='category'?loraCategoryOf(a).localeCompare(loraCategoryOf(b)):a.name.localeCompare(b.name)));
  function saveStack(){const n=stackName.trim()||('Stack '+(config.loraStacks?.length??0)+1);config.loraStacks=[...(config.loraStacks??[]).filter((x:any)=>x.name!==n),{name:n,selections:JSON.parse(JSON.stringify(config.lora?.selections??[]))}];activeStack=n;stackName='';}
  function loadStack(n:string){const st=(config.loraStacks??[]).find((x:any)=>x.name===n);if(st){config.lora=config.lora??{selections:[],associations:{}};config.lora.selections=JSON.parse(JSON.stringify(st.selections));activeStack=n;}}
  function editMeta(lora:any){loraMetaItem=lora;loraMetaText=JSON.stringify(lora.metadata??{},null,2);loraMetaError=''}
  async function saveMeta(){if(!loraMetaItem)return;try{const metadata=JSON.parse(loraMetaText);await call('lora_metadata_save',{path:loraRoot+'/'+loraMetaItem.relativePath,metadata});loraMetaError=''}catch(e){loraMetaError=message(e);return}loraMetaItem=null;await loadLoras()}
  async function loadLoraCovers(){const pending=filteredLoras.filter(i=>i.preview&&!loraCovers[i.preview]).slice(0,48);await Promise.all(pending.map(async i=>{try{const url=await call<string>('lora_cover',{path:i.preview});loraCovers[i.preview]=url;}catch{}}));}
  function addLora(item:any) {
    const loader = loraLoaders.find(l=>l.nodeId===loraLoader);
    if (!loader) { loraError = $t("Choisir un loader LoRA"); return; }
    config.lora = config.lora ?? { selections: [], associations: {} };
    config.lora.selections = [...(config.lora.selections ?? []), { name:item.name, relativePath:item.relativePath, loaderNode:loader.nodeId, mode:loader.mode, modelWeight:1, clipWeight:1, enabled:true }];
  }
  async function previewLora() {
    if (!workflowId) return;
    try { loraPreview = await call('preview_lora', { workflow: workflowId, config: config.lora ?? { selections: [] } }); note=$t("Aperçu LoRA exact, aucune écriture."); }
    catch (e) { loraError = message(e); }
  }
  function temporaryPrompt(role:'p1'|'p2',anatomy=false){
    config.promptOverrides={...(config.promptOverrides??{}),[(anatomy?'ANATOMY_':'')+role.toUpperCase()]:anatomy?roleAnatomy[role]:rolePrompts[role]};
  }
  async function runPreview() {
    if(busy)return;busy=true;error='';preview=null;
    try {
      const seed=resolveSeed(config.seedMode,String(config.masterSeed??''),lastQueued);
      const original=config.masterSeed;config.masterSeed=seed;
      try{await saveProfile();preview=await call('prepare_universal',{id:selectedProfile});}
      finally{config.masterSeed=original;}
      note=$t("Brouillon préparé. Aucune génération soumise, aucune fiche modifiée.");
    }catch(e){error=message(e)}finally{busy=false}
  }
  async function submitted(){lastQueued=await call<string|null>('last_queued_seed',{workflow:workflowId})??'';}
  function draft(){return {selectedProfile,workflowId,name,mode,config:$state.snapshot(config),rolePrompts:$state.snapshot(rolePrompts),roleAnatomy:$state.snapshot(roleAnatomy),actionText};}
  function persistDraft(value:any){draftWrite=draftWrite.then(()=>call('set_preference',{key:'generationDraft',value})).then(()=>{}).catch(e=>{error=message(e)});}
  $effect(()=>{const value=draft();if(!draftReady)return;clearTimeout(draftTimer);draftTimer=setTimeout(()=>persistDraft(value),200)});
  onDestroy(()=>{clearTimeout(draftTimer);if(draftReady)persistDraft(draft())});
  onMount(() => {
    if(native)void (async()=>{
      await load();await loadActiveStack();
      const prefs=await call<any>('preferences');const d=prefs.generationDraft;
      if(d&&workflows.some(w=>w.id===d.workflowId)){
        selectedProfile=d.selectedProfile??'';selected=profiles.find(p=>p.id===selectedProfile)??null;
        workflowId=d.workflowId;name=d.name;mode=d.mode;config={...blank(),...d.config};
        rolePrompts=d.rolePrompts??{p1:'',p2:''};roleAnatomy=d.roleAnatomy??{p1:'',p2:''};actionText=d.actionText??'';
        await loadWorkflow();await refreshBackgrounds();
      }
      draftReady=true;
    })().catch(e=>error=message(e));
    const imported=async(e:Event)=>{const d=(e as CustomEvent).detail;if(d?.workflowId){workflows=await call<{id:string;name:string}[]>('workflows');workflowId=d.workflowId;await loadWorkflow();note=$t("Workflow image importé et sélectionné.")}};
    window.addEventListener('saya-workflow-imported',imported as EventListener);
    return ()=>window.removeEventListener('saya-workflow-imported',imported as EventListener);
  });
</script>
  <style>
    .seed-toolbar{display:grid;grid-template-columns:minmax(240px,1fr) minmax(150px,230px) 2fr;gap:12px;align-items:start}.seed-actions{margin:0;align-self:center}@media(max-width:1100px){.seed-toolbar{grid-template-columns:1fr 1fr}.seed-actions{grid-column:1/-1}}
    .bg-options{display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:8px}
    .person-card.compact{min-height:120px;padding:6px;display:grid;gap:4px;text-align:start}
    .portrait-fallback{width:56px;height:56px;display:grid;place-items:center;border-radius:50%;background:var(--raised);color:var(--text);font-weight:700}
    .role-prompts{display:grid;grid-template-columns:1fr 1fr;gap:8px;margin:8px 0}
    @media(max-width:900px){.role-prompts{grid-template-columns:1fr}}
    .lora-manager{display:grid;grid-template-columns:1fr 1fr;gap:12px}
    .lora-catalog{max-height:360px;overflow:auto;display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:8px}
    .lora-card{border:1px solid var(--line);border-radius:6px;padding:8px;display:grid;gap:4px}
    .lora-card img{width:100%;aspect-ratio:3/4;object-fit:cover;border-radius:4px;background:var(--canvas)}
    .lora-selected{border-inline-start:1px solid var(--line);padding-inline-start:12px}
    .lora-selection{border:1px solid var(--line);border-radius:6px;padding:8px;display:grid;grid-template-columns:1fr auto auto auto auto;gap:6px;align-items:end;margin-bottom:8px}
    @media(max-width:900px){.lora-manager{grid-template-columns:1fr}}
    .sheet-overlay{position:fixed;inset:0;background:rgba(0,0,0,.65);display:flex;align-items:center;justify-content:center;z-index:1000;padding:24px}
    .sheet-window{width:min(760px,94vw);max-height:88vh;overflow:auto}
    .sheet-head{display:flex;justify-content:space-between;align-items:center;gap:12px}
    .role-selectors{display:flex;gap:8px;align-items:end;flex-wrap:wrap;margin:10px 0}
    .role-selectors label{min-width:220px}
    .character-picker{width:min(1400px,96vw)}
    .up-people-grid{max-height:min(64vh,560px);overflow:auto;padding-inline-end:6px;border:1px solid var(--line);border-radius:6px;grid-template-columns:repeat(auto-fill,minmax(118px,1fr))}
    .up-people-grid :global(img){height:92px;object-fit:cover;object-position:top}
    .up-people-grid :global(.image-placeholder){height:92px;min-height:92px}
    .up-people-grid :global(.person-card){min-height:0}
    .up-people-grid :global(.person-card small){font-size:10px;color:var(--muted)}
    .up-people-grid :global(.person-card strong){font-size:12px}
    details.panel>summary{cursor:pointer;font-weight:600;font-size:13px;padding:4px 0}
    details.panel[open]>summary{margin-bottom:10px}
  </style>

{#if !embedded}<div class="page-heading"><div><h1>{$t("Préparation universelle")}</h1><p>{$t("Panneau piloté par mappings, sans ouvrir les nodes. L’original n’est jamais modifié.")}</p></div><button onclick={load} disabled={busy}>{$t("Actualiser")}</button></div>{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if note}<p class="notice">{note}</p>{/if}
<section class="filters">
  <label>{$t("Profil")}<select bind:value={selectedProfile} onchange={() => selectedProfile && loadProfile(selectedProfile)}><option value="">{$t("— nouveau —")}</option>{#each profiles as p (p.id)}<option value={p.id}>{p.name}{p.changed?' · '+$t("modifié"):''}</option>{/each}</select></label>
  <label>{$t("Workflow")}<select bind:value={workflowId} onchange={loadWorkflow}><option value="">{$t("— choisir —")}</option>{#each workflows as w (w.id)}<option value={w.id}>{w.name}</option>{/each}</select></label>
  <label>{$t("Nom")}<input bind:value={name} /></label>
  <label>{$t("Mode")}<select bind:value={mode}><option value="solo">{$t("SOLO")}</option><option value="duo">{$t("DUO")}</option></select></label>
</section>
<div class="seed-toolbar">
  <label>{$t("Master seed")}<input data-up-master-seed bind:value={config.masterSeed} placeholder={$t("u64, ou -1 pour aléatoire rgthree")} /><small class="subtle">{$t("Modes résolus exactement avant l’aperçu. Last Queued vient des soumissions acceptées. L’aperçu ne l’actualise pas.")}</small></label>
  <label>{$t("Mode seed")}<select data-up-seed-mode bind:value={config.seedMode}><option value="fixed">{$t("Fixe")}</option><option value="randomize">{$t("Randomize Each Time")}</option><option value="increment">{$t("Increment")}</option><option value="decrement">{$t("Decrement")}</option><option value="last">{$t("Use Last Queued Seed")}</option></select></label>
  <div class="button-row seed-actions"><button data-up-seed-random onclick={()=>{const b=new Uint8Array(8);crypto.getRandomValues(b);let v=0n;for(const x of b)v=(v<<8n)|BigInt(x);config.masterSeed=((v % 1125899906842624n)+1n).toString();config.seedMode='fixed';}}>{$t("New Fixed Random")}</button>
  <button data-up-seed-randomize onclick={()=>{config.masterSeed='-1';config.seedMode='randomize'}}>{$t("Randomize Each Time")}</button>
  <button data-up-seed-last disabled={!lastQueued} onclick={()=>{config.masterSeed=lastQueued;config.seedMode='last'}}>{$t("Use Last Queued Seed")}</button>
  <button data-up-seed-increment disabled={!config.masterSeed} onclick={()=>{config.seedMode='increment'}}>{$t("+1 seed")}</button>
  <button data-up-seed-decrement disabled={!config.masterSeed} onclick={()=>{config.seedMode='decrement'}}>{$t("−1 seed")}</button>
  </div></div>
<div class="button-row">
  <button data-up-new onclick={newProfile}>{$t("Nouveau")}</button><button class="primary" data-up-save disabled={busy||!workflowId} onclick={async()=>{try{await saveProfile()}catch(e){error=message(e)}}}>{$t("Enregistrer")}</button>
</div>
{#if selected?.changed}<p class="notice" data-up-changed>{$t("Workflow modifié depuis ce profil : vérifier les mappings avant préparation.")}</p>{/if}

<section class="panel" data-up-characters>
  <div class="role-selectors">
    <label data-up-p1>{$t("P1")}<select value={config.p1?.characterId ?? ''} onchange={(e)=>choose('p1',e.currentTarget.value)}><option value="">{$t("— choisir —")}</option>{#each cards as c (c.id)}<option value={c.id}>{c.name}</option>{/each}</select></label>
    <button data-up-browse-p1 onclick={()=>{pickerRole='p1';favoritesOnly=false;categoryFilter=''}}>{$t("Parcourir P1")}</button>
    <button data-up-open-p1 disabled={!config.p1?.characterId} onclick={()=>openSheet('p1')}>{$t("Éditer P1")}</button>
    {#if mode==='duo'}<label data-up-p2>{$t("P2")}<select value={config.p2?.characterId ?? ''} onchange={(e)=>choose('p2',e.currentTarget.value)}><option value="">{$t("— choisir —")}</option>{#each cards as c (c.id)}<option value={c.id}>{c.name}</option>{/each}</select></label>
      <button data-up-browse-p2 onclick={()=>{pickerRole='p2';favoritesOnly=false;categoryFilter=''}}>{$t("Parcourir P2")}</button><button data-up-open-p2 disabled={!config.p2?.characterId} onclick={()=>openSheet('p2')}>{$t("Éditer P2")}</button>{/if}
  </div>
  <div class="role-prompts">
    <label>{$t("Prompt P1")}<textarea data-up-prompt-p1 rows="3" bind:value={rolePrompts.p1} oninput={()=>temporaryPrompt('p1')}></textarea></label>
    <label>{$t("P1 anatomique")}<textarea data-up-anatomy-p1 rows="3" bind:value={roleAnatomy.p1} oninput={()=>temporaryPrompt('p1',true)}></textarea></label>
    {#if mode==='duo'}<label>{$t("Prompt P2")}<textarea data-up-prompt-p2 rows="3" bind:value={rolePrompts.p2} oninput={()=>temporaryPrompt('p2')}></textarea></label>
      <label>{$t("P2 anatomique")}<textarea data-up-anatomy-p2 rows="3" bind:value={roleAnatomy.p2} oninput={()=>temporaryPrompt('p2',true)}></textarea></label>{/if}
  </div>
  {#if config.p1?.characterId}<p class="subtle">{$t("P1 : {name}",{name:roleName('p1')})}{#if mode==='duo' && config.p2?.characterId} · {$t("P2 : {name}",{name:roleName('p2')})}{/if}</p>{/if}
</section>

{#if pickerRole}
<div class="sheet-overlay" role="dialog" aria-modal="true" aria-label={$t("Choisir un personnage")}>
  <section class="panel sheet-window character-picker" data-character-picker>
    <div class="sheet-head"><h2>{$t("Choisir {role}",{role:pickerRole.toUpperCase()})}</h2><button onclick={()=>pickerRole=null}>{$t("Fermer")}</button></div>
    <div class="chip-row"><button class:primary={!favoritesOnly} onclick={()=>favoritesOnly=false}>{$t("Tous")}</button><button class:primary={favoritesOnly} onclick={()=>favoritesOnly=true}>{$t("Favoris ({n})",{n:config.favorites?.length??0})}</button><select bind:value={categoryFilter}><option value="">{$t("Toutes catégories")}</option>{#each categories as category}<option value={category}>{category}</option>{/each}</select><input placeholder={$t("Rechercher…")} bind:value={search} /></div>
    <div class="people-grid up-people-grid">{#each filtered as card (card.id)}<button class="person-card compact" data-picker-character={card.id} onclick={()=>{choose(pickerRole!,card.id);pickerRole=null}}>{#if card.portrait}<Thumbnail id={card.portrait} alt={card.name}/>{:else}<span class="portrait-fallback">{card.name.slice(0,2)}</span>{/if}<strong>{card.name}</strong><small>{$t("{img} img · {var} var.",{img:card.imageCount,var:card.variantCount})}</small></button>{/each}</div>
  </section>
</div>
{/if}
{#if sheet}
<div class="sheet-overlay" role="dialog" aria-modal="true" aria-label={$t("Fiche personnage")}>
  <section class="panel sheet-window" data-up-sheet>
    <div class="sheet-head"><h2>{$t("Fiche · {name}",{name:sheet.name})}</h2><button onclick={()=>sheet=null}>{$t("Fermer")}</button></div>
    <label>{$t("Nom")}<input data-up-sheet-name bind:value={sheet.name} /></label>
    <label>{$t("Prompt complet")}<textarea data-up-sheet-prompt rows="8" bind:value={sheet.identityPrompt}></textarea></label>
    <label>{$t("Anatomie")}<textarea data-up-sheet-anatomy rows="4" bind:value={sheet.anatomyPrompt}></textarea></label>
    <label>{$t("Description")}<input data-up-sheet-description bind:value={sheet.description} /></label>
    <label>{$t("Traits")}<input data-up-sheet-traits bind:value={sheet.traits} /></label>
    <p class="subtle">{$t("{n} BG associé(s).",{n:sheet.backgrounds.length})}</p>
    <div class="button-row"><button class="primary" data-up-sheet-save onclick={saveSheet}>{$t("Enregistrer la fiche")}</button><button data-up-sheet-p1 onclick={()=>{config.p1.characterId=sheet!.id;sheet=null;refreshBackgrounds()}}>{$t("Affecter à P1")}</button>{#if mode==='duo'}<button data-up-sheet-p2 onclick={()=>{config.p2.characterId=sheet!.id;sheet=null;refreshBackgrounds()}}>{$t("Affecter à P2")}</button>{/if}</div>
  </section>
</div>
{/if}

<details class="panel" data-up-bg>
  <summary>{$t("Décor BG · source et BG associés")}</summary>
  <label>{$t("Source")}<select data-up-bg-source bind:value={config.bg.source} onchange={()=>setBgSource(config.bg.source)}><option value="fixed">{$t("BG fixe")}</option><option value="random">{$t("BG aléatoire global")}</option><option value="character">{$t("BG du personnage")}</option>{#if mode==='duo'}<option value="combined">{$t("BG P1 + P2")}</option>{/if}</select></label>
  <label>{$t("Prompt BG")}<textarea data-up-bg-text rows="3" bind:value={config.bg.text}></textarea></label>
  {#if config.bg.source==='random'}<label>{$t("BG candidats (un par ligne)")}<textarea data-up-bg-candidates value={(config.bg.candidates ?? []).join('\n')} oninput={(e)=>{config.bg.candidates=e.currentTarget.value.split('\n').map(v=>v.trim()).filter(Boolean)}}></textarea></label>{/if}
  {#if backgroundPool.length}
    <p data-up-bg-count>{$t("{n} BG associé(s) — aucun maximum.",{n:backgroundPool.length})}</p>
    <div class="chip-row">{#each backgroundPool as b (b.variantId)}<button data-up-bg={b.variantId} class:primary={b.enabled} onclick={()=>toggleBg(b.variantId)}>{b.enabled?'✓':'○'} {b.variantName} · {b.bg}</button>{/each}</div>
    <label>{$t("BG retenu")}<select data-up-bg-selected bind:value={config.bg.selected}><option value={null}>{$t("— tirage aléatoire —")}</option>{#each backgroundPool.filter(b=>b.enabled) as b}<option value={b.bg}>{b.variantName} · {b.bg}</option>{/each}</select></label>
  {/if}
  {#if (config.quick ?? []).some((q:any)=>q.bgOption)}<h3>{$t("Options du décor")}</h3><div class="bg-options">{#each (config.quick ?? []).filter((q:any)=>q.bgOption) as q, i}<label>{q.label}<input data-up-bg-option={q.inputName} bind:value={q.value} /></label>{/each}</div>{/if}
</details>

<details class="panel" data-up-action>
  <summary>{$t("Action · SOLO / DUO")}</summary>
  <label>{$t("Source")}<select data-up-action-source bind:value={config.action.source}><option value="fixed">{$t("Fixe")}</option><option value="random">{$t("Tirage aléatoire")}</option><option value="custom">{$t("Liste autorisée")}</option></select></label>
  <label>{$t("Prompt ACTION")}<textarea data-up-action-text rows="3" bind:value={config.action.text}></textarea></label>
  {#if config.action.source!=='fixed'}<label>{$t("Actions (texte | SOLO,DUO)")}<textarea data-up-action-candidates bind:value={actionText} oninput={applyActions}></textarea></label>
    <p>{$t("{n} action(s) compatible(s)",{n:compatActions.length})}</p>{/if}
</details>

<details class="panel" data-up-prompts>
  <summary>{$t("Qualité, additionnel et négatif")}</summary>
  {#if config.mappings?.NEG}<label>{$t("Négatif")}<textarea data-up-negative rows="3" value={config.negative??''} oninput={e=>config.negative=e.currentTarget.value}></textarea></label>{:else}<p class="subtle">{$t("Négatif : sélectionner sa cible réelle dans les mappings pour l’éditer.")}</p>{/if}
  <p class="subtle">{$t("POSE : aucune entrée distincte confirmée. ACTION reste indépendante.")}</p>
  <label>{$t("Qualité")}<input data-up-quality bind:value={config.quality} placeholder={$t("masterpiece, best quality…")} /></label>
  <label>{$t("Additionnel")}<input data-up-additional bind:value={config.additional} placeholder={$t("complément explicite")} /></label>
</details>

<details class="panel" data-up-mappings>
  <summary>{$t("Mappings universels")}</summary>
  <p>{$t("Chaque commande cible une entrée existante. Les mappings existants ne sont pas supprimés.")}</p>
  {#each commands as [key,label] (key)}
    {@const current = config.mappings?.[key]}
    <label>{$t(label)}<select data-up-map={key} onchange={(e)=>mapCommand(key,e.currentTarget.value)}><option value="">{current?$t("— non mappé ({target}) —",{target:`${current.nodeId}.${current.inputName}`}):$t("— non mappé —")}</option>{#each workflow?.fields ?? [] as f}<option value={`${f.nodeId}::${f.inputName}`}>{f.classType}#{f.nodeId}.{f.inputName} · {f.valueType}</option>{/each}</select></label>
  {/each}
</details>

<details class="panel" data-up-quick>
  <summary>{$t("Paramètres rapides")}</summary>
  <p>{$t("Contrôles choisis explicitement ; les autres entrées restent inchangées.")}</p>
  {#each (workflow?.fields ?? []).filter(f=>!f.enabled).slice(0,40) as f (f.nodeId+'.'+f.inputName)}
    <div class="button-row"><code>{f.classType}#{f.nodeId}.{f.inputName}</code><button data-up-quick-add={f.nodeId+'.'+f.inputName} onclick={()=>addQuick(f,`${f.nodeId}::${f.inputName}`)}>{$t("Ajouter")}</button></div>
  {/each}
  {#each config.quick ?? [] as q, index}
    <label>{q.label ?? `${q.nodeId}.${q.inputName}`} <input data-up-quick-value={index} bind:value={q.value} /> <input type="checkbox" bind:checked={q.visible}/> {$t("visible")}</label>
  {/each}
</details>

<details class="panel" data-up-lora>
  <summary>{$t("LoRA Manager")}</summary>
  <p class="subtle">{$t("Catalogue local en lecture seule. Sélectionnez un loader, filtrez, puis ajoutez les LoRA avec leurs poids modèle/CLIP.")}</p>
  <div class="chip-row"><input data-up-lora-dir bind:value={loraDirectory} placeholder={$t("/chemin/vers/loras")} /><button data-up-lora-dir-pick onclick={chooseLoraDir}>{$t("Dossier…")}</button><button data-up-lora-load onclick={loadLoras} disabled={!loraDirectory.trim()}>{$t("Inventorier")}</button><select data-up-lora-loader bind:value={loraLoader}><option value="">{$t("— loader —")}</option>{#each loraLoaders as l (l.nodeId)}<option value={l.nodeId}>{l.nodeId} · {l.mode}</option>{/each}</select><input data-up-lora-search bind:value={loraSearch} oninput={()=>loadLoraCovers()} placeholder={$t("Rechercher…")} /></div>
  <div class="chip-row"><select data-up-lora-category bind:value={loraCategory}><option value="">{$t("Toutes catégories")}</option>{#each loraCategories as c}<option value={c}>{$t(c)}</option>{/each}</select><select data-up-lora-sort bind:value={loraSort}><option value="name">{$t("Tri A–Z")}</option><option value="size">{$t("Taille")}</option><option value="category">{$t("Catégorie")}</option></select><button class:primary={loraFavoritesOnly} onclick={()=>{loraFavoritesOnly=!loraFavoritesOnly;loadLoraCovers()}}>{$t("Favoris")}</button></div>
  {#if loraError}<p class="error">{loraError}</p>{/if}
  <div class="chip-row"><input bind:value={stackName} placeholder={$t("Nom du stack")} /><button onclick={saveStack}>{$t("Enregistrer le stack")}</button>{#each config.loraStacks ?? [] as st (st.name)}<button class:primary={activeStack===st.name} onclick={()=>loadStack(st.name)}>{st.name} ({st.selections.length})</button>{/each}</div>
  <div class="lora-manager">
    <div class="lora-catalog">{#each filteredLoras.slice(0,loraLimit) as lora (lora.relativePath)}<article class="lora-card" data-up-lora-item={lora.relativePath}>{#if loraCovers[lora.preview]}<img src={loraCovers[lora.preview]} alt={lora.name} loading="lazy" />{/if}<strong>{lora.name}</strong><small>{$t(loraCategoryOf(lora))} · {Math.round(lora.bytes/1024)} Ko</small>{#if lora.metadata?.tags}<small>{Array.isArray(lora.metadata.tags)?lora.metadata.tags.join(', '):lora.metadata.tags}</small>{/if}<div class="button-row"><button data-up-lora-add={lora.relativePath} onclick={()=>addLora(lora)}>{$t("Ajouter")}</button><button data-up-lora-fav={lora.relativePath} onclick={()=>toggleLoraFavorite(lora)}>{isLoraFavorite(lora)?'★':'☆'}</button><button data-up-lora-meta={lora.relativePath} onclick={()=>editMeta(lora)}>{$t("Métadonnées")}</button></div></article>{/each}</div>{#if filteredLoras.length>loraLimit}<button onclick={()=>{loraLimit+=48;loadLoraCovers()}}>{$t("Charger plus ({n})",{n:filteredLoras.length-loraLimit})}</button>{/if}
    <div class="lora-selected"><h3>{$t("Sélection")}</h3>
      {#each config.lora?.selections ?? [] as sel, i (sel.relativePath+i)}
        <div class="lora-selection"><strong>{sel.name}</strong><label>{$t("Modèle")}<input type="number" step="0.05" min="-2" max="2" bind:value={sel.modelWeight} /></label><label>{$t("CLIP")}<input type="number" step="0.05" min="-2" max="2" bind:value={sel.clipWeight} /></label><label><input type="checkbox" bind:checked={sel.enabled}/> {$t("actif")}</label><button onclick={()=>{config.lora.selections.splice(i,1)}}>{$t("Retirer")}</button></div>
      {:else}<p class="subtle">{$t("Aucune LoRA sélectionnée.")}</p>{/each}
      <button data-up-lora-preview onclick={previewLora} disabled={!workflowId}>{$t("Aperçu API")}</button>{#if activeLoraStack.length}<button class="primary" data-up-apply-stack onclick={applyActiveStack}>{$t("Appliquer le stack actif ({n})",{n:activeLoraStack.length})}</button>{/if}
      {#if loraPreview}<p class="subtle">{$t("{n} cible(s) LoRA vérifiée(s).",{n:loraPreview.edits?.length ?? 0})}</p>{/if}
    </div>
  </div>
  {#if loraMetaItem}<div class="sheet-overlay" role="dialog" aria-modal="true" aria-label={$t("Métadonnées LoRA")}><section class="panel sheet-window"><div class="sheet-head"><h2>{loraMetaItem.name}</h2><button onclick={()=>loraMetaItem=null}>{$t("Fermer")}</button></div><p class="subtle">{$t("Fichier Saya")} <code>{$t(".metadata.json")}</code> {$t("à côté du modèle ; le fichier LoRA et le sidecar existant ne sont pas modifiés.")}</p><textarea rows="14" bind:value={loraMetaText}></textarea>{#if loraMetaError}<p class="error">{loraMetaError}</p>{/if}<div class="button-row"><button class="primary" onclick={saveMeta}>{$t("Enregistrer les métadonnées")}</button></div></section></div>{/if}
</details>

<div class="button-row"><button class="primary" data-up-preview disabled={busy||!workflowId} onclick={runPreview}>{$t("Préparer l’aperçu API")}</button><button data-up-export disabled={!selected} onclick={exportProfile}>{$t("Exporter le profil")}</button><button data-up-audit disabled={!selected||!workflowId} onclick={auditProfile}>{$t("Auditer / remapper")}</button></div>
<details><summary>{$t("Importer un profil")}</summary><textarea data-up-import bind:value={importText} placeholder={$t("JSON saya-prepare-profile")}></textarea><button data-up-import-apply disabled={!workflowId||!importText.trim()} onclick={importProfile}>{$t("Importer sur ce workflow")}</button></details>
{#if audit}<section class="panel" data-up-audit-result><h2>{$t("Audit de changements")}</h2><p data-up-audit-changed>{audit.changed?$t("Workflow modifié"):$t("Signature identique")} · {$t("+{added} / -{removed} node(s)",{added:audit.addedNodes?.length??0,removed:audit.removedNodes?.length??0})}</p>{#each Object.entries(audit.mappings ?? {}) as [command,state]}<p><strong>{command}</strong> : {(state as any).status}{#if (state as any).suggestions?.length} · {$t("{n} suggestion(s) à confirmer",{n:(state as any).suggestions.length})}{/if}</p>{/each}</section>{/if}
{#if preview}
<section class="panel" data-up-preview-result>
  <h2>{$t("Aperçu de préparation")}</h2>
  <p data-up-resolution>{$t("Master seed")} <code>{preview.resolution?.masterSeed ?? '—'}</code> {$t("· BG :")} <strong>{preview.resolution?.bg}</strong> {$t("· Action :")} <strong>{preview.resolution?.action}</strong> · {$t("P1 : {name}",{name:preview.resolution?.p1Name})}</p>
  <p>{$t("{n} entrée(s) modifiée(s) · brouillon {id}",{n:preview.prepared?.changes?.length ?? 0,id:preview.prepared?.jobId})}</p>
  <ul>{#each preview.prepared?.changes ?? [] as c}<li><code>{c.node}.{c.input}</code> : {c.before} → {c.after}</li>{/each}</ul>
  <details><summary>{$t("Requête API exacte")}</summary><pre>{preview.prepared?.requestJson}</pre></details>
  <PreparedDraft prepared={preview.prepared as PreparedJob} onsubmitted={submitted} />
</section>
{/if}
