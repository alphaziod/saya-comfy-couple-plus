<script lang="ts">
  // Action drafts (Casting "Brouillons"): new actions live in a test copy of the engine,
  // are drawn by the casting from that copy, and enter the real pull only on a YES per
  // action (`solo action add` in the real engine, which keeps a backup for rollback).
  import { onMount } from 'svelte';
  import { t, tn } from '../lib/i18n';
  import { call, message } from '../lib/api';
  type Draft = { id: string; famille: string; action: Record<string, unknown>; state: 'draft' | 'promoted'; updated?: string; promotedAt?: string; backup?: string };
  const backupName = (d: Draft) => String(d.backup ?? '').split('/').pop() ?? '';
  let { disabled = false, onlaunch = () => {} }: { disabled?: boolean; onlaunch?: (ids: string[], n: number) => void } = $props();
  let drafts = $state<Draft[]>([]), families = $state<string[]>([]), fields = $state<Record<string, string>>({});
  let engineMissing = $state('');
  let busy = $state(false), error = $state(''), note = $state('');
  let famille = $state(''), id = $state(''), add = $state(''), noteText = $state(''), extra = $state('');
  let editing = $state(''), confirmId = $state(''), checked = $state<string[]>([]), n = $state(4);
  const pending = $derived(drafts.filter(d => d.state === 'draft'));
  const promoted = $derived(drafts.filter(d => d.state === 'promoted'));
  const extraFields = $derived(Object.keys(fields).filter(k => !['add', 'note', 'valide', '_note'].includes(k)));

  function apply(r: { drafts: Draft[]; families: string[]; fields: Record<string, string>; engineMissing?: string }) {
    drafts = r.drafts; families = r.families; fields = r.fields; engineMissing = r.engineMissing ?? '';
    if (!famille && families.length) famille = families[0];
    checked = checked.filter(c => pending.some(d => d.id === c));
  }
  async function run(fn: () => Promise<void>) {
    if (busy) return;
    busy = true; error = ''; note = '';
    try { await fn(); } catch (e) { error = message(e); } finally { busy = false; }
  }
  const load = () => run(async () => apply(await call('action_drafts')));
  onMount(load);

  function clearForm() { editing = ''; id = ''; add = ''; noteText = ''; extra = ''; }
  function edit(d: Draft) {
    editing = d.id; id = d.id; famille = d.famille;
    const { add: text, note: n0, valide: _v, ...rest } = d.action as Record<string, unknown>;
    add = String(text ?? ''); noteText = typeof n0 === 'string' ? n0 : '';
    extra = Object.keys(rest).length ? JSON.stringify(rest, null, 1) : '';
  }
  function save() {
    return run(async () => {
      let more: Record<string, unknown> = {};
      if (extra.trim()) {
        try { more = JSON.parse(extra); } catch { throw Error($t("Champs avancés : JSON invalide")); }
        if (!more || typeof more !== 'object' || Array.isArray(more)) throw Error($t("Champs avancés : un objet JSON est attendu"));
      }
      const entry = { ...more, add: add.trim(), ...(noteText.trim() ? { note: noteText.trim() } : {}) };
      const saved = await call<Draft>('action_draft_save', { id: id.trim(), famille, entry });
      apply(await call('action_drafts'));
      if (!checked.includes(saved.id)) checked = [...checked, saved.id];
      note = $t("Brouillon « {id} » enregistré dans la copie de test (le vrai moteur n'est pas touché).", { id: saved.id });
      clearForm();
    });
  }
  function remove(d: Draft) {
    return run(async () => {
      await call('action_draft_delete', { id: d.id });
      apply(await call('action_drafts'));
      if (editing === d.id) clearForm();
    });
  }
  function promote(d: Draft) {
    return run(async () => {
      const done = await call<Draft>('action_draft_promote', { id: d.id });
      confirmId = '';
      apply(await call('action_drafts'));
      note = $t("« {id} » est maintenant dans le pull réel (famille {famille}). Pour annuler : solo rollback --backup {backup}", { id: d.id, famille: d.famille, backup: backupName(done) });
    });
  }
  function refresh() {
    return run(async () => {
      apply(await call('action_drafts_refresh'));
      note = $t("Copie de test refaite depuis le vrai moteur, brouillons rejoués.");
    });
  }
  function toggle(d: Draft) { checked = checked.includes(d.id) ? checked.filter(c => c !== d.id) : [...checked, d.id]; }
</script>

<div class="drafts" data-action-drafts>
  <p class="small muted">{$t("Écris une action, teste-la au casting, puis dis Oui pour l'ajouter au vrai pull. Rien n'entre dans le moteur réel sans ton Oui.")}</p>

  {#if engineMissing}<p class="small" role="status" data-draft-engine-missing>{$t("Moteur de tirage introuvable dans {path}. Les brouillons d'actions ont besoin du moteur (dossier configurable dans saya studio config ou par SAYA_ENGINE_DIR).", { path: engineMissing })}</p>{/if}
  <form class="form" onsubmit={e => { e.preventDefault(); save(); }}>
    <div class="row">
      <select data-draft-famille aria-label={$t("Famille d'actions")} bind:value={famille} disabled={busy || !!editing}>{#each families as f}<option value={f}>{f}</option>{/each}</select>
      <input data-draft-id aria-label={$t("Identifiant")} placeholder={$t("identifiant_action")} bind:value={id} disabled={busy || !!editing} />
    </div>
    <textarea data-draft-add rows="3" aria-label={$t("Texte de l'action (add)")} placeholder={$t("Texte de l'action (tags ajoutés au prompt)")} bind:value={add} disabled={busy}></textarea>
    <input data-draft-note aria-label={$t("Note")} placeholder={$t("Note (facultative)")} bind:value={noteText} disabled={busy} />
    <details class="advanced"><summary>{$t("Champs avancés (JSON)")}</summary>
      <textarea data-draft-extra rows="4" aria-label={$t("Champs avancés (JSON)")} placeholder={'{"negative_add": "…"}'} bind:value={extra} disabled={busy}></textarea>
      <p class="small muted">{$t("Champs possibles : {fields}", { fields: extraFields.join(', ') })}</p>
    </details>
    <div class="row">
      <button class="primary" type="submit" data-draft-save disabled={busy || !famille || !id.trim() || !add.trim()}>{editing ? $t("Mettre à jour") : $t("Enregistrer le brouillon")}</button>
      {#if editing}<button type="button" class="quiet" onclick={clearForm} disabled={busy}>{$t("Annuler")}</button>{/if}
    </div>
  </form>

  {#if pending.length}
    <ul class="list" data-draft-list>
      {#each pending as d (d.id)}
        <li class:current={editing === d.id}>
          <label class="pick"><input type="checkbox" data-draft-check={d.id} checked={checked.includes(d.id)} onchange={() => toggle(d)} disabled={busy} />
            <span class="name">{d.id}</span><span class="muted small">{d.famille}</span></label>
          <div class="actions">
            <button class="quiet" data-draft-edit={d.id} onclick={() => edit(d)} disabled={busy}>{$t("Modifier")}</button>
            {#if confirmId === d.id}
              <button class="primary" data-draft-confirm={d.id} onclick={() => promote(d)} disabled={busy}>{$t("Confirmer : ajouter au vrai moteur")}</button>
              <button class="quiet" onclick={() => (confirmId = '')} disabled={busy}>{$t("Annuler")}</button>
            {:else}
              <button data-draft-yes={d.id} onclick={() => (confirmId = d.id)} disabled={busy}>{$t("Oui → pull réel")}</button>
              <button class="quiet danger" data-draft-delete={d.id} onclick={() => remove(d)} disabled={busy}>{$t("Supprimer")}</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    <div class="row">
      <input class="num n" type="number" min="1" max="200" aria-label={$t("Nombre d'essais")} data-draft-n bind:value={n} disabled={busy || disabled} />
      <button class="primary" data-draft-test disabled={busy || disabled || !checked.length} onclick={() => onlaunch(checked, Math.max(1, Number(n) || 1))}>{$tn("Tester {n} brouillon", "Tester {n} brouillons", checked.length)}</button>
    </div>
    <p class="small muted">{$t("Les essais arrivent dans la grille : garde ou jette comme d'habitude.")}</p>
  {:else}
    <p class="small muted" data-draft-empty>{$t("Aucun brouillon en attente.")}</p>
  {/if}

  {#if promoted.length}
    <details open><summary class="small">{$tn("{n} action passée dans le pull réel", "{n} actions passées dans le pull réel", promoted.length)}</summary>
      <ul class="list done">{#each promoted as d (d.id)}<li data-draft-promoted={d.id}><span class="name">{d.id}</span><span class="muted small">{d.famille}</span>{#if backupName(d)}<code class="small" title={$t("Commande pour annuler cet ajout")}>solo rollback --backup {backupName(d)}</code>{/if}</li>{/each}</ul>
    </details>
  {/if}

  <button class="quiet small" data-draft-refresh onclick={refresh} disabled={busy}>{$t("Refaire la copie de test")}</button>
  {#if busy}<p class="small muted" role="status">{$t("Moteur en cours…")}</p>{/if}
  {#if note}<p class="small" role="status" data-draft-note>{note}</p>{/if}
  {#if error}<p class="error small" role="alert" data-draft-error>{error}</p>{/if}
</div>

<style>
  .drafts{display:flex;flex-direction:column;gap:8px;padding:4px 0}
  .form{display:flex;flex-direction:column;gap:6px}
  .row{display:flex;gap:6px;align-items:center}
  .row>*{flex:1;min-width:0}
  .row .n{width:56px;flex:none}
  input,select,textarea{width:100%;min-width:0}
  textarea{resize:vertical;font-family:inherit}
  .advanced summary{font-size:11.5px;color:var(--muted);cursor:pointer}
  .list{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:4px}
  .list li{display:flex;flex-direction:column;gap:4px;padding:6px;border:1px solid var(--line);border-radius:4px}
  .list li.current{border-color:var(--accent)}
  .list.done li{flex-direction:row;flex-wrap:wrap;gap:8px;padding:3px 6px}
  .list.done code{flex-basis:100%;color:var(--muted);overflow-wrap:anywhere}
  .pick{display:flex;align-items:center;gap:6px;min-width:0}
  .pick input{width:auto}
  .name{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .actions{display:flex;flex-wrap:wrap;gap:4px}
  .actions button{flex:1 1 auto}
  .danger{color:var(--danger)}
  .small{font-size:11.5px}
  .muted{color:var(--muted)}
  .error{color:var(--danger)}
</style>
