import type { GraphView } from './graph/view';

// Views are replaced after edits. Decode their large source once, not once per widget.
const parsedSources = new WeakMap<GraphView, { text: string | null | undefined; value: any }>();
function sourceOf(view: GraphView | null): any {
  if (!view) return {};
  const text = view.sourceDetails, cached = parsedSources.get(view);
  if (cached && cached.text === text) return cached.value;
  let value: any = {};
  try { value = JSON.parse(text || '{}') ?? {}; } catch { /* Unavailable source. */ }
  parsedSources.set(view, { text, value });
  return value;
}

// Identify editable producers, never the connected CLIP consumers.
export function promptNodes(view: GraphView | null) {
  const nodes = (view?.document.nodes ?? []).filter(n =>
    (n.classType === 'PrimitiveStringMultiline' && 'value' in (view?.paramText[n.id] ?? {})) ||
    (n.classType === 'SayaMainPromptFR' && 'decor_libre' in (view?.paramText[n.id] ?? {})));
  // 2-column grid: P1 identity | P1 anatomy, then P2 identity | P2 anatomy below.
  const rank = (title: string) => /Person 1.*Anatomy/i.test(title) ? 1 : /Person 2.*Anatomy/i.test(title) ? 3 :
    /Person 1/i.test(title) ? 0 : /Person 2/i.test(title) ? 2 : /Action/i.test(title) ? 4 : /Negative/i.test(title) ? 5 : 6;
  return nodes.sort((a,b)=>rank(a.title??'')-rank(b.title??''));
}
export function promptLabel(title = '') {
  return title.replace(/^Prompt\s*·\s*/, '').replace('Person 1 · Anatomy','Anatomie · personnage 1')
    .replace('Person 2 · Anatomy','Anatomie · personnage 2').replace('Person 1','Identité · personnage 1')
    .replace('Person 2','Identité · personnage 2').replace('Negative','Négatif').replace('Main / Background','Décor · arrière-plan');
}
export function modeNode(view: GraphView | null) {
  return view?.document.nodes.find(n=>n.classType==='saya-couple-mode-toggle' && 'Couple Mode' in (view.paramText[n.id]??{}));
}

export function nodePass(view: GraphView | null, id: string): number {
  const node=view?.document.nodes.find(n=>n.id===id);if(!node)return 0;
  let title=node.title??'';
  if(view?.sourceDetails){try{const source=sourceOf(view);title+=' '+(source.definitions?.subgraphs?.find((s:any)=>s.id===node.classType)?.name??'')}catch{}}
  return Number(title.match(/Phase\s*([1-6])\b/i)?.[1]??0);
}

export function nodeLabel(view: GraphView | null, id:string):string {
  const n=view?.document.nodes.find(n=>n.id===id);if(!n)return id;
  if(n.title)return n.title;
  try{return sourceOf(view).definitions?.subgraphs?.find((s:any)=>s.id===n.classType)?.name??n.classType}catch{return n.classType}
}

// Follow actual promoted links to the leaf schema; never guess model filenames.
export function widgetChoices(view: GraphView|null, catalog:any[], id:string, input:string):string[]|null {
  const source=sourceOf(view);
  const defs=source.definitions?.subgraphs??[];
  function resolve(node:any, field:string, depth:number):string[]|null {
    if(!node||depth>8)return null;
    if(node.type==='CustomCombo'&&field==='choice'){
      const named=node.widgets_values_named??{};
      const keys=Object.keys(named).filter(k=>/^option\d+$/.test(k)).sort((a,b)=>Number(a.slice(6))-Number(b.slice(6)));
      return (keys.length?keys.map(k=>named[k]):node.widgets_values?.slice(2)??[]).filter((v:any)=>typeof v==='string'&&v.length);
    }
    const schema=catalog.find(c=>c.name===node.type)?.inputs.find((i:any)=>i.name===field)?.type;
    if(Array.isArray(schema))return schema.map(String);
    const def=defs.find((d:any)=>d.id===node.type),port=def?.inputs?.find((p:any)=>p.name===field);
    for(const link of def?.links??[]){
      const linkId=Array.isArray(link)?link[0]:link.id;if(!port?.linkIds?.includes(linkId))continue;
      const target=def.nodes.find((n:any)=>n.id===(Array.isArray(link)?link[3]:link.target_id));
      const slot=Array.isArray(link)?link[4]:link.target_slot;
      const found=resolve(target,target?.inputs?.[slot]?.name,depth+1);if(found?.length)return found;
    }
    return null;
  }
  return resolve(source.nodes?.find((n:any)=>String(n.id)===id),input,0);
}
// Leaf schema of a widget, following promoted links like widgetChoices: real type and min/max/step.
export function widgetSpec(view: GraphView|null, catalog:any[], id:string, input:string):{type:string;config:any}|null {
  const source=sourceOf(view);
  const defs=source.definitions?.subgraphs??[];
  function resolve(node:any, field:string, depth:number):{type:string;config:any}|null {
    if(!node||depth>8)return null;
    const schema=catalog.find(c=>c.name===node.type)?.inputs.find((i:any)=>i.name===field);
    if(typeof schema?.type==='string')return {type:schema.type,config:schema.config??{}};
    const def=defs.find((d:any)=>d.id===node.type),port=def?.inputs?.find((p:any)=>p.name===field);
    for(const link of def?.links??[]){
      const linkId=Array.isArray(link)?link[0]:link.id;if(!port?.linkIds?.includes(linkId))continue;
      const target=def.nodes.find((n:any)=>n.id===(Array.isArray(link)?link[3]:link.target_id));
      const found=resolve(target,target?.inputs?.[Array.isArray(link)?link[4]:link.target_slot]?.name,depth+1);if(found)return found;
    }
    return null;
  }
  return resolve(source.nodes?.find((n:any)=>String(n.id)===id),input,0);
}
export function generationTechnical(classType:string,title='') {
  return /Imprint|^DaSiWa_CreateExtraMetadata$|^SayaImagePhase\dLoad$|^SayaValueToString$|^StringCompare$|^CustomCombo$/.test(classType)||/Cache Key|Imprint|Prompt Gate|SOLO \(empty\)/.test(title);
}

// A scoped view nulls boundary links in its synthetic source; read the definition to keep linked widgets hidden.
export function widgetLinked(view: GraphView|null, id:string, input:string):boolean {
  if(view?.document.connections.some(c=>c.to.node===id&&c.to.input===input))return true;
  const source=sourceOf(view);
  const linked=(node:any)=>node?.inputs?.some((p:any)=>p.name===input&&p.link!=null);
  if(linked(source.nodes?.find((n:any)=>String(n.id)===id)))return true;
  const def=source.id?source.definitions?.subgraphs?.find((d:any)=>d.id===source.id):null;
  return !!linked(def?.nodes?.find((n:any)=>String(n.id)===id));
}

// LoraManager resolves "folder/name" without extension; the list comes from the engine, never from a fixed table.
export function loraChoices(catalog:any[]):string[]|null {
  const files=catalog.find(c=>c.name==='LoraLoader')?.inputs.find((i:any)=>i.name==='lora_name')?.type;
  if(!Array.isArray(files)||!files.length)return null;
  return Array.from(new Set(files.map(f=>String(f).replace(/\.(safetensors|ckpt|pt|bin)$/i,""))));
}
// Every engine LoRA is listed; words match the file path or the LoraManager display name.
export function loraMatches(names:string[], query:string, labels:Record<string,{label:string;base:string}>={}):string[] {
  const words=query.toLowerCase().split(/\s+/).filter(Boolean);
  return names.filter(n=>{const text=`${n} ${labels[n]?.label??''} ${labels[n]?.base??''}`.toLowerCase();return words.every(w=>text.includes(w))});
}
// Stored names may be bare basenames; show which real file LoraManager would use.
export function loraResolved(names:string[], value:string):string|null {
  if(names.includes(value))return value;
  return names.find(n=>n.split('/').pop()===value.split('/').pop())??null;
}
export function parseLoraSyntax(text:string):{name:string;strength:string}[] {
  return Array.from(text.matchAll(/<lora:([^:>]+):([^:>]+)(?::[^>]*)?>/g)).map(m=>({name:m[1],strength:m[2]}));
}
export function formatLoraSyntax(entries:{name:string;strength:string|number}[]):string {
  return entries.map(e=>`<lora:${e.name}:${e.strength}>`).join(' ');
}

// Where a root routing widget is really consumed: chains of subgraph instance ids, followed through links,
// subgraph boundaries and Set/Get pairs. Widget "USDU 1" feeds output "USDU 1 Choice" by the hub convention.
export function routeHomes(view: GraphView|null, id:string, input:string):string[][] {
  const source=sourceOf(view);
  const defs=new Map<string,any>((source.definitions?.subgraphs??[]).map((d:any)=>[d.id,d]));
  const link=(l:any)=>Array.isArray(l)?{id:l[0],from:l[1],to:l[3],slot:l[4]}:{id:l.id,from:l.origin_id,to:l.target_id,slot:l.target_slot};
  const chains=new Map<string,string[]>();
  (function index(nodes:any[],chain:string[],depth:number){if(depth>8)return;for(const n of nodes??[]){const d=defs.get(n.type);if(d&&!chains.has(n.type)){chains.set(n.type,[...chain,String(n.id)]);index(d.nodes,[...chain,String(n.id)],depth+1)}}})(source.nodes,[],0);
  const homes:string[][]=[],seen=new Set<string>();
  const add=(chain:string[])=>{const k=chain.join('/');if(chain.length&&!seen.has(k)){seen.add(k);homes.push(chain)}};
  function enter(node:any,slot:number,chain:string[],depth:number){
    const def=defs.get(node.type);if(!def||depth>8)return false;const here=[...chain,String(node.id)];
    const port=def.inputs?.find((p:any)=>p.name===node.inputs?.[slot]?.name);let deeper=false;
    for(const raw of def.links??[]){const l=link(raw);if(!port?.linkIds?.includes(l.id))continue;
      const target=def.nodes.find((n:any)=>n.id===l.to);if(target&&enter(target,l.slot,here,depth+1))deeper=true}
    if(!deeper)add(here);return true;
  }
  function follow(nodes:any[],links:any[],node:any,outputs:any[],chain:string[],depth:number){
    if(depth>8)return;
    if(node.type==='SetNode'){const name=node.widgets_values?.[0];
      const scan=(list:any[],ls:any[],c:string[])=>{for(const g of list??[])if(g.type==='GetNode'&&g.widgets_values?.[0]===name)follow(list,ls,g,g.outputs??[],c,depth+1)};
      scan(source.nodes,source.links,[]);for(const [defId,d] of Array.from(defs.entries()))scan(d.nodes,d.links,chains.get(defId)??[]);return}
    for(const out of outputs)for(const raw of links??[]){const l=link(raw);if(!out.links?.includes(l.id))continue;
      const target=nodes.find((n:any)=>n.id===l.to);if(!target)continue;
      if(!enter(target,l.slot,chain,depth)){if(chain.length)add(chain);else follow(nodes,links,target,target.outputs??[],chain,depth+1)}}
  }
  const node=source.nodes?.find((n:any)=>String(n.id)===id);if(!node)return [];
  const outs=(node.outputs??[]).filter((o:any)=>o.name===input||o.name?.startsWith(input+' '));
  if(outs.length)follow(source.nodes,source.links,node,outs,[],0);
  return homes;
}
