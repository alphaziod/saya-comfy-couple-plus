import { test } from 'node:test';
import assert from 'node:assert/strict';
import { restoreFilters, captureLabel } from '../src/lib/gallery.ts';
test('restores old filters and rejects malformed persistent preferences', () => {
  assert.equal(restoreFilters({ lora: 'legacy' }).lora, 'legacy');
  assert.deepEqual(restoreFilters({ checkpoint: 'old' }).checkpoints, ['old']);
  assert.deepEqual(restoreFilters({ loras: ['A', 'A', 2], offset: -1, sort: 'bad', favorite: 'true' }), { availability:'',metadataOnly:false,checkpoint:'',checkpoints:[],lora:'',loras:['A'],character:'',characters:[],variant:'',workflow:'',job:'',search:'',offset:0,sort:'name_asc',metadataState:'',loraExact:false,duplicates:false,favorite:false,review:'',capture:'' });
  assert.equal(restoreFilters({ offset: 48, sort: 'newest', favorite: true }).offset, 48);
  assert.equal(restoreFilters(null).loras.length, 0);
});

test('restores combined historical provenance filters',()=>{
 const f=restoreFilters({characters:['P1','P2','P1',3],variant:'old-variant',workflow:'workflow',job:'job',favorite:true,review:'accepted'});
 assert.deepEqual(f.characters,['P1','P2']);assert.equal(f.variant,'old-variant');assert.equal(f.workflow,'workflow');assert.equal(f.job,'job');assert.equal(f.favorite,true);assert.equal(f.review,'accepted');
});

test('exact indexed LoRA group survives preferences while legacy mode stays inclusive',()=>{
 assert.equal(restoreFilters({loraExact:true,loras:['A','B','A']}).loraExact,true);
 assert.deepEqual(restoreFilters({loraExact:true,loras:['A','B','A']}).loras,['A','B']);
 assert.equal(restoreFilters({loraExact:'true'}).loraExact,false);
 assert.equal(restoreFilters({lora:'legacy'}).loraExact,false);
});

test('file state preferences preserve missing and removed views and reject unknown values',()=>{
 for(const availability of ['missing','removed','source_inaccessible','outside_roots','unreadable']) assert.equal(restoreFilters({availability}).availability,availability);
 assert.equal(restoreFilters({availability:'invented'}).availability,'');
});

test('capture view filter restores only known values and labels passes readably',()=>{
 assert.equal(restoreFilters({capture:'all'}).capture,'all');
 assert.equal(restoreFilters({capture:'main'}).capture,'main');
 assert.equal(restoreFilters({capture:'invented'}).capture,'');
 assert.equal(captureLabel('pull',1,6),'PASSE 1/6');
 assert.equal(captureLabel('final',6,6),'Finale');
 assert.equal(captureLabel('final',null,null),'Finale');
 assert.equal(captureLabel(null,null,null),'');
});
