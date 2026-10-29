<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import { onMount, onDestroy, type Snippet } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import {getCurrentWebview} from '@tauri-apps/api/webview';
  import { call, message, type CharacterCard } from '../lib/api';
  import type { GraphView } from '../lib/graph/view';
  import { promptNodes, promptLabel, modeNode, nodePass, nodeLabel, widgetChoices, generationTechnical, widgetLinked, widgetSpec, routeHomes, loraChoices, parseLoraSyntax, formatLoraSyntax } from '../lib/generation-workspace';
  import LoraPicker from './LoraPicker.svelte';
  import { randomSeed, resolveSeed } from '../lib/generation-state';
  import SayaCouple from './SayaCouple.svelte';
  import SamplerComparators from './SamplerComparators.svelte';
  import Icon from './Icon.svelte';
  import {missingModel} from '../lib/model-inputs';
  let { active = true, expert = $bindable(false), imagePath = null, comparisonJob = null, oncloseComparison = () => {}, tools }: { active?:boolean; expert?: boolean; imagePath?:string|null; comparisonJob?: string|null; oncloseComparison?: () => void; tools?: Snippet } = $props();
  let session=$state(''), sourcePath=$state(''), view=$state<GraphView|null>(null), seed=$state('');
  let randomEach=$state(false), lastSeed=$state('');
  let name=$state('Ma préparation'), saved=$state<any[]>([]), cards=$state<CharacterCard[]>([]), p1=$state(''),p2=$state('');
  let scopeView=$state<GraphView|null>(null),scopePath=$state<string[]>([]),scopeDrafts=$state<Record<string,{path:string[];node:string;input:string;value:string}>>({});
  type Group={path:string[];chain:string[];view:GraphView;label:string};
  type Card={g:Group;node:string;inputs:string[];lead:boolean};
  let groups=$state<Group[]>([]);
  const passCache=new Map<number,Group[]>();
  let cacheView:GraphView|null=null;
  let tab=$state('preview'),promptTab=$state(''),lorasOpen=$state(false), pass=$state(0), loraNames=$state<string[]>([]), activeStack=$state<any[]>([]);
  let busy=$state(false),error=$state(''),note=$state(''),preview=$state<any>(null),showApi=$state(false),settings=$state(false),selectedNode=$state('');
  let catalog=$state<any[]>([]),drafts=$state<Record<string,string>>({}),loraLabels=$state<Record<string,{label:string;base:string}>>({});
  let alive=true, pending=Promise.resolve(), initialization=Promise.resolve();
  let temporary=$state(false),launchToken='';
  let currentRun=$state<any>(null);
  type MissingModel={path:string[];node:string;input:string;label:string;original:string;replacement:string;options:string[]};
  let missingModels=$state<MissingModel[]>([]),modelDialog=$state(false);
  let settleModels:((accepted:boolean)=>void)|null=null;
  function finishModels(accepted:boolean){modelDialog=false;settleModels?.(accepted);settleModels=null}
  const showModelDialog=(node:HTMLDialogElement)=>{node.showModal();return{destroy:()=>node.close()}};
  async function checkModels(){
    const live=await call<{nodes:any[]}>('node_catalog');catalog=live.nodes;
    const found:MissingModel[]=[];
    async function scan(v:GraphView,path:string[],depth:number){
      if(depth>8)throw Error($t("Sous-graphes trop imbriqués pour vérifier les modèles."));
      for(const n of v.document.nodes){
        for(const input of fields(v,n.id)){
          const value=path.length?(scopeDrafts[JSON.stringify([...path,n.id,input])]?.value??v.paramText[n.id]?.[input]??''):text(n.id,input);
          const schema=catalog.find(c=>c.name===n.classType)?.inputs.find((i:any)=>i.name===input)?.type;
          const options=widgetChoices(v,catalog,n.id,input)??(Array.isArray(schema)?schema.map(String):null);
          if(missingModel(input,value,options))found.push({path,node:n.id,input,label:`${n.title||n.classType} · ${input}`,original:value,replacement:'',options:options!});
        }
      }
      for(const group of containers(v)){const child=await call<{view:GraphView}>('graph_scoped_widget',{session,path:[...path,group.id],edit:null});await scan(child.view,[...path,group.id],depth+1)}
    }
    if(view)await scan(view,[],0);
    if(!found.length)return;
    missingModels=found;modelDialog=true;
    const accepted=await new Promise<boolean>(resolve=>settleModels=resolve);
    if(!accepted)throw Error($t("Lancement annulé : aucun modèle remplacé."));
    for(const item of missingModels){
      if(!item.options.includes(item.replacement))throw Error($t("Choisis un modèle disponible pour chaque champ."));
      if(!item.path.length)edit(item.node,item.input,item.replacement);
      else scopeDrafts={...scopeDrafts,[JSON.stringify([...item.path,item.node,item.input])]:{path:item.path,node:item.node,input:item.input,value:item.replacement}};
    }
  }
  let suspended:any=null,unlisten:(()=>void)|undefined;
  async function openImage(path:string){
    await initialization;if(!alive)return;
    await task(async()=>{
      if(!temporary){await remember();suspended={...preference(),view,tab,pass,groups,scopeView,scopePath,preview,settings}}
      const previous=temporary?session:null;
      const result=await call<{session:string;view:GraphView}>('image_temporary_open',{path});
      if(!alive){await call('image_temporary_close',{session:result.session});return}
      if(previous)await call('image_temporary_close',{session:previous});
      temporary=true;launchToken='';session=result.session;view=result.view;sourcePath=path;name=path.split('/').pop()??'Image';
      drafts={};scopeDrafts={};scopeView=null;scopePath=[];groups=[];pass=0;p1='';p2='';randomEach=false;lastSeed='';preview=null;settings=false;
      seed=Object.entries(view.paramText).find(([id])=>view?.document.nodes.some(n=>n.id===id&&n.classType==='Seed (rgthree)'))?.[1]?.seed??'';
      tab=promptNodes(view).length?'preview':'nodes';if(tab==='nodes')groups=[{path:[],chain:[],view,label:$t("Réglages du workflow")}];
      note=$t("Workflow temporaire");
    });
  }
  async function closeImage(){await task(async()=>{
    await call('image_temporary_close',{session});temporary=false;
    if(suspended){({session,sourcePath,seed,name,p1,p2,drafts,randomEach,lastSeed,scopeDrafts,view,tab,pass,groups,scopeView,scopePath,preview,settings}=suspended);suspended=null}
    else {session='';view=null;sourcePath='';drafts={};scopeDrafts={};groups=[]}
    note='';
  })}
  async function launchImage(){
    await pending;await checkModels();seed=resolveSeed(randomEach?'randomize':'fixed',seed,lastSeed);await flush();
    launchToken ||= crypto.randomUUID();
    const result=await call<{jobId?:string;run?:any}>('image_temporary_submit',{session,launch:launchToken,master_seed:seed||null,edits:[],confirmed:true});
    launchToken='';
    return result.run??{id:result.jobId, singleJob:true, phaseCount:1, state:'running', phases:{'1':{job:result.jobId,state:'submitted'}}};
  }

  const preset=$derived.by(()=>{try{return !!JSON.parse(view?.sourceDetails??'{}').extra?.saya_preset}catch{return false}});
  const prompts=$derived(promptNodes(view));
  const mode=$derived(modeNode(view));
  const solo=$derived(mode ? text(mode.id,'Couple Mode')==='OFF' : false);
  const key=(id:string,input:string)=>`${id}\u0000${input}`;
  function text(id:string,input:string){return drafts[key(id,input)]??view?.paramText[id]?.[input]??''}
  const inputNames=(id:string)=>Object.keys(view?.paramText[id]??{}).filter(input=>!view?.document.connections.some(c=>c.to.node===id&&c.to.input===input));
  function choices(id:string,input:string){return widgetChoices(view,catalog,id,input)}
  function type(id:string,input:string){return view?.document.nodes.find(n=>n.id===id)?.inputs.find(i=>i.name===input)?.type}
  function edit(id:string,input:string,value:string){drafts={...drafts,[key(id,input)]:value};preview=null;note=$t("Modifications dans la copie de travail.")}
  function scopeKey(id:string,input:string){return JSON.stringify([...scopePath,id,input])}
  function scopeText(id:string,input:string){return scopeDrafts[scopeKey(id,input)]?.value??scopeView?.paramText[id]?.[input]??''}
  function scopeEdit(id:string,input:string,value:string){scopeDrafts={...scopeDrafts,[scopeKey(id,input)]:{path:[...scopePath],node:id,input,value}};preview=null}
  function containers(v:GraphView|null){return v?.document.subgraphs??[]}
  function fields(v:GraphView,id:string){return Object.keys(v.paramText[id]??{}).filter(input=>!widgetLinked(v,id,input))}
  async function loadPass(n:number){await task(async()=>{pass=n;groups=[];if(!view)return;if(cacheView!==view){passCache.clear();cacheView=view}const cached=passCache.get(n);if(cached){groups=cached;return}const found:Group[]=[];
    async function walk(v:GraphView,path:string[],chain:string[],label:string,depth:number){if(depth>8)return;found.push({path,chain,view:v,label});
      for(const c of containers(v)){const node=v.document.nodes.find(node=>c.nodeIds.includes(node.id));if(!node||(!path.length&&nodePass(v,node.id)!==n))continue;
        const r=await call<{view:GraphView}>('graph_scoped_widget',{session,path:[...path,c.id],edit:null});await walk(r.view,[...path,c.id],[...chain,node.id],nodeLabel(v,node.id),depth+1);
      }
    }
    await walk(view,[],[],n?$t("Passe {n} · sorties",{n}):$t("Réglages communs"),0);
    // Publish the pass once complete so routed settings never land in a parent section first.
    passCache.set(n,found);groups=found;
  })}
  function bypassed(g:Group,id:string){try{return JSON.parse(g.view.sourceDetails||'{}').nodes?.find((n:any)=>String(n.id)===id)?.mode===4}catch{return false}}
  async function toggleBypass(g:Group,id:string){await task(async()=>{await flush();const op={op:'set_bypass',node:id,bypass:!bypassed(g,id)};if(g.path.length){const r=await call<{root:GraphView;view:GraphView}>('graph_scoped_widget',{session,path:g.path,edit:op});view=r.root;g.view=r.view}else{view=await call<GraphView>('graph_edit',{session,op});g.view=view}groups=[...groups];passCache.clear();await remember();note=op.bypass?$t("Node bypassé dans la copie de travail."):$t("Node réactivé.")})}
  function groupText(g:{path:string[];view:GraphView},id:string,input:string){return g.path.length?scopeDrafts[JSON.stringify([...g.path,id,input])]?.value??g.view.paramText[id]?.[input]??'':text(id,input)}
  function groupEdit(g:{path:string[];view:GraphView},id:string,input:string,value:string){if(!g.path.length){edit(id,input,value);return}const k=JSON.stringify([...g.path,id,input]);scopeDrafts={...scopeDrafts,[k]:{path:g.path,node:id,input,value}};preview=null}

  // Present the workflow by treatment: subgraph sections, then the workflow's own LiteGraph groups inside them.
  // Root routing widgets move to the section that really consumes them (links, subgraph ports, Set/Get).
  function shown(n:{id:string;classType:string;title?:string|null}){return !generationTechnical(n.classType,n.title??'')&&!prompts.some(p=>p.id===n.id)&&n.classType!=='Lora Loader (LoraManager)'&&n.classType!=='Seed (rgthree)'&&n.id!==mode?.id}
  // A node drawn outside every workflow group belongs to the group of the node it feeds (e.g. SAM loader → its detailer).
  function groupOf(v:GraphView,id:string,depth=0):GraphView['document']['groups'][number]|undefined{
    const own=v.document.groups.find(x=>x.nodeIds.includes(id));if(own||depth>4)return own;
    for(const c of v.document.connections)if(c.from.node===id){const lg=groupOf(v,c.to.node,depth+1);if(lg)return lg}
  }
  const sections=$derived.by(()=>{
    const order=pass?[...groups.filter(g=>g.path.length),...groups.filter(g=>!g.path.length)]:groups,root=groups.find(g=>!g.path.length);
    const map=new Map<string,{key:string;title:string;rank:number[];cards:Card[]}>();
    const key=(chain:string[])=>chain.join('/')||'root';
    const rankOf=(chain:string[])=>{const i=order.findIndex(g=>key(g.chain)===key(chain));return i<0?order.length:i};
    function put(k:string,title:string,rank:number[],g:Group,node:string,inputs:string[],lead=false){
      if(!map.has(k))map.set(k,{key:k,title,rank,cards:[]});const sec=map.get(k)!,card=sec.cards.find(c=>c.node===node&&c.g===g);
      if(card)card.inputs=[...new Set([...card.inputs,...inputs])];else sec.cards.push({g,node,inputs,lead});
    }
    for(const g of order){
      const inside=new Set(containers(g.view).flatMap(c=>c.nodeIds));
      for(const n of g.view.document.nodes){
        if(!shown(n))continue;const inputs=fields(g.view,n.id);if(!inputs.length)continue;
        if(g.path.length){
          if(inside.has(n.id)){const c=[...g.chain,n.id];put(key(c),nodeLabel(g.view,n.id),[rankOf(c),0],g,n.id,inputs,true);continue}
          const lg=groupOf(g.view,n.id),gi=lg?g.view.document.groups.indexOf(lg)+1:0;
          put(lg?`${key(g.chain)}#${lg.id}`:key(g.chain),lg?lg.title:g.label,[rankOf(g.chain),gi],g,n.id,inputs);continue;
        }
        const own=nodePass(g.view,n.id);
        if(own){if(own!==pass)continue;const c=inside.has(n.id)?[n.id]:[];put(key(c),c.length?nodeLabel(g.view,n.id):g.label,[c.length?rankOf(c):order.length,0],g,n.id,inputs,true);continue}
        const routed=new Map(inputs.map(i=>[i,routeHomes(view,n.id,i)]));
        const common=inputs.filter(i=>!routed.get(i)?.length);
        if(!pass){if(common.length)put('root',g.label,[-1,0],g,n.id,common,inside.has(n.id));continue}
        for(const [input,homes] of routed)for(const home of homes){if(nodePass(view,home[0])!==pass)continue;
          const best=order.filter(x=>x.path.length&&key(home).startsWith(key(x.chain))&&(key(home)===key(x.chain)||key(home).startsWith(key(x.chain)+'/'))).sort((a,b)=>b.chain.length-a.chain.length)[0];
          // A hub widget named like a workflow group ("Face" → "Detailer · Face") sits inside that group.
          const lgs=best?.view.document.groups??[],lg=lgs.find(x=>{const tail=x.title.split(' · ').slice(1).join(' · ');return !!tail&&(input===tail||input.startsWith(tail+' ·'))});
          if(best&&lg)put(`${key(best.chain)}#${lg.id}`,lg.title,[rankOf(best.chain),lgs.indexOf(lg)+1],g,n.id,[input],true);
          else put(best?key(best.chain):'root',best?best.label:(root?.label??''),[best?rankOf(best.chain):order.length,0],g,n.id,[input],true)}
      }
    }
    return [...map.values()].filter(s=>!/^Prompt ·/.test(s.title)).sort((a,b)=>a.rank[0]-b.rank[0]||a.rank[1]-b.rank[1])
      .map(s=>({...s,cards:[...s.cards].sort((a,b)=>Number(b.lead)-Number(a.lead)||Number(!/Checkpoint|Model|Loader/.test(nodeLabel(a.g.view,a.node)))-Number(!/Checkpoint|Model|Loader/.test(nodeLabel(b.g.view,b.node))))}));
  });

  function preference(){return {session,sourcePath,seed,name,p1,p2,drafts,randomEach,lastSeed,scopeDrafts}}
  async function remember(){if(temporary)return;await call('set_preference',{key:'generationWorkspace',value:preference()})}
  async function flush(){
    const master=view?.document.nodes.find(n=>n.classType==='Seed (rgthree)');
    if(master&&/^\d+$/.test(seed.trim())&&view?.paramText[master.id]?.seed!==seed.trim())edit(master.id,'seed',resolveSeed('fixed',seed,''));
    const edits=Object.entries(drafts), sid=session;
    for(const [k,value] of edits){const [node,input]=k.split('\u0000');view=await call<GraphView>('graph_edit',{session:sid,op:{op:'set_param',node,input,value}});if(drafts[k]===value){const copy={...drafts};delete copy[k];drafts=copy}}
    for(const [k,edit] of Object.entries(scopeDrafts)){const r=await call<{root:GraphView;view:GraphView}>('graph_scoped_widget',{session:sid,path:edit.path,edit:{op:'set_param',node:edit.node,input:edit.input,value:edit.value}});view=r.root;groups=groups.map(g=>JSON.stringify(g.path)===JSON.stringify(edit.path)?{...g,view:r.view}:g);if(JSON.stringify(edit.path)===JSON.stringify(scopePath))scopeView=r.view;const copy={...scopeDrafts};delete copy[k];scopeDrafts=copy}
    await remember();
  }
  async function task(fn:()=>Promise<void>){if(busy)return;busy=true;error='';try{await pending;await fn()}catch(e){error=message(e)}finally{busy=false}}
  async function openFile(pick=true){await task(async()=>{
    const path=pick?await invoke<string|null>('pick_workflow_file'):sourcePath;if(!path)return;
    if(session)await flush();
    const result=await call<{session:string;view:GraphView}>('graph_open_file',{path});session=result.session;view=result.view;sourcePath=path;drafts={};scopeDrafts={};scopeView=null;scopePath=[];groups=[];tab='preview';p1='';p2='';randomEach=false;lastSeed='';seed=Object.entries(result.view.paramText).find(([id])=>result.view.document.nodes.some(n=>n.id===id&&n.classType==='Seed (rgthree)'))?.[1]?.seed??'';preview=null;name=path.split('/').pop()?.replace(/\.json$/,'')??'Préparation';selectedNode=prompts[0]?.id??'';await expandPreset();await remember();note=$t("Workflow ouvert · copie de travail.");void receiveLoras().catch(e=>error=message(e));
  })}
  async function expandPreset(){
    let source:any;try{source=JSON.parse(view?.sourceDetails??'{}')}catch{return}
    if(!source.extra?.saya_split)return;
    await flush();
    view=await call<GraphView>('generation_preset',{session,source_path:sourcePath});
    groups=[];scopeView=null;scopePath=[];await remember();
  }
  async function save(){await task(async()=>{await flush();const entry={session,sourcePath,seed,name,p1,p2,randomEach,lastSeed};saved=[entry,...saved.filter(s=>s.session!==session)];await call('set_preference',{key:'generationPreparations',value:saved});await remember();note=$t("Préparation enregistrée. Le fichier source reste intact.")})}
  async function resume(id:string){await task(async()=>{if(session)await flush();const savedEntry=saved.find(s=>s.session===id);if(!savedEntry)return;const r=await call<{session:string;view:GraphView}>('graph_resume',{id});session=r.session;view=r.view;({sourcePath,seed,name,p1,p2}=savedEntry);randomEach=savedEntry.randomEach??false;lastSeed=savedEntry.lastSeed??'';drafts={};scopeDrafts={};scopeView=null;scopePath=[];preview=null;groups=[];await expandPreset();await remember();note=$t("Préparation chargée.");void receiveLoras().catch(e=>error=message(e))})}
  async function choose(role:1|2,id:string){await task(async()=>{
    if(!id){if(role===1)p1='';else p2='';await remember();return}
    const identity=prompts.find(n=>new RegExp(`Person ${role}$`,'i').test(n.title??''));const anatomy=prompts.find(n=>new RegExp(`Person ${role}.*Anatomy`,'i').test(n.title??''));
    if(!identity||!anatomy)throw Error($t("Les nodes identité et anatomie du personnage {role} ne sont pas identifiés dans ce workflow.",{role}));
    const sheet=await call<any>('character_sheet',{id});edit(identity.id,'value',sheet.identityPrompt??'');edit(anatomy.id,'value',sheet.anatomyPrompt??'');if(role===1)p1=id;else p2=id;await remember();
  })}
  async function inspect(){await task(async()=>{await flush();preview=await call('graph_prepare_api',{session});showApi=true;tab='api'})}
  async function prepareSource(){
    busy=true;error='';try{await pending;await checkModels();seed=resolveSeed(randomEach?'randomize':'fixed',seed,lastSeed);await flush();await remember();
      // Existing UI exporter preserves subgraphs and exact u64 values in Rust.
      const exported=await call<{path:string}>('graph_save_ui',{session,name:name||'Préparation'});return {path:exported.path,seed};
    }catch(e){error=message(e);throw e}finally{busy=false}
  }
  async function pickLoraRoot(){await task(async()=>{const paths=await invoke<string[]>('pick_directories');if(!paths?.length)return;const r=await call<any>('lora_inventory',{directory:paths[0]});loraNames=r.items.map((i:any)=>i.relativePath.replace(/\.safetensors$/i,''));await call('set_preference',{key:'loraRoot',value:r.root});note=$t("{n} LoRA locales disponibles.",{n:loraNames.length})})}
  function loras(id:string):any[]{try{return JSON.parse(text(id,'loras')).__value__??[]}catch{return []}}
  function setLoras(id:string,entries:any[]){edit(id,'loras',JSON.stringify({__value__:entries}))}
  function editLora(id:string,index:number,field:string,value:any){setLoras(id,loras(id).map((l,i)=>i===index?{...l,[field]:value}:l))}
  const loraList=$derived(loraChoices(catalog)??(loraNames.length?loraNames:null));
  const loaders=$derived((view?.document.nodes??[]).filter(n=>n.classType==='Lora Loader (LoraManager)'&&inputNames(n.id).includes('loras')));
  function addLora(id:string,name:string){setLoras(id,[...loras(id),{name,strength:1,clipStrength:1,active:true,expanded:false,locked:false,selected:false}])}
  // The LoRA page sends picks to a named slot through preferences; Generation applies them to its working copy.
  let slotsSent='';
  $effect(()=>{const value={session,slots:loaders.map(n=>({id:n.id,title:n.title||'LoRA'}))},json=JSON.stringify(value);if(!session||temporary||json===slotsSent)return;slotsSent=json;void call('set_preference',{key:'generationLoraSlots',value}).catch(()=>{})});
  async function receiveLoras(){
    const inbox:any[]=(await call<any>('preferences')).generationLoraInbox??[];if(!inbox.length||!view)return;
    const mine=inbox.filter(i=>i.session===session&&loaders.some(n=>n.id===i.slot));
    for(const i of mine)setLoras(i.slot,[...loras(i.slot),{name:i.name,strength:i.modelWeight??1,clipStrength:i.clipWeight??i.modelWeight??1,active:true,expanded:false,locked:false,selected:false}]);
    await call('set_preference',{key:'generationLoraInbox',value:inbox.filter(i=>!mine.includes(i))});
    if(mine.length){lorasOpen=true;note=$t("{n} LoRA reçue(s) de la page LoRA · copie de travail.",{n:mine.length})}
  }
  function syntaxEditable(text:string){return !text.trim()||formatLoraSyntax(parseLoraSyntax(text))===text.trim().replace(/\s+/g,' ')}

  // 1-to-1 casting (U1) and its rotations (U2) have lived since V0.1 in their own
  // menu (src/components/Casting.svelte): Generation is back to 100% normal
  // prompting + workflow config. The Casting menu consumes the preparation from here
  // (generationWorkspace preference: session + drafts, graph_save_ui export).

  function numeric(g:Group,id:string,input:string){
    const spec=widgetSpec(g.view,catalog,id,input),kind=spec?.type??g.view.document.nodes.find(n=>n.id===id)?.inputs.find(i=>i.name===input)?.type;
    if((kind!=='INT'&&kind!=='FLOAT')||/seed/i.test(input)||groupText(g,id,input).length>15)return null;
    return {min:spec?.config?.min,max:spec?.config?.max,step:spec?.config?.step??(kind==='INT'?1:0.01)};
  }
  // Free-text prompt fields (schema multiline) sit with the prompt, not among the selectors.
  function promptTexts(id:string){return inputNames(id).filter(i=>i!=='decor_libre'&&(i==='prefixe'||i==='complement'||widgetSpec(view,catalog,id,i)?.config?.multiline===true))}
  const textLabel=(input:string)=>$t(({prefixe:'Qualité · préfixe',complement:'Complément'} as Record<string,string>)[input]??input);
  function applyStack(id:string){setLoras(id,[...loras(id),...activeStack.map(l=>({name:l.relativePath.replace(/\.safetensors$/i,''),strength:l.modelWeight,clipStrength:l.clipWeight,active:l.enabled!==false,expanded:false,locked:false,selected:false}))])}
  async function submitted(run:any){lastSeed=run.masterSeed??'';if(saved.some(s=>s.session===session)){saved=saved.map(s=>s.session===session?{...s,lastSeed}:s);await call('set_preference',{key:'generationPreparations',value:saved})}await remember()}
  function stepSeed(direction:'increment'|'decrement'){try{seed=resolveSeed(direction,'',seed||lastSeed);randomEach=false;error=''}catch(e){error=message(e)}}
  onMount(()=>{
    void getCurrentWebview().onDragDropEvent(e=>{if(active&&e.payload.type==='drop'&&e.payload.paths[0]){const path=e.payload.paths[0];if(/\.(png|json)$/i.test(path))void openImage(path)}}).then(fn=>{if(alive)unlisten=fn;else fn()}).catch(e=>{if(alive)error=message(e)});
    initialization=(async()=>{try{
    void call<CharacterCard[]>('character_cards').then(people=>{if(alive)cards=people}).catch(e=>{if(alive)error=message(e)});
    const prefs=await call<any>('preferences');if(!alive)return;saved=prefs.generationPreparations??[];activeStack=prefs.loraActiveStack??[];if(prefs.loraRoot){void call<any>('lora_inventory',{directory:prefs.loraRoot}).then(r=>{if(alive)loraNames=r.items.map((i:any)=>i.relativePath.replace(/\.safetensors$/i,''))}).catch(()=>{})}
    const state=prefs.generationWorkspace;
    // Base preset: multi-pass workflow prepared at install time, not yet opened.
    if(state?.sourcePath&&!state?.session){sourcePath=state.sourcePath;name=state.name??'Préparation';await openFile(false);if(!alive)return}
    else if(state?.session){const r=await call<{session:string;view:GraphView}>('graph_resume',{id:state.session});if(!alive)return;session=r.session;view=r.view;sourcePath=state.sourcePath??'';seed=state.seed??'';name=state.name??'Préparation';p1=state.p1??'';p2=state.p2??'';drafts=state.drafts??{};scopeDrafts=state.scopeDrafts??{};randomEach=state.randomEach??false;lastSeed=state.lastSeed??''}
    if(view)await expandPreset();
    void call<any>('node_catalog').then(c=>{if(alive)catalog=c.nodes??[]}).catch(()=>{/* Fields remain editable offline. */});
    void call<{labels:{path:string;label:string;base:string}[]}>('lora_labels').then(r=>{if(alive)loraLabels=Object.fromEntries(r.labels.map(l=>[l.path,{label:l.label,base:l.base}]))}).catch(()=>{/* Names stay file paths without LoraManager. */});
    if(alive&&view)await receiveLoras();
  }catch(e){error=message(e)}})();});
  let requestedImage:string|null=null;
  $effect(()=>{const path=imagePath;if(!path){requestedImage=null;return}if(active&&path!==requestedImage){requestedImage=path;void openImage(path)}});
  onDestroy(()=>{alive=false;unlisten?.();if(temporary){void call('image_temporary_close',{session}).catch(()=>{});return}if(session){const value=preference();pending=call('set_preference',{key:'generationWorkspace',value}).then(()=>{}).catch(()=>{})}});
  // Presentation helpers (UI V1): one prompt editor at a time, value chips, center tabs.
  type Role='p1'|'p2'|'action'|'decor'|'neg'|'other';
  function roleOf(n:{classType:string;title?:string|null}):Role{const title=n.title??'';if(n.classType==='SayaMainPromptFR')return 'decor';if(/Person 1/i.test(title))return 'p1';if(/Person 2/i.test(title))return 'p2';if(/Action/i.test(title))return 'action';if(/Negative/i.test(title))return 'neg';return 'other'}
  const roleLabels:Record<Role,string>={p1:'Perso 1',p2:'Perso 2',action:'Action',decor:'Décor',neg:'Négatif',other:'Autres'};
  const roles=$derived((['p1','p2','action','decor','neg','other'] as Role[]).map(key=>({key,nodes:prompts.filter(n=>roleOf(n)===key)})).filter(r=>r.nodes.length));
  const shownRole=$derived(roles.some(r=>r.key===promptTab)?promptTab:roles[0]?.key??'');
  const short=(value:string)=>value.split(/[\\/]/).pop()?.replace(/\.(safetensors|ckpt|pt|bin|sft|gguf)$/i,'')??value;
  const seedNode=$derived(view?.document.nodes.find(n=>n.classType==='Seed (rgthree)'));
  const seedChip=$derived(seed.trim()?(seed.trim().length>10?`${seed.trim().slice(0,4)}…${seed.trim().slice(-4)}`:seed.trim()):'');
  const modelLabels:Record<string,string>={ckpt_name:'Checkpoint',unet_name:'UNet',vae_name:'VAE',clip_name:'CLIP',model_name:'Modèle'};
  const modelFields=$derived((view?.document.nodes??[]).flatMap(n=>inputNames(n.id).filter(i=>/^(ckpt_name|unet_name|vae_name|clip_name\d*|model_name)$/.test(i)).map(input=>({id:n.id,input,label:modelLabels[input.replace(/\d+$/,'')]??input,title:`${nodeLabel(view,n.id)} · ${input}`}))));
  const sizeNode=$derived((view?.document.nodes??[]).find(n=>/Latent|Resolution|Size/i.test(`${n.classType} ${n.title??''}`)&&inputNames(n.id).includes('width')&&inputNames(n.id).includes('height')));
  const activeLoras=$derived(loaders.reduce((sum,n)=>sum+loras(n.id).filter(l=>l.active!==false).length,0));
  const rootBypassed=$derived.by(()=>{try{return (JSON.parse(view?.sourceDetails||'{}').nodes??[]).filter((n:any)=>n.mode===4).map((n:any)=>({id:String(n.id),label:nodeLabel(view,String(n.id))}))}catch{return []}});
  const runPhases=$derived(Array.from({length:currentRun?.phaseCount??6},(_,i)=>({n:i+1,state:currentRun?.phases?.[String(i+1)]?.state??'waiting'})));
  const panels=$derived([{id:'nodes',label:$t("Nodes par passe")},...(showApi?[{id:'api',label:$t("API")}]:[]),...(settings&&view?[{id:'settings',label:$t("Réglages du workflow")}]:[]),...(expert&&tools?[{id:'tools',label:$t("Outils avancés")}]:[])]);
  function openNodes(n:number){tab='nodes';void loadPass(n)}
  function toggleSettings(){settings=!settings;selectedNode ||= view?.document.nodes.find(n=>inputNames(n.id).length)?.id??'';tab=settings?'settings':'preview'}
  function toggleTools(){expert=!expert;tab=expert?'tools':'preview'}
  $effect(()=>{if(comparisonJob)tab='compare'});
</script>
{#if modelDialog}
<dialog class="model-dialog" use:showModelDialog oncancel={e=>{e.preventDefault();finishModels(false)}}>
  <h2>{$t("Modèle à remplacer")}</h2><p>{$t("Un modèle du workflow est absent de ComfyUI. Choisis son remplacement ici.")}</p>
  {#each missingModels as item,i}<label>{item.label}<small>{item.original||$t("Aucun modèle sélectionné")}</small>
    <select aria-label={item.label} bind:value={item.replacement}><option value="" disabled>{$t("Choisir un modèle disponible…")}</option>{#each item.options as option}<option value={option}>{option}</option>{/each}</select>
    {#if !item.options.length}<small>{$t("Aucun modèle disponible pour ce champ. Ajoute-le dans ComfyUI puis réessaie.")}</small>{/if}
  </label>{/each}
  <div class="dialog-actions"><button onclick={()=>finishModels(false)}>{$t("Annuler")}</button><button class="primary" disabled={missingModels.some(m=>!m.options.includes(m.replacement))} onclick={()=>finishModels(true)}>{$t("Appliquer et lancer")}</button></div>
</dialog>
{/if}
{#snippet param(id:string,input:string)}
  <label class="node-option"><span>{input}</span>
    {#if choices(id,input)}<select value={text(id,input)} onchange={e=>edit(id,input,e.currentTarget.value)} disabled={busy} data-node={id} data-input={input}>{#each choices(id,input) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>
    {:else if type(id,input)==='BOOLEAN'}<span class="bool-field" class:on={text(id,input)==='true'}><input type="checkbox" checked={text(id,input)==='true'} onchange={e=>edit(id,input,String(e.currentTarget.checked))} disabled={busy} data-node={id} data-input={input}/><em>{text(id,input)==='true'?$t("Activé"):$t("Désactivé")}</em></span>
    {:else}<input value={text(id,input)} oninput={e=>edit(id,input,e.currentTarget.value)} disabled={busy} data-node={id} data-input={input}/>{/if}
  </label>
{/snippet}
{#snippet groupParam(g:Group,id:string,input:string)}
  {@const options=widgetChoices(g.view,catalog,id,input)}
  {@const number=numeric(g,id,input)}
  {@const kind=g.view.document.nodes.find(n=>n.id===id)?.inputs.find(i=>i.name===input)?.type}
  <label class="node-option"><span>{input}</span>
    {#if options?.length}<select value={groupText(g,id,input)} onchange={e=>groupEdit(g,id,input,e.currentTarget.value)} disabled={busy} data-node={g.path.length?undefined:id} data-scope-node={g.path.length?id:undefined} data-input={input}>
      {#if !options.includes(groupText(g,id,input))}<option value={groupText(g,id,input)}>{groupText(g,id,input)} · {$t("indisponible")}</option>{/if}
      {#each options as option}<option value={option}>{option}</option>{/each}</select>
    {:else if kind==='BOOLEAN'}<span class="bool-field" class:on={groupText(g,id,input)==='true'}><input type="checkbox" checked={groupText(g,id,input)==='true'} onchange={e=>groupEdit(g,id,input,String(e.currentTarget.checked))} disabled={busy} data-node={g.path.length?undefined:id} data-scope-node={g.path.length?id:undefined} data-input={input}/><em>{groupText(g,id,input)==='true'?$t("Activé"):$t("Désactivé")}</em></span>
    {:else if kind==='COMBO'}<select disabled><option>{groupText(g,id,input)} · {$t("choix indisponibles")}</option></select>
    {:else if number}<input type="number" min={number.min} max={number.max} step={number.step} value={groupText(g,id,input)} onchange={e=>e.currentTarget.value!==''&&groupEdit(g,id,input,e.currentTarget.value)} disabled={busy} data-node={g.path.length?undefined:id} data-scope-node={g.path.length?id:undefined} data-input={input}/>
    {:else if input==='extra_loras'&&syntaxEditable(groupText(g,id,input))}{@const rows=parseLoraSyntax(groupText(g,id,input))}
      <div class="syntax-loras" data-node={g.path.length?undefined:id} data-input={input}>{#each rows as row,i}<div class="syntax-row"><LoraPicker value={row.name} names={loraList} labels={loraLabels} disabled={busy} label={$t("LoRA supplémentaire")} onpick={name=>groupEdit(g,id,input,formatLoraSyntax(rows.map((r,j)=>j===i?{...r,name}:r)))}/><input aria-label={$t("Poids")} type="number" step="0.05" value={row.strength} onchange={e=>groupEdit(g,id,input,formatLoraSyntax(rows.map((r,j)=>j===i?{...r,strength:e.currentTarget.value||'1'}:r)))} disabled={busy}/><button aria-label={$t("Retirer {name}",{name:row.name})} onclick={()=>groupEdit(g,id,input,formatLoraSyntax(rows.filter((_,j)=>j!==i)))} disabled={busy}>×</button></div>{/each}
        <LoraPicker names={loraList} labels={loraLabels} disabled={busy} label={$t("Ajouter une LoRA supplémentaire")} placeholder={$t("Ajouter une LoRA…")} onpick={name=>groupEdit(g,id,input,formatLoraSyntax([...rows,{name,strength:'1'}]))}/></div>
    {:else if kind==='STRING'&&/text|prompt|value|decor/.test(input)}<textarea rows="3" value={groupText(g,id,input)} oninput={e=>groupEdit(g,id,input,e.currentTarget.value)} disabled={busy} data-node={g.path.length?undefined:id} data-scope-node={g.path.length?id:undefined} data-input={input}></textarea>
    {:else}<input value={groupText(g,id,input)} oninput={e=>groupEdit(g,id,input,e.currentTarget.value)} disabled={busy} data-node={g.path.length?undefined:id} data-scope-node={g.path.length?id:undefined} data-input={input}/>{/if}
  </label>
{/snippet}
<section class="generation-workspace" data-generation-workspace>
{#snippet promptArea(n:any,input:string,rows:number)}<textarea rows={rows} aria-label={promptLabel(n.title)} data-node={n.id} data-input={input} value={text(n.id,input)} oninput={e=>edit(n.id,input,e.currentTarget.value)} disabled={busy} spellcheck="false" placeholder={input==='decor_libre'?$t("Écrire le décor…"):$t("Écrire le prompt…")}></textarea>{/snippet}
{#snippet person(role:1|2)}
  <label class="field-row picker"><span>{role===1?$t("Personnage 1"):$t("Personnage 2")}</span>{#if role===1}<select data-generation-p1 value={p1} disabled={!view||busy} onchange={e=>choose(1,e.currentTarget.value)}><option value="">{$t("Prompts du workflow")}</option>{#each cards as c}<option value={c.id}>{c.name}</option>{/each}</select>{:else}<select data-generation-p2 value={p2} disabled={!view||solo||busy} onchange={e=>choose(2,e.currentTarget.value)}><option value="">{$t("Prompts du workflow")}</option>{#each cards as c}<option value={c.id}>{c.name}</option>{/each}</select>{/if}</label>
{/snippet}
{#snippet leftPanel(advanced:import('svelte').Snippet)}
  <div class="prep">
    {#if temporary}<div class="prep-row"><strong class="temp-name" data-temporary-workflow={session} title={sourcePath}>{name}</strong><button data-image-close disabled={busy} onclick={closeImage}>{$t("Fermer")}</button></div>
    {:else}
      <div class="prep-row"><input class="grow" aria-label={$t("Workflow UI")} title={preset?$t("Preset · maître + 6 passes"):$t("Workflow UI")} data-saya-source bind:value={sourcePath} placeholder={$t("Choisir ton workflow ComfyUI")} disabled={busy}/><button data-workflow-open onclick={()=>openFile(false)} disabled={busy||!sourcePath.trim()}>{$t("Ouvrir")}</button><button class="icon-btn quiet" aria-label={$t("Parcourir…")} title={$t("Parcourir…")} onclick={()=>openFile()} disabled={busy}><Icon name="folder"/></button></div>
      <div class="prep-row"><input class="grow" aria-label={$t("Préparation")} bind:value={name} disabled={busy}/>{#if saved.length}<select class="saved" aria-label={$t("Préparations enregistrées")} value={session} onchange={e=>resume(e.currentTarget.value)} disabled={busy}><option value="">{$t("Charger une préparation…")}</option>{#each saved as s}<option value={s.session}>{s.name}</option>{/each}</select>{/if}<button class="icon-btn quiet" data-generation-save aria-label={$t("Enregistrer")} title={$t("Enregistrer")} onclick={save} disabled={!view||busy}><Icon name="save"/></button></div>
    {/if}
    <p class="save-note" role="status">{note||$t("Travail sur une copie · original protégé")}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
  <div class="wb-scroll">
    <section class="sec prompt-sec">
      <div class="sec-head"><span class="sec-title">{$t("Prompt")}</span><button class="mode" data-generation-mode aria-pressed={!solo} title={$t("Mode")} disabled={!mode||busy} onclick={()=>mode&&edit(mode.id,'Couple Mode',solo?'ON':'OFF')}>{solo?'SOLO':'DUO'}</button></div>
      <div class="sec-body">
        {#if roles.length}
          <div class="seg" role="tablist" aria-label={$t("Prompt")}>{#each roles as r (r.key)}<button role="tab" aria-selected={shownRole===r.key} class:on={shownRole===r.key} class:dim={solo&&r.key==='p2'} onclick={()=>promptTab=r.key}>{$t(roleLabels[r.key])}</button>{/each}</div>
          {#each roles as r (r.key)}<div class="prompt-pane" hidden={shownRole!==r.key}>
            {#each r.nodes as n (n.id)}
              {@const bg=n.classType==='SayaMainPromptFR'}
              <article class="prompt-node" class:inactive={solo&&/Person 2/.test(n.title??'')} data-prompt-node={n.id}>
                <header><span>{bg?$t("Décor libre"):promptLabel(n.title)}</span><small title={n.classType}>#{n.id}{solo&&/Person 2/.test(n.title??'')?' · '+$t("conservé en SOLO"):''}</small></header>
                {#if bg}{@const texts=promptTexts(n.id)}
                  {@render promptArea(n,'decor_libre',5)}
                  {#each texts as tx}<label class="sub-text"><span>{textLabel(tx)}</span><textarea rows="2" data-node={n.id} data-input={tx} value={text(n.id,tx)} oninput={e=>edit(n.id,tx,e.currentTarget.value)} disabled={busy} spellcheck="false"></textarea></label>{/each}
                  <div class="prompt-options">{#each inputNames(n.id).filter(i=>i!=='decor_libre'&&!texts.includes(i)) as option}{@render param(n.id,option)}{/each}</div>
                {:else}{@render promptArea(n,'value',/Anatomy/i.test(n.title??'')?3:r.nodes.length>1?4:7)}{/if}
              </article>
            {/each}
            {#if r.key==='p1'}{@render person(1)}{:else if r.key==='p2'}{@render person(2)}{/if}
          </div>{/each}
        {:else}
          <p class="muted" title={view?$t("Aucun producteur de prompt éditable reconnu. Vérifie la disponibilité du moteur et ses nodes."):$t("Ses prompts et leurs options apparaîtront ici.")}>{$t("Ouvrir ton workflow")}</p>
          <button onclick={()=>openFile()} disabled={busy}>{$t("Choisir le workflow ComfyUI")}</button>
        {/if}
      </div>
    </section>
    {#if !temporary||seedNode}
    <details class="sec" open>
      <summary><span class="sec-title">{$t("Image")}</span>{#if sizeNode}<span class="chip">{text(sizeNode.id,'width')}×{text(sizeNode.id,'height')}</span>{/if}{#if randomEach}<span class="chip">{$t("aléatoire")}</span>{:else if seedChip}<span class="chip" title={seed}>{seedChip}</span>{/if}</summary>
      <div class="sec-body">
        {#if sizeNode}<div class="field-row"><span>{$t("Format")}</span><span class="inline"><input class="num" type="number" aria-label={$t("Largeur")} value={text(sizeNode.id,'width')} onchange={e=>edit(sizeNode.id,'width',e.currentTarget.value)} disabled={busy}/><span class="muted">×</span><input class="num" type="number" aria-label={$t("Hauteur")} value={text(sizeNode.id,'height')} onchange={e=>edit(sizeNode.id,'height',e.currentTarget.value)} disabled={busy}/></span></div>{/if}
        <label class="field-row"><span title={seedNode?`node ${seedNode.id}`:''}>{$t("Master seed")}</span><span class="inline"><input class="num grow" data-saya-seed bind:value={seed} inputmode="numeric" placeholder={$t("Vide = seeds du workflow")} disabled={busy}/><button class="icon-btn quiet" aria-label={$t("Nouveau nombre fixe")} title={$t("Nouveau nombre fixe")} onclick={()=>{seed=randomSeed();randomEach=false}} disabled={busy}><Icon name="shuffle"/></button></span></label>
        <div class="field-row"><span>{$t("Aléatoire")}</span><button class="switch" class:on={randomEach} aria-pressed={randomEach} onclick={()=>randomEach=!randomEach} disabled={busy}><i></i>{$t("Aléatoire à chaque lancement")}</button></div>
        <div class="field-row"><span></span><span class="inline"><button class="grow" data-seed-last onclick={()=>{seed=lastSeed;randomEach=false}} disabled={busy||!lastSeed} title={lastSeed}>{$t("Dernière seed utilisée")}</button><button class="icon-btn" aria-label={$t("Diminuer la seed de 1")} onclick={()=>stepSeed('decrement')} disabled={busy}>−1</button><button class="icon-btn" aria-label={$t("Augmenter la seed de 1")} onclick={()=>stepSeed('increment')} disabled={busy}>+1</button></span></div>
      </div>
    </details>
    {/if}
    {#if modelFields.length}
    <details class="sec">
      <summary><span class="sec-title">{$t("Modèle")}</span><span class="chip model-chip" title={text(modelFields[0].id,modelFields[0].input)}>{short(text(modelFields[0].id,modelFields[0].input))}</span></summary>
      <div class="sec-body">{#each modelFields as f (f.id+':'+f.input)}{@const options=choices(f.id,f.input)}{@const value=text(f.id,f.input)}
        <label class="field-row" title={f.title}><span>{$t(f.label)}</span>{#if options?.length}<select {value} onchange={e=>edit(f.id,f.input,e.currentTarget.value)} disabled={busy}>{#if !options.map(String).includes(value)}<option {value}>{short(value)} · {$t("indisponible")}</option>{/if}{#each options as o}<option value={String(o)}>{short(String(o))}</option>{/each}</select>{:else}<input {value} oninput={e=>edit(f.id,f.input,e.currentTarget.value)} disabled={busy}/>{/if}</label>
      {/each}</div>
    </details>
    {/if}
    <details class="sec" data-generation-loras bind:open={lorasOpen}>
      <summary title={loraChoices(catalog)?$t("{n} LoRA connues du moteur ComfyUI · taper pour filtrer, cliquer pour choisir",{n:loraList?.length ?? 0}):''}><span class="sec-title">{$t("LoRA")}</span><span class="chip">{$tn("{n} active","{n} actives",activeLoras)}</span></summary>
      <div class="sec-body">
        <div class="lora-loaders">{#each loaders as n (n.id)}<div class="lora-loader" data-node-card={n.id}>
          <div class="loader-head"><span>{n.title||'LoRA'}</span><small>#{n.id}</small></div>
          {#each loras(n.id) as l,i}<div class="lora-row" data-lora-index={i}><input aria-label={$t("Activer {name}",{name:l.name})} type="checkbox" checked={l.active} onchange={e=>editLora(n.id,i,'active',e.currentTarget.checked)} disabled={busy}/><LoraPicker label={$t("Nom LoRA")} value={l.name} names={loraList} labels={loraLabels} disabled={busy} onpick={name=>editLora(n.id,i,'name',name)}/><input class="num" aria-label={$t("Poids MODEL")} title={$t("MODEL")} type="number" step="0.05" value={l.strength} onchange={e=>editLora(n.id,i,'strength',e.currentTarget.value)} disabled={busy}/><input class="num" aria-label={$t("Poids CLIP")} title={$t("CLIP")} type="number" step="0.05" value={l.clipStrength??l.strength} onchange={e=>editLora(n.id,i,'clipStrength',e.currentTarget.value)} disabled={busy}/><button class="icon-btn quiet" aria-label={$t("Retirer {name}",{name:l.name})} onclick={()=>setLoras(n.id,loras(n.id).filter((_,j)=>j!==i))} disabled={busy}>×</button></div>{/each}
          <div class="add-lora"><LoraPicker label={$t("Ajouter une LoRA à {name}",{name:n.title||$t("ce loader")})} placeholder={$t("Ajouter une LoRA… (taper pour chercher)")} names={loraList} labels={loraLabels} disabled={busy} onpick={name=>addLora(n.id,name)}/>{#if activeStack.length}<button class="quiet" onclick={()=>applyStack(n.id)} disabled={busy}>{$t("Ajouter le stack du Manager ({n})",{n:activeStack.length})}</button>{/if}</div>
        </div>{:else}<p class="muted" title={$t("Aucun loader LoraManager reconnu dans ce workflow. Les autres loaders sont accessibles dans les réglages du workflow.")}>{$t("Aucun loader LoraManager")}</p>{/each}</div>
        {#if !loraChoices(catalog)}<div class="inline"><button onclick={pickLoraRoot} disabled={busy}>{$t("Dossier des LoRA…")}</button><span class="muted">{$t("{n} LoRA locales · moteur indisponible",{n:loraNames.length})}</span></div>{/if}
      </div>
    </details>
    <details class="sec">
      <summary><span class="sec-title">{$t("Passes")}</span><span class="chip">{runPhases.filter(p=>p.state==='succeeded').length}/{runPhases.length}</span></summary>
      <div class="sec-body pass-rows">{#each [0,...runPhases.map(p=>p.n)] as n}{@const state=runPhases.find(p=>p.n===n)?.state}
        <div class="pass-row"><i class="dot" data-state={state??'common'} title={state??''}></i><span>{n?$t("Passe {n}",{n}):$t("Communs")}</span><button class="quiet" aria-label={$t("Réglages de la passe {n}",{n})} disabled={!view||busy} onclick={()=>openNodes(n)}>{$t("Réglages")}</button></div>
      {/each}</div>
    </details>
    <details class="sec">
      <summary><span class="sec-title">{$t("Nodes du workflow")}</span>{#if rootBypassed.length}<span class="chip">{rootBypassed.length} bypass</span>{/if}</summary>
      <div class="sec-body">
        {#each rootBypassed as b (b.id)}<div class="pass-row"><span>{b.label}</span><small class="muted">#{b.id}</small></div>{/each}
        <button disabled={!view} onclick={()=>openNodes(pass)}>{$t("Ouvrir les nodes")}</button>
      </div>
    </details>
    <details class="sec">
      <summary><span class="sec-title">{$t("Avancé")}</span></summary>
      <div class="sec-body">
        <button data-generation-api disabled={!view||busy} onclick={inspect}>{$t("Vérifier l’API")}</button>
        <button disabled={!view} aria-pressed={settings} onclick={toggleSettings}>{$t("Réglages du workflow")}</button>
        <button data-generation-tools aria-pressed={expert} onclick={toggleTools}>{expert?$t("Fermer les outils"):$t("Outils avancés")}</button>
        {@render advanced()}
      </div>
    </details>
  </div>
{/snippet}
{#snippet compareExtra()}{#if comparisonJob}<section data-generation-comparator class="generation-comparator"><SamplerComparators job={comparisonJob} context={$t("Génération · résultats")} onclose={oncloseComparison} /></section>{/if}{/snippet}
{#snippet centerPanel(id:string)}
  {#if id==='nodes'}<div class="node-workspace">
    <div class="pass-tabs">{#each [0,1,2,3,4,5,6] as n}<button class:chosen={pass===n} disabled={busy} onclick={()=>loadPass(n)}>{n?$t("Passe {n}",{n}):$t("Communs")}</button>{/each}</div>
    <div class="node-scroll">
    {#each sections as sec (sec.key)}<section class="pass-group" data-pass-section={sec.title}><h2>{sec.title}</h2><div class="node-grid">{#each sec.cards as c (c.g.chain.join('/')+':'+c.node)}<article class="node-card" class:off={bypassed(c.g,c.node)} data-node-card={c.node}><header><h2>{nodeLabel(c.g.view,c.node)}</h2><small>#{c.node}</small><button data-node-bypass={c.node} aria-pressed={bypassed(c.g,c.node)} disabled={busy||c.g.view.sourceFormat!=='comfy_ui'||c.g.path.length>0} title={c.g.path.length?$t("Bypass indisponible dans un subgraph"):''} onclick={()=>toggleBypass(c.g,c.node)}>{bypassed(c.g,c.node)?$t("Réactiver"):'Bypass'}</button></header><div class="settings-grid">{#each c.inputs as input}{@render groupParam(c.g,c.node,input)}{/each}</div></article>{/each}</div></section>{:else}<p class="muted empty">{view?$t("Choisis une passe."):$t("Ouvrir ton workflow")}</p>{/each}
    </div>
  </div>
  {:else if id==='api'}<section class="secondary"><header><h2>{$t("API de la copie de travail")}</h2><button onclick={()=>{showApi=false;tab='preview'}}>{$t("Fermer")}</button></header><p class="muted">{preview?.report?.complete?$t("Conversion complète"):$t("Conversion incomplète : consulter les diagnostics")}</p><pre>{preview?.previewJson??JSON.stringify(preview,null,2)}</pre></section>
  {:else if id==='settings'&&view}<section class="secondary"><header><h2>{$t("Paramètres réels du workflow")}</h2><button onclick={()=>{settings=false;tab='preview'}}>{$t("Fermer")}</button></header><select aria-label={$t("Node à régler")} bind:value={selectedNode}>{#each view.document.nodes.filter(n=>inputNames(n.id).length) as n}<option value={n.id}>{n.title||n.classType} · {n.id}</option>{/each}</select><div class="settings-grid">{#each inputNames(selectedNode) as input}{@render param(selectedNode,input)}{/each}</div></section>
  {:else if id==='tools'}<div class="tools-page">{@render tools?.()}</div>{/if}
{/snippet}
{#key temporary ? session : 'saved'}<SayaCouple compact {expert} {prepareSource} startRun={temporary?launchImage:null} temporary={temporary} {active} onrun={value=>currentRun=value} onsubmitted={submitted} ready={!!view&&!busy} left={leftPanel} {panels} panel={centerPanel} {compareExtra} bind:tab/>{/key}
</section>
<style>
.generation-workspace{height:100%;min-height:0}
.model-dialog{max-width:720px;width:90vw;max-height:85vh;overflow:auto;background:var(--panel);color:var(--text);border:1px solid var(--line-strong);border-radius:6px;padding:16px}.model-dialog::backdrop{background:#000b}.model-dialog h2{font-size:14px;margin:0 0 6px}.model-dialog p{color:var(--muted);margin:0}.model-dialog label{display:grid;gap:4px;margin:12px 0}.model-dialog small{overflow-wrap:anywhere;color:var(--muted)}.dialog-actions{display:flex;justify-content:flex-end;gap:8px}
.prep{display:grid;gap:6px;padding:8px 12px;border-bottom:1px solid var(--line);flex:none}
.prep-row{display:flex;gap:6px;align-items:center;min-width:0}
.prep-row .grow,.inline .grow{flex:1;min-width:0}
.prep-row .saved{width:118px;min-width:0}
.temp-name{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}
.save-note{margin:0;font-size:11px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.prep .error{margin:0;font-size:12px;overflow-wrap:anywhere}
.inline{display:flex;gap:6px;align-items:center;min-width:0}.inline input{min-width:0;flex:1}
.mode{min-height:22px;font-size:11px;padding:0 8px;color:var(--text)}
.seg button.dim{opacity:.6}
.prompt-pane{display:grid;gap:8px}.prompt-pane[hidden]{display:none}
.prompt-node{display:grid;gap:4px;min-width:0}
.prompt-node header{display:flex;justify-content:space-between;gap:8px;font-size:11px;color:var(--muted)}.prompt-node header small{color:var(--faint);white-space:nowrap}
.prompt-node textarea,.sub-text textarea{width:100%;resize:vertical;min-height:0}
.prompt-node.inactive textarea{opacity:.6}
.sub-text{display:grid;gap:3px;font-size:11px;color:var(--muted)}
.prompt-options{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:6px}
.picker{margin-top:2px}
.node-option{display:flex;flex-direction:column;gap:3px;font-size:11px;color:var(--muted);min-width:0}.node-option>span:first-child{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.node-option input,.node-option select,.node-option textarea{width:100%;min-width:0;margin:0;font-size:12px}
.node-option:has(.syntax-loras){grid-column:1/-1}
.bool-field{display:flex;align-items:center;gap:6px;min-height:28px;padding:0 8px;border:1px solid var(--line);border-radius:var(--radius);background:var(--raised);cursor:pointer}.bool-field.on{border-color:var(--line-strong)}.bool-field em{font-style:normal;font-size:12px;color:var(--text)}.node-option .bool-field input[type=checkbox]{width:14px;height:14px;min-height:0;margin:0;padding:0;accent-color:var(--accent)}
.switch{display:flex;align-items:center;gap:8px;border:0;background:transparent;padding:0;color:var(--muted);text-align:start;justify-content:flex-start}.switch:hover:not(:disabled){background:transparent;color:var(--text)}
.switch i{width:28px;height:16px;border-radius:8px;background:var(--line-strong);position:relative;flex:none}.switch i::after{content:"";position:absolute;top:2px;left:2px;width:12px;height:12px;border-radius:50%;background:var(--text)}.switch.on i{background:var(--accent)}.switch.on i::after{left:14px;background:var(--accent-text)}
.model-chip{max-width:150px;overflow:hidden;text-overflow:ellipsis}
.lora-loaders{display:grid;gap:10px}
.lora-loader{display:grid;gap:4px;min-width:0}
.loader-head{display:flex;justify-content:space-between;font-size:11px;color:var(--muted)}.loader-head small{color:var(--faint)}
.lora-row{display:grid;grid-template-columns:16px minmax(0,1fr) 50px 50px 24px;gap:4px;align-items:center}
.lora-row input{min-width:0;width:100%;padding:0 4px}.lora-row input[type=checkbox]{width:14px;min-height:0;accent-color:var(--accent)}
.lora-row :global(.lora-picker),.add-lora :global(.lora-picker){min-width:0}
.add-lora{display:grid;gap:4px}.add-lora button{justify-self:start}
.syntax-loras{display:grid;gap:4px}.syntax-row{display:grid;grid-template-columns:minmax(0,1fr) 60px 26px;gap:4px}.syntax-row input,.syntax-row button{margin:0;padding:0 6px;font-size:12px}
.pass-rows{gap:2px}
.pass-row{display:flex;align-items:center;gap:8px;min-height:26px}.pass-row span{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.pass-row button{min-height:22px;font-size:12px}
.dot{width:7px;height:7px;border-radius:50%;background:var(--line-strong);flex:none}.dot[data-state=succeeded]{background:var(--ok)}.dot[data-state=running],.dot[data-state=queued],.dot[data-state=submitted],.dot[data-state=submitting]{background:var(--accent)}.dot[data-state=failed]{background:var(--danger)}.dot[data-state=common]{background:transparent;border:1px solid var(--line-strong)}
.node-workspace{display:flex;flex-direction:column;height:100%;min-height:0}
.pass-tabs{display:flex;gap:2px;padding:6px 12px;border-bottom:1px solid var(--line);flex:none;flex-wrap:wrap}
.pass-tabs button{min-height:26px;border:0;background:transparent;color:var(--muted);font-size:12px}.pass-tabs button.chosen{background:var(--raised);color:var(--text)}
.node-scroll{flex:1;min-height:0;overflow:auto;padding:12px 16px}
.pass-group{margin:0 0 20px}.pass-group>h2{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);margin:0 0 8px;padding-bottom:6px;border-bottom:1px solid var(--line)}
.node-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:10px}
.node-card{border:1px solid var(--line);border-radius:6px;background:var(--panel);padding:10px;min-width:0}.node-card.off{opacity:.6}
.node-card header{display:flex;align-items:center;gap:8px}.node-card h2{font-size:12.5px;font-weight:600;margin:0;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.node-card small{font-size:11px;color:var(--faint)}.node-card header button{min-height:24px;font-size:11.5px}
.settings-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;margin-top:8px}
.empty{padding:8px 0}
.secondary{padding:16px;display:grid;gap:10px;align-content:start}.secondary header{display:flex;justify-content:space-between;align-items:center;gap:12px}.secondary h2{font-size:13px;font-weight:600;margin:0}.secondary p{margin:0}.secondary pre{margin:0;overflow:auto;font-size:11.5px;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:10px;max-height:calc(100vh - 200px)}
.secondary .settings-grid{grid-template-columns:repeat(auto-fill,minmax(220px,1fr))}
.tools-page{padding:0 0 24px}
.generation-comparator{display:block}
</style>
