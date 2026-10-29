import {test} from 'node:test';
import assert from 'node:assert/strict';
import {castingTimestamp,castingDate} from '../src/lib/casting-progress.ts';
test('historical Rust timestamps and ISO dates are readable without changing files',()=>{
 assert.equal(castingTimestamp('SystemTime { tv_sec: 1793052345, tv_nsec: 699389210 }'),1793052345699.3892);
 assert.equal(castingTimestamp('2026-10-27T00:00:00Z'),Date.parse('2026-10-27T00:00:00Z'));
 for(const value of [null,undefined,'','invalid']){assert.equal(castingTimestamp(value),null);assert.equal(castingDate(value),'—');}
 assert.ok(!castingDate('SystemTime { tv_sec: 1793052345, tv_nsec: 699389210 }').includes('Invalid'));
});
