import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  LOCALES, DEFAULT_LOCALE, RTL, pluralCategory, interpolate, systemLocale, validLocale, lookupTranslation, lookupPlural,
} from '../src/lib/i18n-core.ts';
import { pageGuides } from '../src/lib/page-guides.ts';
import { labels as stateLabels, stateLabel } from '../src/lib/state-label.ts';
import { gallerySorts, reviewLabels, captureLabel } from '../src/lib/gallery.ts';
import { ENGINE_ERROR_KEYS } from '../src/lib/engine-error-keys.ts';
import { usedKeys, walk } from './i18n-keys.mjs';

const ici = fileURLToPath(new URL('.', import.meta.url));
const lus = (code) => JSON.parse(readFileSync(new URL(`../src/locales/${code}.json`, import.meta.url), 'utf8'));

const fr = lus('fr');
const codes = LOCALES.map((l) => String(l.code));

// Plural forms live under `<plural key>|<CLDR category>` (see src/lib/i18n.ts header).
const CATEGORIES = new Set(['zero', 'one', 'two', 'few', 'many', 'other']);
const pluralBase = (k) => {
  const i = k.lastIndexOf('|');
  return i > 0 && CATEGORIES.has(k.slice(i + 1)) ? k.slice(0, i) : null;
};
const placeholders = (s) => (s.match(/\{[A-Za-z_]\w*\}/g) ?? []).sort();

test('toutes les locales existent et ont exactement les clés du français (aucune manquante, aucune ajoutée)', () => {
  const base = Object.keys(fr).filter((k) => !pluralBase(k)).sort();
  for (const code of codes) {
    const table = lus(code);
    const plain = Object.keys(table).filter((k) => !pluralBase(k)).sort();
    assert.deepEqual(plain, base, `clés de ${code}`);
    for (const k of Object.keys(table)) {
      const b = pluralBase(k);
      if (b) assert.ok(b in fr, `${code}:${k} — clé plurielle sans clé de base`);
    }
  }
});

test('JSON valide et valeurs non vides (pas de fallback silencieux)', () => {
  for (const code of codes) {
    for (const [k, v] of Object.entries(lus(code))) {
      assert.equal(typeof v, 'string', `${code}:${k}`);
      assert.ok(v.trim().length >= 1, `${code}:${k} vide`);
    }
  }
});

test('aucun placeholder perdu ni ajouté ({x} conservés à l’identique)', () => {
  for (const code of codes) {
    for (const [k, v] of Object.entries(lus(code))) {
      const b = pluralBase(k);
      if (b) {
        // A plural form may drop {n} ("one" forms: "Une image") but never invent a placeholder.
        const permis = new Set(placeholders(b));
        for (const p of placeholders(v)) assert.ok(permis.has(p), `${code}:${k} — placeholder ${p} inconnu`);
      } else {
        assert.deepEqual(placeholders(v), placeholders(k), `${code}:${k}`);
      }
    }
  }
});

// ---- Source scan: every literal key used by the UI exists, translated, in every locale.
const tableKeys = new Map();
const noter = (k, o) => { if (typeof k === 'string' && k && !tableKeys.has(k)) tableKeys.set(k, o); };
for (const [id, g] of Object.entries(pageGuides)) { noter(g.title, `page-guides:${id}.title`); noter(g.next, `page-guides:${id}.next`); }
for (const [id, v] of Object.entries(stateLabels)) noter(v, `state-label:${id}`);
noter(stateLabel(null), 'state-label:fallback');
for (const [id, v] of gallerySorts) noter(v, `gallery:sort.${id}`);
for (const [id, v] of Object.entries(reviewLabels)) noter(v, `gallery:review.${id}`);
noter(captureLabel('final', null, null), 'gallery:captureLabel');
// Fixed Rust core errors translated at runtime by userError(): they count as referenced keys.
for (const k of ENGINE_ERROR_KEYS) noter(k, 'engine-error-keys');
const lettre = /\p{L}/u;
const attendues = () => {
  const m = new Map();
  for (const [k, f] of usedKeys()) if (lettre.test(k)) m.set(k, f.split('/src/')[1] ?? f);
  for (const [k, o] of tableKeys) m.set(k, m.get(k) ?? o);
  return m;
};

test('les libellés des tables (guides de page, états, tris, validation) sont tous des clés', () => {
  assert.ok(tableKeys.size >= 50, `trop peu de libellés de table lus : ${tableKeys.size}`);
});

test('toute clé littérale $t("…") / t("…") du code existe, non vide, dans les 14 locales', () => {
  const manque = [];
  for (const [cle, origine] of attendues()) {
    for (const code of codes) {
      const v = lus(code)[cle];
      if (typeof v !== 'string' || !v.trim()) manque.push(`${code} ← ${JSON.stringify(cle)} (${origine})`);
    }
  }
  assert.deepEqual(manque, [], `${manque.length} traduction(s) manquante(s)`);
});

test('les {x} de chaque clé du code se retrouvent dans chaque traduction', () => {
  const perdus = [];
  for (const [cle, origine] of attendues()) {
    const attendus = placeholders(cle);
    if (!attendus.length) continue;
    for (const code of codes) {
      const v = lus(code)[cle];
      if (typeof v !== 'string') continue; // reported by the previous test
      const presents = new Set(placeholders(v));
      for (const p of attendus) if (!presents.has(p)) perdus.push(`${code}:${JSON.stringify(cle)} sans ${p} (${origine})`);
    }
  }
  assert.deepEqual(perdus, []);
});

test('aucune clé de locale morte (jamais référencée dans src/)', () => {
  const sources = walk().map((f) => readFileSync(f, 'utf8')).join('\n');
  const variantes = (k) => [k, k.replaceAll('"', '\\"'), k.replaceAll("'", "\\'"), k.replaceAll('\n', '\\n')];
  const morts = Object.keys(fr).filter((k) => {
    const base = pluralBase(k) ?? k;
    return !attendues().has(base) && !variantes(base).some((v) => sources.includes(v));
  });
  assert.deepEqual(morts, [], `${morts.length} clé(s) de locale non utilisée(s)`);
});

test('fallback : locale manquante → anglais → clé', () => {
  const tables = { en: { 'Bonjour': 'Hello' } };
  assert.equal(lookupTranslation(tables, 'en', 'Bonjour'), 'Hello');
  assert.equal(lookupTranslation(tables, 'ja', 'Bonjour'), 'Hello', 'ja sans table → en');
  assert.equal(lookupTranslation(tables, 'ja', 'Absente partout'), 'Absente partout', 'nulle part → clé');
});

test('placeholders substitués sans être altérés', () => {
  assert.equal(lookupTranslation({ en: { '{n} images': '{n} images' } }, 'en', '{n} images', { n: 5 }), '5 images');
});

test('détection langue système (mapping Saya)', () => {
  assert.equal(systemLocale('fr-FR'), 'fr');
  assert.equal(systemLocale('ja'), 'ja');
  assert.equal(systemLocale('ko-KR'), 'ko');
  assert.equal(systemLocale('zh-CN'), 'zh-CN');
  assert.equal(systemLocale('zh-SG'), 'zh-CN');
  assert.equal(systemLocale('zh-Hans'), 'zh-CN');
  assert.equal(systemLocale('zh-TW'), 'zh-TW');
  assert.equal(systemLocale('zh-HK'), 'zh-TW');
  assert.equal(systemLocale('zh-Hant'), 'zh-TW');
  assert.equal(systemLocale('pt-BR'), 'pt-BR');
  assert.equal(systemLocale('pt-PT'), 'en', 'jamais le pays seul : pt-PT → anglais');
  assert.equal(systemLocale('es-AR'), 'es');
  assert.equal(systemLocale('ru'), 'ru');
  assert.equal(systemLocale('tr'), 'tr');
  assert.equal(systemLocale('ar'), 'ar');
  assert.equal(systemLocale('vi'), 'vi');
  assert.equal(systemLocale('th'), 'th');
  assert.equal(systemLocale('id'), 'id');
  assert.equal(systemLocale('de'), 'en', 'inconnu → anglais');
  assert.equal(systemLocale(undefined), DEFAULT_LOCALE);
});

test('langue inconnue → défaut, RTL arabe uniquement, retour LTR propre', () => {
  assert.equal(validLocale('xx'), DEFAULT_LOCALE);
  assert.ok(RTL.has('ar'));
  assert.ok(!RTL.has('fr') && !RTL.has('en'));
});

test('interpolation : {name} substitué après lookup, variable absente laissée telle quelle', () => {
  const tables = { en: { 'Envoyé à {slot} ({n})': 'Sent to {slot} ({n})' } };
  assert.equal(lookupTranslation(tables, 'en', 'Envoyé à {slot} ({n})', { slot: 'P1', n: 3 }), 'Sent to P1 (3)');
  assert.equal(lookupTranslation(tables, 'en', 'Envoyé à {slot} ({n})', { n: 0 }), 'Sent to {slot} (0)');
  assert.equal(lookupTranslation(tables, 'fr', 'Envoyé à {slot} ({n})', { slot: 'P2', n: 1 }), 'Sent to P2 (1)', 'fallback en puis substitution');
  assert.equal(interpolate('{a}{a} {b}', { a: 1 }), '11 {b}');
  assert.equal(interpolate('{toString}', { n: 1 }), '{toString}', 'pas de fuite du prototype');
  assert.equal(interpolate('sans variable'), 'sans variable');
});

test('pluriel : catégories CLDR et repli un/autre', () => {
  const U = '{n} image', A = '{n} images';
  const tables = {
    en: { [U]: '{n} image', [A]: '{n} images' },
    fr: { [U]: '{n} image', [A]: '{n} images' },
    ru: { [U]: '{n} изображение', [A]: '{n} изображений', [`${A}|few`]: '{n} изображения', [`${A}|many`]: '{n} изображений' },
    ar: { [U]: 'صورة واحدة', [A]: '{n} صورة', [`${A}|zero`]: 'لا صور', [`${A}|two`]: 'صورتان', [`${A}|few`]: '{n} صور' },
    ja: { [U]: '{n} 枚', [A]: '{n} 枚' },
  };
  assert.equal(lookupPlural(tables, 'en', U, A, 1), '1 image');
  assert.equal(lookupPlural(tables, 'en', U, A, 5), '5 images');
  assert.equal(lookupPlural(tables, 'fr', U, A, 0), '0 image', 'fr : 0 est singulier');
  assert.equal(lookupPlural(tables, 'fr', U, A, 2), '2 images');
  assert.equal(lookupPlural(tables, 'ru', U, A, 1), '1 изображение');
  assert.equal(lookupPlural(tables, 'ru', U, A, 3), '3 изображения');
  assert.equal(lookupPlural(tables, 'ru', U, A, 5), '5 изображений');
  assert.equal(lookupPlural(tables, 'ru', U, A, 21), '21 изображение');
  assert.equal(lookupPlural(tables, 'ar', U, A, 0), 'لا صور');
  assert.equal(lookupPlural(tables, 'ar', U, A, 1), 'صورة واحدة');
  assert.equal(lookupPlural(tables, 'ar', U, A, 2), 'صورتان');
  assert.equal(lookupPlural(tables, 'ar', U, A, 7), '7 صور');
  assert.equal(lookupPlural(tables, 'ar', U, A, 11), '11 صورة', 'many absent → repli sur la clé plurielle');
  assert.equal(lookupPlural(tables, 'ja', U, A, 1), '1 枚', 'ja : seulement other');
  assert.equal(lookupPlural(tables, 'xx', U, A, 3), '3 images', 'locale inconnue → en');
  assert.equal(lookupPlural({}, 'en', U, A, 4), '4 images', 'aucune table → clé plurielle source');
  assert.equal(lookupPlural(tables, 'en', '{n} job pour {who}', '{n} jobs pour {who}', 2, { who: 'P1' }), '2 jobs pour P1');
  assert.equal(pluralCategory('ru', 2), 'few');
  assert.equal(pluralCategory('zz-invalid-', 1), 'one', 'code invalide → règles en');
});

test('pluriel : chaque appel $tn(un, autre, …) du code a ses deux clés + les formes |catégorie exigées par ru et ar', () => {
  const pluriels = [];
  for (const file of walk()) {
    const texte = readFileSync(file, 'utf8');
    for (const m of texte.matchAll(/(?<![\w.])\$?tn\(\s*(["'])((?:(?!\1)[^\\]|\\.)*)\1\s*,\s*(["'])((?:(?!\3)[^\\]|\\.)*)\3/g)) pluriels.push([m[2], m[4], file]);
  }
  const manque = [];
  for (const [un, autre, file] of pluriels) {
    for (const code of codes) {
      const table = lus(code);
      for (const k of [un, autre]) if (!table[k]?.trim()) manque.push(`${code}:${k} (${file})`);
      if (code === 'ru' || code === 'ar') {
        for (const cat of new Intl.PluralRules(code).resolvedOptions().pluralCategories) {
          if (cat !== 'one' && cat !== 'other' && !table[`${autre}|${cat}`]?.trim()) manque.push(`${code}:${autre}|${cat} (${file})`);
        }
      }
    }
  }
  assert.deepEqual(manque, []);
});
