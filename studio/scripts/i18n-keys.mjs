// Helpers shared by i18n tests/tools: extract literal translation keys used in src/.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src/', import.meta.url));

export function walk(dir = SRC, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) { if (name !== 'locales') walk(p, out); }
    else if (/\.(svelte|ts)$/.test(name) && !/^i18n(-core)?\.ts$/.test(name)) out.push(p);
  }
  return out;
}

const unescape = (s, q) => s.replace(/\\(.)/g, (_, c) => (c === 'n' ? '\n' : c === 't' ? '\t' : c));
// `$t("…")`, `$t('…')`, `t("…")` (not part of a longer identifier or member access), `tn(`/`$tn(` first two args.
const CALL = /(?<![\w.])\$?tn?\(\s*(?:"((?:[^"\\\n]|\\.)*)"|'((?:[^'\\\n]|\\.)*)')(?:\s*,\s*(?:"((?:[^"\\\n]|\\.)*)"|'((?:[^'\\\n]|\\.)*)'))?/g;

/** Returns Map<key, firstFile>. Plural calls `$tn(one, other, n)` register both keys. */
export function usedKeys() {
  const keys = new Map();
  for (const file of walk()) {
    const text = readFileSync(file, 'utf8');
    for (const m of text.matchAll(CALL)) {
      const isPlural = /tn\(/.test(m[0].slice(0, 5));
      const k1 = m[1] ?? (m[2] !== undefined ? unescape(m[2]) : undefined);
      if (k1 === undefined) continue;
      const first = m[1] !== undefined ? unescape(m[1]) : unescape(m[2]);
      if (!keys.has(first)) keys.set(first, file);
      if (isPlural) {
        const second = m[3] !== undefined ? unescape(m[3]) : m[4] !== undefined ? unescape(m[4]) : undefined;
        if (second !== undefined && !keys.has(second)) keys.set(second, file);
      }
    }
  }
  return keys;
}
