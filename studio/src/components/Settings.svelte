<script lang="ts">
  import { call, message, native } from '../lib/api';
  import { get } from 'svelte/store';
  import { changeLocale, locale, t, LOCALES } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { parseBootstrap, type Bootstrap } from '../lib/contracts';
  let note = $state(''), error = $state(''), busy=$state(false);
  // Folder preferences gathered here; library and LoRA roots keep their own pages for indexing.
  // Language of the folder and file names written in the capture root ('' = same as the interface).
  let fileLanguage = $state('');
  let captureRoot = $state(''), captureDraft = $state(''), captureDefault = $state(''), libraryRoot = $state(''), loraRoot = $state(''), pathsNote = $state('');
  // Read-only information (engine connection, application version): never written from this page.
  let engine = $state<{ connected:boolean; version:string|null }|null>(null), about = $state<Bootstrap|null>(null);
  // ComfyUI output folder: detected path and which source found it (see generation_browser::resolve_output_dir).
  let outputInfo = $state<{ path:string|null; source:string }|null>(null), outputPref = $state('');
  async function loadOutput() {
    try { outputInfo = await call<{ path:string|null; source:string }>('comfy_output_info'); }
    catch (e) { error = message(e); }
  }
  async function saveOutput(value: string) {
    if (busy) return; busy = true; error = '';
    try { await call('set_preference', { key: 'comfyOutputDir', value }); outputPref = value; await loadOutput(); }
    catch (e) { error = message(e); } finally { busy = false; }
  }
  async function chooseOutput() {
    if (busy) return;
    try { const picked = await invoke<string[]>('pick_directories'); if (picked?.length) await saveOutput(picked[0]); }
    catch (e) { error = message(e); }
  }
  function sourceLabel(source: string) {
    switch (source) {
      case 'env': return $t("Variable SAYA_COMFY_OUTPUT_DIR");
      case 'setting': return $t("Réglage");
      case 'running': return $t("ComfyUI en marche");
      case 'default': return $t("Dossier par défaut");
      default: return $t("Introuvable");
    }
  }
  onMount(async () => {
    loadOutput();
    if (native) {
      call<{ connected:boolean; version:string|null }>('connection').then(value => { engine = value; }).catch(() => { engine = null; });
      invoke('get_bootstrap').then(value => { about = parseBootstrap(value); }).catch(() => { about = null; });
    }
    try { const prefs = await call<Record<string, unknown>>('preferences');
    captureRoot = typeof prefs.captureRoot === 'string' ? prefs.captureRoot : ''; captureDraft = captureRoot;
    fileLanguage = typeof prefs.fileNamingLanguage === 'string' ? prefs.fileNamingLanguage : '';
    captureDefault = typeof prefs.defaultCaptureRoot === 'string' ? prefs.defaultCaptureRoot : '';
    outputPref = typeof prefs.comfyOutputDir === 'string' ? prefs.comfyOutputDir : '';
    libraryRoot = typeof prefs.libraryRoot === 'string' ? prefs.libraryRoot : ''; loraRoot = typeof prefs.loraRoot === 'string' ? prefs.loraRoot : '';
  } catch (e) { error = message(e); } });
  async function saveCaptureRoot() {
    if (busy) return; busy = true; error = ''; pathsNote = '';
    try { await call('set_preference', { key: 'captureRoot', value: captureDraft.trim() }); captureRoot = captureDraft.trim(); pathsNote = get(t)("Dossier enregistré. Les prochaines images y seront rangées ; les images existantes ne bougent pas."); }
    catch (e) { error = message(e); } finally { busy = false; }
  }
  async function chooseFileLanguage(code: string) {
    if (busy) return; busy = true; error = '';
    try { await call('set_preference', { key: 'fileNamingLanguage', value: code }); fileLanguage = code; }
    catch (e) { error = message(e); } finally { busy = false; }
  }
  async function chooseLocale(code: string) {
    if(busy)return;busy=true;error='';note='';
    try { await call('set_preference', { key: 'language', value: code });
          changeLocale(code);
          await call('set_preference', { key: 'language_setup_completed', value: true });
          note = get(t)("Langue confirmée — modifiable à tout moment dans Paramètres."); }
    catch (e) { error = message(e); }finally{busy=false}
  }
</script>
<div class="settings">
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <section class="set-sec">
    <h2 class="set-title" data-settings-interface>{$t("Interface")}</h2>
    <div class="set-row top">
      <span class="set-label">{$t("Langue")}</span>
      <div class="set-control">
        <div class="langue-grille" data-settings-langues>
          {#each LOCALES as l}<button class:chosen={$locale === l.code} aria-pressed={$locale === l.code} data-langue={l.code} disabled={busy} onclick={() => chooseLocale(l.code)}>{l.name}</button>{/each}
        </div>
        {#if note}<p class="notice" role="status" data-langue-note>{note}</p>{/if}
      </div>
    </div>
    <div class="set-row">
      <span class="set-label">{$t("Langue des noms de fichiers")}</span>
      <div class="set-control">
        <select data-file-language aria-label={$t("Langue des noms de fichiers")} title={$t("Dossiers et images rangés par l'app (ex. pull/Final, sans-famille)")} value={fileLanguage} disabled={busy} onchange={e=>chooseFileLanguage(e.currentTarget.value)}>
          <option value="">{$t("Comme l'interface")}</option>
          {#each LOCALES as l}<option value={l.code}>{l.name}</option>{/each}
        </select>
      </div>
    </div>
  </section>

  <section class="set-sec" data-settings-paths>
    <h2 class="set-title">{$t("Dossiers")}</h2>
    <div class="set-row">
      <label class="set-label" for="settings-capture-root">{$t("Chemin des nouvelles images")}</label>
      <div class="set-control inline">
        <input id="settings-capture-root" data-settings-capture-root bind:value={captureDraft} placeholder={captureDefault} spellcheck="false" disabled={busy}/>
        <button disabled={busy||!captureDraft.trim()||captureDraft.trim()===captureRoot} onclick={saveCaptureRoot}>{$t("Enregistrer")}</button>
      </div>
    </div>
    {#if pathsNote}<p class="notice row-note" role="status">{pathsNote}</p>{/if}
    <div class="set-row" title={$t("Se règle dans la Galerie.")}>
      <span class="set-label">{$t("Bibliothèque principale")}</span>
      <code class="set-value">{libraryRoot||$t("Non configuré")}</code>
    </div>
    <div class="set-row" title={$t("Se règle dans LoRA.")}>
      <span class="set-label">{$t("Dossier des LoRA")}</span>
      <code class="set-value">{loraRoot||$t("Dossiers du moteur ComfyUI")}</code>
    </div>
    <div class="set-row top" data-settings-comfy-output>
      <span class="set-label">{$t("Dossier output de ComfyUI")}</span>
      <div class="set-control">
        <code class="set-value">{outputInfo?.path || $t("Introuvable")}</code>
        {#if outputInfo}<span class="notice">{$t("Source :")} {sourceLabel(outputInfo.source)}</span>{/if}
        {#if outputInfo && !outputInfo.path}<p class="error" role="alert" data-comfy-output-missing>{$t("Aucun dossier output de ComfyUI trouvé. Choisissez-le pour retrouver les copies et nettoyer les aperçus.")}</p>{/if}
        <div class="set-control inline">
          <button disabled={busy||!native} onclick={chooseOutput}>{$t("Choisir…")}</button>
          <button disabled={busy||!outputPref} onclick={() => saveOutput('')}>{$t("Détection automatique")}</button>
        </div>
      </div>
    </div>
  </section>

  <section class="set-sec">
    <h2 class="set-title">{$t("Moteur ComfyUI")}</h2>
    <div class="set-row">
      <span class="set-label">{$t("État")}</span>
      <span class="set-value"><i class="dot" class:on={engine?.connected}></i>{engine ? (engine.connected ? $t("Connecté") : $t("Hors ligne")) : $t("État non vérifié")}</span>
    </div>
    <div class="set-row">
      <span class="set-label">{$t("Version")}</span>
      <span class="set-value num">{engine?.version ?? $t("non indiquée")}</span>
    </div>
  </section>

  <section class="set-sec">
    <h2 class="set-title">{$t("À propos")}</h2>
    <div class="set-row">
      <span class="set-label">{$t("Saya Studio")}</span>
      <span class="set-value num">{about ? `v${about.appVersion}` : '—'}</span>
    </div>
    <div class="set-row">
      <span class="set-label">{$t("Schéma de la base")}</span>
      <span class="set-value num">{about?.schemaVersion ?? '—'}</span>
    </div>
  </section>
</div>
<style>
  .settings{max-width:760px}
  p{margin:0}
  .set-sec{border-bottom:1px solid var(--line);padding:12px 0}
  .set-sec:first-of-type{padding-top:0}
  .set-title{font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;color:var(--muted);margin:0 0 4px}
  .set-row{display:grid;grid-template-columns:200px minmax(0,1fr);align-items:center;gap:12px;min-height:36px;padding:4px 0;border-top:1px solid var(--line)}
  .set-title+.set-row{border-top:0}
  .set-row.top{align-items:start}.set-row.top .set-label{padding-top:5px}
  .set-label{display:block;color:var(--muted);font-size:12.5px;flex-direction:row}
  .set-control{min-width:0;display:grid;gap:6px}
  .set-control.inline{display:flex;gap:8px}.set-control.inline input{flex:1;min-width:0}
  .set-value{min-width:0;overflow-wrap:anywhere;color:var(--text);display:flex;align-items:center;gap:6px}
  code.set-value{font-size:12px}
  .dot{width:8px;height:8px;border-radius:50%;background:var(--faint);display:inline-block}.dot.on{background:var(--ok)}
  .notice{background:none;border:0;padding:0;color:var(--muted);font-size:12px}
  .row-note{padding:0 0 6px;padding-inline-start:212px}
  .langue-grille{display:flex;flex-wrap:wrap;gap:4px}
  .langue-grille button{min-height:26px;padding:0 9px;font-size:12px;background:var(--raised);border-color:var(--line);color:var(--muted)}
  .langue-grille .chosen{background:var(--hover);border-color:var(--sel);color:var(--text)}
  @media(max-width:700px){.set-row{grid-template-columns:1fr;gap:4px}.row-note{padding-inline-start:0}}
</style>
