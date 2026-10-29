// Static guard for src-tauri/src/lib.rs: `handle` has a first match (actions
// served without the database lock, each returning early) and a second
// exhaustive match where those same actions sit in an `unreachable!()` group.
// The compiler cannot see a group entry missing from the first match: that is
// a runtime panic. This test makes it a CI failure instead.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
const lines = source.split('\n');
// The real dispatcher is `handle_inner`: `handle` is only the SAYA_STUDIO_TRACE
// tracing wrapper, whose label match (8 spaces) is not an
// action service — the scanner mistook it for early returns.
const start = lines.findIndex(l => l.startsWith('fn handle_inner('));
const lock = lines.findIndex((l, i) => i > start && l.includes('let mut db = s.db.lock()'));
const unreachable = lines.findIndex((l, i) => i > lock && l.includes('unreachable!()'));

function unreachableGroup() {
  const group = [];
  for (let i = unreachable; i > lock; i--) {
    const names = [...lines[i].matchAll(/Request::(\w+)/g)].map(m => m[1]);
    if (!names.length) break;
    group.push(...names);
    if (lines[i].trim().startsWith('Request::')) break;
  }
  return group;
}

test('enum Request has no duplicate variants', () => {
  const body = source.slice(source.indexOf('enum Request {'));
  const variants = [...body.slice(0, body.indexOf('\n}\n')).matchAll(/^    ([A-Z]\w*)\b/gm)].map(m => m[1]);
  assert.ok(variants.length > 50);
  assert.deepEqual(variants.filter((v, i) => variants.indexOf(v) !== i), []);
});

test('every unreachable!() action is served before the database lock, and vice versa', () => {
  assert.ok(start >= 0 && lock > start && unreachable > lock, 'handle() layout changed');
  const group = unreachableGroup();
  const early = new Set([...lines.slice(start, lock).join('\n').matchAll(/^        Request::(\w+)/gm)].map(m => m[1]));
  assert.ok(group.length > 10);
  assert.deepEqual(group.filter(name => !early.has(name)), [], 'would panic at runtime');
  assert.deepEqual([...early].filter(name => !group.includes(name)).sort(), []);
});

test('no action is matched twice in the locked dispatcher', () => {
  const locked = lines.slice(lock, unreachable).join('\n');
  const arms = [...locked.matchAll(/^        Request::(\w+)/gm)].map(m => m[1]);
  assert.deepEqual(arms.filter((v, i) => arms.indexOf(v) !== i), []);
});

import { readdirSync } from 'node:fs';
function frontendActions() {
  const files = [];
  const walk = dir => { for (const entry of readdirSync(dir, { withFileTypes: true })) { const path = dir + '/' + entry.name; if (entry.isDirectory()) walk(path); else if (/\.(svelte|ts)$/.test(entry.name)) files.push(path); } };
  walk(new URL('../src', import.meta.url).pathname);
  const actions = new Set();
  for (const file of files) for (const m of readFileSync(file, 'utf8').matchAll(/\bcall(?:<[^(]*?>)?\(\s*'([a-z_]+)'/g)) actions.add(m[1]);
  return actions;
}

test('every action called by the frontend exists in enum Request', () => {
  const body = source.slice(source.indexOf('enum Request {'));
  const snake = new Set([...body.slice(0, body.indexOf('\n}\n')).matchAll(/^    ([A-Z]\w*)\b/gm)]
    .map(m => m[1].replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase()));
  const actions = frontendActions();
  assert.ok(actions.size > 30);
  assert.deepEqual([...actions].filter(a => !snake.has(a)).sort(), []);
});
