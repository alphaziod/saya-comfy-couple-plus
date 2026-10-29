// Pure i18n logic (no Vite or Svelte): directly testable with node --test.
export const LOCALES = [
  { code: 'fr', name: 'Français' },
  { code: 'en', name: 'English' },
  { code: 'ja', name: '日本語' },
  { code: 'ko', name: '한국어' },
  { code: 'zh-CN', name: '简体中文' },
  { code: 'zh-TW', name: '繁體中文' },
  { code: 'pt-BR', name: 'Português (Brasil)' },
  { code: 'es', name: 'Español' },
  { code: 'ru', name: 'Русский' },
  { code: 'tr', name: 'Türkçe' },
  { code: 'ar', name: 'العربية', rtl: true },
  { code: 'vi', name: 'Tiếng Việt' },
  { code: 'th', name: 'ไทย' },
  { code: 'id', name: 'Bahasa Indonesia' },
] as const;

export type LocaleCode = (typeof LOCALES)[number]['code'];
export const DEFAULT_LOCALE: LocaleCode = 'fr';
/** Absolute translation fallback (Saya, 2026-10-26): English. */
export const FALLBACK: LocaleCode = 'en';
const CODES = new Set<string>(LOCALES.map((l) => l.code as string));
export const RTL = new Set<string>(LOCALES.filter((l) => 'rtl' in l).map((l) => l.code as string));

export function validLocale(code: string | null | undefined): LocaleCode {
  return code && CODES.has(code) ? (code as LocaleCode) : DEFAULT_LOCALE;
}

/** System locale → supported code. Never based on the country alone: pt-PT → default. */
export function systemLocale(tag: string | undefined): LocaleCode {
  if (!tag) return DEFAULT_LOCALE;
  const bas = tag.toLowerCase();
  const base = bas.split('-')[0];
  if (base === 'zh') return /hant|(tw|hk|mo)/.test(bas) ? 'zh-TW' : 'zh-CN';
  if (base === 'pt') return bas.includes('br') ? 'pt-BR' : 'en';
  const direct: Partial<Record<string, LocaleCode>> = {
    fr: 'fr', ja: 'ja', ko: 'ko', es: 'es', ru: 'ru', tr: 'tr',
    ar: 'ar', vi: 'vi', th: 'th', id: 'id', en: 'en',
  };
  return direct[base] ?? 'en';
}

export type Vars = Record<string, string | number>;
type Tables = Record<string, Record<string, string>>;

/** Substitutes `{name}` placeholders; an unknown placeholder is left as-is (never throws). */
export function interpolate(value: string, vars?: Vars): string {
  if (!vars) return value;
  return value.replace(/\{([A-Za-z_][\w]*)\}/g, (m, name: string) =>
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : m);
}

/** Locale → en → key. {x} placeholders are substituted, never altered. */
export function lookupTranslation(tables: Tables, code: string, key: string, vars?: Vars): string {
  return interpolate(tables[code]?.[key] ?? tables[FALLBACK]?.[key] ?? key, vars);
}

const PLURAL_RULES = new Map<string, Intl.PluralRules>();
/** CLDR plural category of `n` for a locale code (zero|one|two|few|many|other). */
export function pluralCategory(code: string, n: number): Intl.LDMLPluralRule {
  let r = PLURAL_RULES.get(code);
  if (!r) {
    try { r = new Intl.PluralRules(code); } catch { r = new Intl.PluralRules(FALLBACK); }
    PLURAL_RULES.set(code, r);
  }
  return r.select(n);
}

/**
 * Plural lookup. `singularKey` is the French singular source key, `pluralKey` the French plural key.
 * Per locale, the form for category C is `tables[code][pluralKey + '|' + C]`; when absent we fall
 * back to `singularKey` (category `one`) or `pluralKey` (every other category), then to en, then to the
 * source key. `{n}` is always provided as the count; extra `vars` are merged.
 */
export function lookupPlural(tables: Tables, code: string, singularKey: string, pluralKey: string, n: number, vars?: Vars): string {
  const cat = pluralCategory(code, n);
  const base = cat === 'one' ? singularKey : pluralKey;
  const lookup = (c: string): string | undefined =>
    tables[c]?.[`${pluralKey}|${cat}`] ?? tables[c]?.[base];
  // The fr source keys are the last resort; fr `one` covers 0 and 1 (CLDR).
  const value = lookup(code) ?? lookup(FALLBACK) ?? base;
  return interpolate(value, { n, ...vars });
}

/** Sets the direction (RTL for Arabic) and the language on the document root. */
export function applyDirection(root: { lang: string; dir: string }, code: string): void {
  root.lang = code;
  root.dir = RTL.has(code) ? 'rtl' : 'ltr';
}
