// Imperative translation for non-component code (lib validators, thrown errors, notes assigned
// in scripts). Components keep using the reactive `$t`/`$tn` stores; App.svelte binds them here
// so these helpers follow the current locale. Without a binding (node tests, early boot) they
// fall back to the French source key with placeholders substituted.
// Self-contained on purpose: node --test loads this file without a bundler (no extensionless imports).
type Vars = Record<string, string | number>;
function interpolate(value: string, vars?: Vars): string {
  if (!vars) return value;
  return value.replace(/\{([A-Za-z_][\w]*)\}/g, (m, name: string) =>
    Object.prototype.hasOwnProperty.call(vars, name) ? String(vars[name]) : m);
}

type Translate = (key: string, vars?: Vars) => string;
type TranslatePlural = (one: string, other: string, n: number, vars?: Vars) => string;

let current: Translate = (key, vars) => interpolate(key, vars);
let currentPlural: TranslatePlural = (one, other, n, vars) => interpolate(n === 1 ? one : other, { n, ...vars });

export function bindTranslator(translate: Translate, translatePlural: TranslatePlural): void {
  current = translate;
  currentPlural = translatePlural;
}

/** Same as the store $t outside components: French source key plus optional vars. */
export function t(key: string, vars?: Vars): string { return current(key, vars); }
/** Same as the store $tn outside components: singular key, plural key, count, optional vars. */
export function tn(one: string, other: string, n: number, vars?: Vars): string { return currentPlural(one, other, n, vars); }
