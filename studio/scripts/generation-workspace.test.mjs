import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
const source=ts.transpileModule(readFileSync(new URL('../src/lib/generation-workspace.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ES2022}}).outputText;
const {promptNodes,modeNode,widgetChoices,generationTechnical,widgetLinked,widgetSpec,loraChoices,loraMatches,loraResolved,parseLoraSyntax,formatLoraSyntax,routeHomes}=await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
test('actual producer mappings exclude connected encoders and unqualified widgets',()=>{
 const view={document:{nodes:[{id:'145',classType:'PrimitiveStringMultiline',title:'Prompt · Negative'},{id:'807',classType:'PrimitiveStringMultiline',title:'Prompt · Person 1'},{id:'100017',classType:'CLIPTextEncode',title:'CLIP · Person 1'},{id:'x',classType:'PrimitiveStringMultiline',title:'unqualified'},{id:'100220',classType:'saya-couple-mode-toggle',title:'Couple Mode'}]},paramText:{'145':{value:''},'807':{value:'identity'},'100017':{text:''},'100220':{'Couple Mode':'OFF'}}};
 assert.deepEqual(promptNodes(view).map(n=>n.id),['807','145']);assert.equal(modeNode(view).id,'100220');
});

test('promoted choices follow real links and CustomCombo options',()=>{
 const view={sourceDetails:JSON.stringify({nodes:[{id:1,type:'hub'}],definitions:{subgraphs:[{id:'hub',inputs:[{name:'model',linkIds:[10]},{name:'route',linkIds:[11]}],links:[{id:10,target_id:2,target_slot:0},[11,-10,1,3,0,'COMBO']],nodes:[{id:2,type:'Loader',inputs:[{name:'file'}]},{id:3,type:'CustomCombo',inputs:[{name:'choice'}],widgets_values_named:{option1:'Model 1',option2:'Model 2',option3:'',option4:'Model 4'}}]}]}})};
 assert.deepEqual(widgetChoices(view,[{name:'Loader',inputs:[{name:'file',type:['first.safetensors','second.safetensors']}]}],'1','model'),['first.safetensors','second.safetensors']);
 assert.deepEqual(widgetChoices(view,[],'1','route'),['Model 1','Model 2','Model 4']);
 assert.equal(widgetChoices(view,[],'1','model'),null);
 assert.equal(generationTechnical('DaSiWa_CreateExtraMetadata'),true);
 assert.equal(generationTechnical('CheckpointLoaderSimple'),false);
});

test('scoped views keep boundary-linked widgets hidden and read real numeric schema',()=>{
 const def={id:'sg',nodes:[{id:5,type:'KSamplerSelect',inputs:[{name:'sampler_name',link:7}]},{id:6,type:'BasicScheduler',inputs:[{name:'steps',link:null}]}],links:[]};
 const scoped={document:{connections:[],nodes:[]},sourceDetails:JSON.stringify({id:'sg',nodes:[{id:5,type:'KSamplerSelect',inputs:[{name:'sampler_name',link:null}]},{id:6,type:'BasicScheduler',inputs:[{name:'steps',link:null}]}],definitions:{subgraphs:[def]}})};
 assert.equal(widgetLinked(scoped,'5','sampler_name'),true);assert.equal(widgetLinked(scoped,'6','steps'),false);
 assert.deepEqual(widgetSpec(scoped,[{name:'BasicScheduler',inputs:[{name:'steps',type:'INT',config:{min:1,max:10000}}]}],'6','steps'),{type:'INT',config:{min:1,max:10000}});
});
test('LoRA choices come from the engine list and resolve LoraManager basenames',()=>{
 const names=loraChoices([{name:'LoraLoader',inputs:[{name:'lora_name',type:['Style/nyalia.safetensors','detail/AddMicroDetails_v5.safetensors']}]}]);
 assert.deepEqual(names,['Style/nyalia','detail/AddMicroDetails_v5']);assert.equal(loraChoices([]),null);
 assert.deepEqual(loraMatches(names,'micro add'),['detail/AddMicroDetails_v5']);assert.deepEqual(loraMatches(names,''),names);assert.deepEqual(loraMatches(names,'luca',{'Style/nyalia':{label:'(Anime Character) Luca',base:'Illustrious'}}),['Style/nyalia']);
 assert.equal(loraResolved(names,'nyalia'),'Style/nyalia');assert.equal(loraResolved(names,'absent'),null);
 const rows=parseLoraSyntax('<lora:a/b:0.5> <lora:c:1:1>');assert.deepEqual(rows,[{name:'a/b',strength:'0.5'},{name:'c',strength:'1'}]);
 assert.equal(formatLoraSyntax(rows),'<lora:a/b:0.5> <lora:c:1>');
});
test('routing widgets resolve to the subgraph that consumes them, through Set/Get',()=>{
 const source={nodes:[{id:1,type:'hub',outputs:[{name:'USDU 1 Choice',links:[10]},{name:'VAE 5',links:[11]}]},{id:2,type:'p6',inputs:[{name:'USDU 1 Choice'}]},{id:3,type:'SetNode',widgets_values:['route_5'],inputs:[{name:'VAE'}],outputs:[{name:'VAE',links:[]}]}],
  links:[[10,1,0,2,0,'COMBO'],[11,1,1,3,0,'VAE']],definitions:{subgraphs:[{id:'hub',nodes:[],links:[]},{id:'p6',inputs:[{name:'USDU 1 Choice',linkIds:[20]}],nodes:[{id:4,type:'usdu',inputs:[{name:'choice'}]},{id:8,type:'GetNode',widgets_values:['route_5'],outputs:[{name:'VAE',links:[21]}]}],links:[{id:20,origin_id:-10,target_id:4,target_slot:0},{id:21,origin_id:8,target_id:4,target_slot:1}]},{id:'usdu',inputs:[{name:'choice',linkIds:[]}],nodes:[],links:[]}]}};
 const view={sourceDetails:JSON.stringify(source)};
 assert.deepEqual(routeHomes(view,'1','USDU 1'),[['2','4']]);
 assert.deepEqual(routeHomes(view,'1','VAE 5'),[['2','4']]);
 assert.deepEqual(routeHomes(view,'1','Other'),[]);
});

test('widget source is decoded once per view and refreshed when its source changes',()=>{
 const view={document:{nodes:[{id:'1',classType:'sg',inputs:[]}],connections:[]},sourceDetails:JSON.stringify({nodes:[{id:1,type:'CustomCombo',widgets_values_named:{option1:'first'}}],definitions:{subgraphs:[]}})};
 const original=JSON.parse;let parses=0;
 JSON.parse=(...args)=>{parses++;return original(...args)};
 try{
  for(let i=0;i<100;i++){assert.deepEqual(widgetChoices(view,[],'1','choice'),['first']);widgetLinked(view,'1','choice');}
  assert.equal(parses,1);
  view.sourceDetails=JSON.stringify({nodes:[{id:1,type:'CustomCombo',widgets_values_named:{option1:'changed'}}]});
  assert.deepEqual(widgetChoices(view,[],'1','choice'),['changed']);assert.equal(parses,2);
 }finally{JSON.parse=original}
});
