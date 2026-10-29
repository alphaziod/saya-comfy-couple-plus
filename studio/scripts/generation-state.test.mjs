import {test} from 'node:test';
import assert from 'node:assert/strict';
import {phaseRows, runControls, resolveSeed} from '../src/lib/generation-state.ts';
test('six phases use backend states, one next action, no continue while queued',()=>{
 const run={state:'running',phases:{'1':{state:'succeeded'},'2':{state:'queued'}}};
 assert.equal(phaseRows(run).length,6);
 assert.equal(phaseRows(run)[1].state,'queued');
 assert.equal(phaseRows(run)[5].state,'waiting');
 assert.deepEqual(runControls(run),{stop:true,continue:false,restart:false,next:2});
 run.phases['2'].state='succeeded';
 assert.equal(runControls(run).next,3);assert.equal(runControls(run).continue,true);
 assert.equal(runControls({...run,state:'stopped'}).continue,false);
 assert.equal(runControls({...run,state:'completed'}).continue,false);
 assert.equal(runControls({state:'completed',phases:Object.fromEntries(Array.from({length:6},(_,i)=>[i+1,{state:'succeeded'}]))}).restart,true);
});
test('seed resolution is exact and never changes last queued at preview',()=>{
 assert.equal(resolveSeed('fixed','9007199254740993','10'),'9007199254740993');
 assert.equal(resolveSeed('increment','10','9007199254740993'),'9007199254740994');
 assert.equal(resolveSeed('decrement','10','0'),'0');
 assert.equal(resolveSeed('last','10','9007199254740993'),'9007199254740993');
 assert.throws(()=>resolveSeed('last','10',''));
 assert.throws(()=>resolveSeed('fixed','18446744073709551616',''));
 assert.throws(()=>resolveSeed('fixed','bad',''));
});
