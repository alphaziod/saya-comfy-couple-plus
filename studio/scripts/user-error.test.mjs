import {test} from 'node:test';
import assert from 'node:assert/strict';
import {userError} from '../src/lib/user-error.ts';
test('offline ComfyUI has a short actionable message',()=>{assert.equal(userError(new Error('error sending request for url (http://127.0.0.1:8188/queue)')),'ComfyUI est indisponible. Ouvre-le pour reprendre le suivi.')});
test('validation and unrelated network failures stay explicit',()=>{for(const error of ['model absent','error sending request for url (https://example.com/queue)'])assert.equal(userError(error),error)});
import {bindTranslator} from '../src/lib/translate-now.ts';
import {ENGINE_ERROR_KEYS} from '../src/lib/engine-error-keys.ts';
const fake=(key,vars)=>`[en] ${key}`;
test('exact engine error key is translated',()=>{
 bindTranslator(fake,fake);
 try{assert.equal(userError(new Error('Brouillon introuvable')),'[en] Brouillon introuvable');}finally{bindTranslator((k)=>k,(a)=>a)}
});
test('known prefix is translated and the variable detail is kept',()=>{
 bindTranslator(fake,fake);
 try{assert.equal(userError('Casting : graine absente'),'[en] Casting : graine absente');
  assert.equal(userError('Run casting invalide : id 42 : x'),'[en] Run casting invalide : id 42 : x');}finally{bindTranslator((k)=>k,(a)=>a)}
});
test('unknown text and unknown prefix are unchanged',()=>{
 bindTranslator(fake,fake);
 try{for(const s of ['Inconnu','Préfixe inconnu : détail'])assert.equal(userError(s),s);}finally{bindTranslator((k)=>k,(a)=>a)}
});
test('engine error key list has no duplicates',()=>{assert.equal(new Set(ENGINE_ERROR_KEYS).size,ENGINE_ERROR_KEYS.length)});
