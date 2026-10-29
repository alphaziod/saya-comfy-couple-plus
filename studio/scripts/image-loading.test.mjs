import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';
import {LruCache} from '../src/lib/stream.ts';
const source=readFileSync(new URL('../src/lib/api.ts',import.meta.url),'utf8');
const body=source.slice(source.indexOf('let active = 0;'),source.indexOf('export interface LiveProgress')).replace('export async function','async function');
function fixture(){
 const calls=[],waiting=[];
 const call=(_action,args)=>{calls.push(args);return new Promise(resolve=>waiting.push(()=>resolve('image:'+args.id)))};
 const load=new Function('call','thumbCache','previewCache',stripTypeScriptTypes(body)+';return imageSource;')(call,new LruCache(250),new LruCache(8));
 return {calls,waiting,load};
}
const flush=()=>new Promise(resolve=>setImmediate(resolve));
test('shared request survives one consumer abort and fills the shared preview cache',async()=>{
 const {calls,waiting,load}=fixture(),controller=new AbortController();
 const first=load('same',true,controller.signal).catch(e=>e.name),second=load('same',true);
 assert.equal(calls.length,1);controller.abort();waiting.shift()();
 assert.equal(await first,'AbortError');assert.equal(await second,'image:same');
 assert.equal(await load('same',true),'image:same');assert.equal(calls.length,1);
});
test('three concurrent decodes, previews first, abandoned queued thumbnails skipped',async()=>{
 const {calls,waiting,load}=fixture(),controller=new AbortController();
 const jobs=['a','b','c'].map(id=>load(id));
 const abandoned=load('offscreen',false,controller.signal).catch(e=>e.name);
 const preview=load('opened',true);controller.abort();assert.equal(calls.length,3);
 waiting.shift()();await flush();assert.equal(calls[3].id,'opened');
 while(waiting.length){waiting.shift()();await flush();}
 assert.equal(await preview,'image:opened');assert.equal(await abandoned,'AbortError');
 await Promise.all(jobs);assert.equal(calls.length,4);
});
