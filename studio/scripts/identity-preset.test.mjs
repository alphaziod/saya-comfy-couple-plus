import assert from 'node:assert/strict';
import {test} from 'node:test';
import {identityText, applyIdentityPreset} from '../src/lib/identity-preset.ts';
test('identity preset keeps exact bytes and maps P1 preset into the chosen P2 input only',()=>{
 const literal='(silver hair:1.2),  exact\nsecond line';
 const fields=[{nodeId:'1',enabled:true,section:'P1',valueType:'string',value:'one'},{nodeId:'2',enabled:true,section:'P2',valueType:'string',value:'two'},{nodeId:'3',enabled:true,section:'BG',valueType:'string',value:'forest'},{nodeId:'4',enabled:true,section:null,valueType:'seed',value:'18446744073709551615'}];
 const original=structuredClone(fields);const text=identityText({identity:{identity_prompt:'base'}},{promptBlocks:{P1:literal,P2:'wrong identity',BG:'not loaded'}});
 const changed=applyIdentityPreset(fields,'P2',text);assert.equal(changed[1].value,literal);assert.deepEqual(fields,original);assert.deepEqual(changed.filter(f=>f.nodeId!=='2'),original.filter(f=>f.nodeId!=='2'));
});
test('missing, ambiguous or non-text mapping and malformed presets are rejected',()=>{
 const field={enabled:true,section:'P1',valueType:'string',value:'old'};
 assert.throws(()=>applyIdentityPreset([],'P1','text'));
 assert.throws(()=>applyIdentityPreset([field,{...field}],'P1','text'));
 assert.throws(()=>applyIdentityPreset([{...field,valueType:'integer'}],'P1','text'));
 assert.throws(()=>identityText(undefined));assert.throws(()=>identityText({identity:{identity_prompt:42}}));assert.throws(()=>identityText({identity:{identity_prompt:'base'}},{promptBlocks:{BG:'scene'}}));
 assert.equal(identityText({identity:{identityPrompt:'legacy  exact'}}),'legacy  exact');
});
test('append composition preserves both complete texts, weights and whitespace',async()=>{
 const {composePrompt}=await import('../src/lib/identity-preset.ts');
 const before='(original:1.2),  exact\nline',after=' new, (tag:0.7)\n';
 assert.equal(composePrompt(before,after,true),before+'\n'+after);
 assert.equal(composePrompt(before,after,false),after);
 assert.equal(composePrompt('',after,true),after);
 assert.equal(composePrompt(before,'',true),before);
});
