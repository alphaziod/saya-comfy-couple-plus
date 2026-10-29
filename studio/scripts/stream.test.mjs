import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LruCache, fingerprintFilters, windowsToCover, thumbCache } from '../src/lib/stream.ts';

test('lru cache evicts least recently used and refreshes recency on read', () => {
  const cache = new LruCache(3);
  cache.set('a', 1); cache.set('b', 2); cache.set('c', 3);
  assert.equal(cache.get('a'), 1);
  cache.set('d', 4); // evicts 'b' (least recently used), not 'a'
  assert.equal(cache.get('b'), undefined);
  assert.equal(cache.get('a'), 1);
  assert.equal(cache.get('c'), 3);
  assert.equal(cache.get('d'), 4);
  assert.equal(cache.size, 3);
  cache.set('e', 5); // evicts 'a' (oldest after the reads above refreshed c and d)
  assert.equal(cache.get('a'), undefined);
  assert.equal(cache.get('c'), 3);
  assert.equal(cache.get('e'), 5);
});

test('lru cache rejects invalid capacity and keeps set-bound under churn', () => {
  assert.throws(() => new LruCache(0));
  assert.throws(() => new LruCache(1.5));
  const cache = new LruCache(2);
  for (let i = 0; i < 1000; i++) cache.set(`k${i}`, i);
  assert.equal(cache.size, 2);
  assert.equal(cache.get('k999'), 999);
  assert.equal(cache.get('k998'), 998);
  assert.equal(cache.get('k0'), undefined);
});

test('shared thumbnail cache stays bounded', () => {
  for (let i = 0; i < 700; i++) thumbCache.set(`asset-${i}`, `data:image/jpeg;base64,${i}`);
  assert.equal(thumbCache.size, 250);
  assert.ok(thumbCache.get('asset-699'));
  assert.equal(thumbCache.get('asset-449'), undefined);
});

test('stream fingerprint ignores pagination offset but keeps every filter', () => {
  const base = { checkpoint: 'a', loras: ['x'], favorite: true, sort: 'newest', offset: 0 };
  assert.equal(fingerprintFilters(base), fingerprintFilters({ ...base, offset: 240 }));
  assert.notEqual(fingerprintFilters(base), fingerprintFilters({ ...base, favorite: false }));
  assert.notEqual(fingerprintFilters(base), fingerprintFilters({ ...base, search: 's' }));
  assert.notEqual(fingerprintFilters(base), fingerprintFilters({ ...base, characters: ['p1'] }));
});

test('window planning covers restored positions with bounded request counts', () => {
  assert.equal(windowsToCover(0, 120), 0);
  assert.equal(windowsToCover(1, 120), 1);
  assert.equal(windowsToCover(120, 120), 1);
  assert.equal(windowsToCover(121, 120), 2);
  // 5000 synthetic entries at a 120 window: 42 requests to reach the end.
  assert.equal(windowsToCover(5000, 120), 42);
  assert.throws(() => windowsToCover(10, 0));
});

import { shouldPrefetch, acceptWindow, restoreNeedsMore, restoreIndex, streamComplete } from '../src/lib/stream.ts';

const base = { loaded: 0, total: null, loading: false, restoring: false, error: false };

test('empty result is complete and never refetched (V12 defect: empty loop)', () => {
  assert.equal(streamComplete(0, 0), true);
  assert.equal(shouldPrefetch({ ...base, total: 0 }, 500), false);
  assert.equal(shouldPrefetch({ ...base, total: null }, 500), false);
  assert.equal(shouldPrefetch({ ...base, loaded: 120, total: 130 }, 300), true);
  assert.equal(shouldPrefetch({ ...base, loaded: 130, total: 130 }, 300), false);
  assert.equal(shouldPrefetch({ ...base, loaded: 120, total: 500, loading: true }, 300), false);
  assert.equal(shouldPrefetch({ ...base, loaded: 120, total: 500, error: true }, 300), false);
  assert.equal(shouldPrefetch({ ...base, loaded: 400, total: 500 }, 300), false);
});

test('queued window from an older generation or stale offset is rejected (V12 defect: stale queued tasks)', () => {
  assert.equal(acceptWindow(3, 3, 120, 120), true);
  assert.equal(acceptWindow(2, 3, 0, 0), false, 'filter changed while queued');
  assert.equal(acceptWindow(3, 3, 240, 0), false, 'offset computed for previous items');
  assert.equal(acceptWindow(3, 3, 120, 240), false, 'duplicate window already appended');
});

test('deep restore is bounded by saved index and total, not by a window cap (V12 defect: deep restore)', () => {
  const ids = n => Array.from({ length: n }, (_, i) => 'a' + i);
  assert.equal(restoreNeedsMore(ids(7200), 20000, { assetId: 'zz', index: 15000 }), true, 'beyond 60x120');
  assert.equal(restoreNeedsMore(ids(15001), 20000, { assetId: 'zz', index: 15000 }), false, 'vanished anchor stops at index');
  assert.equal(restoreNeedsMore(ids(500), 20000, { assetId: 'a10', index: 15000 }), false, 'anchor found early');
  assert.equal(restoreNeedsMore(ids(130), 130, { assetId: 'zz', index: 900 }), false, 'total reached');
  assert.equal(restoreIndex(ids(500), { assetId: 'a42', index: 300 }), 42);
  assert.equal(restoreIndex(ids(500), { assetId: 'gone', index: 300 }), 300);
  assert.equal(restoreIndex(ids(100), { assetId: 'gone', index: 300 }), 99);
  assert.equal(restoreIndex([], { assetId: null, index: 5 }), 0);
});

import { restoreTop } from '../src/lib/stream.ts';
test('restore preserves pixel offset inside anchored row after remount',()=>{
 assert.equal(restoreTop(12,4,250,{rowOffset:73}),823);
 assert.equal(restoreTop(8,4,250,{rowOffset:73}),573);
 assert.equal(restoreTop(0,4,250,{rowOffset:900}),249);
});
