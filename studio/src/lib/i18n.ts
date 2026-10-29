// Saya Studio i18n (2026-10-26): reactive store, locales loaded once (static Vite
// imports, no fetch or re-parse per render), fallback locale → en → key.
// RTL: Arabic sets dir="rtl" on <html> (applyLocale).
//
// API
//   $t("Texte français")                          lookup (locale → en → key)
//   $t("{n} LoRA envoyée(s) vers {slot}", {n, slot})   `{name}` placeholders, substituted after
//                                                 lookup; an unknown placeholder is left as-is.
//   $tn("{n} image", "{n} images", count, vars?)  plural: keys are the French singular / plural
//                                                 forms. `{n}` = count is always available.
// Plural convention (CLDR via Intl.PluralRules): for the category C of the count in the current
// locale, the form is looked up under `<plural key>|C` (C in zero|one|two|few|many|other), e.g.
//   "{n} images|few": "{n} изображения"  (ru; ar needs |zero |two |few |many too)
// Missing `|C` falls back to the singular key (C = one) or the plural key (any other C), so
// fr/en/es/pt-BR/tr/id/vi/zh/ja/ko/th need NO suffixed keys: only the two plain keys.
// Both plain keys are required in every locale; `|C` keys are optional extras (test-enforced
// for ru and ar, whose categories differ from one/other).
import { derived, get, writable } from 'svelte/store';
import { DEFAULT_LOCALE, applyDirection, systemLocale, validLocale, lookupTranslation, lookupPlural } from './i18n-core';
export { LOCALES, DEFAULT_LOCALE, systemLocale, validLocale } from './i18n-core';
import type { LocaleCode } from './i18n-core';

const SOURCES = import.meta.glob('../locales/*.json', { eager: true });
const tables: Record<string, Record<string, string>> = {};
for (const [chemin, module_] of Object.entries(SOURCES)) {
  const code = chemin.split('/').pop()?.replace('.json', '');
  if (code) tables[code] = (module_ as { default: Record<string, string> }).default ?? {};
}

export const locale = writable<LocaleCode>(DEFAULT_LOCALE);

export function applyLocale(code: string): void {
  const valide = validLocale(code);
  locale.set(valide);
  if (typeof document !== 'undefined') applyDirection(document.documentElement, valide);
}

/** Reactive translation function: {$t("…")} updates when the language changes. */
export const t = derived(locale, (code) => (cle: string, vars?: Record<string, string | number>) =>
  lookupTranslation(tables, code, cle, vars));

/** Plural translation: {$tn("{n} image", "{n} images", count)}. See header for the convention. */
export const tn = derived(locale, (code) => (singularKey: string, pluralKey: string, n: number, vars?: Record<string, string | number>) =>
  lookupPlural(tables, code, singularKey, pluralKey, n, vars));

export function changeLocale(code: string): void {
  applyLocale(validLocale(code));
}

export function currentLocale(): LocaleCode {
  return get(locale);
}

/** Real system detection from the browser (Tauri WebView). */
export function currentSystemLocale(): LocaleCode {
  return systemLocale(typeof navigator !== 'undefined' ? navigator.language : undefined);
}
