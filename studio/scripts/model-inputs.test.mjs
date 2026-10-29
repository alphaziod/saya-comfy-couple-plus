import {test} from 'node:test';
import assert from 'node:assert/strict';
import {missingModel} from '../src/lib/model-inputs.ts';
test('missing checkpoint and empty model request replacement, never substitute silently',()=>{
 assert.equal(missingModel('ckpt_name','old.safetensors',['new.safetensors']),true);
 assert.equal(missingModel('ckpt_name','',['new.safetensors']),true);
 assert.equal(missingModel('ckpt_name','new.safetensors',['new.safetensors']),false);
 assert.equal(missingModel('vae_name','missing',[]),true);
});
test('promoted model choices, valid sentinel and non-model menus',()=>{
 assert.equal(missingModel('Checkpoint · principal','missing',['new.gguf']),true);
 assert.equal(missingModel('vae_name','Baked VAE',['Baked VAE']),false);
 assert.equal(missingModel('sampler','missing',['euler']),false);
 assert.equal(missingModel('ckpt_name','missing',null),false);
});
