# Saya Studio — UI V1 design

Status: proposal for validation. Mockup: `design/ui-v1-mockup.html` (open in a browser).
The current Svelte UI is treated only as a feature inventory. Backend, data and validated behaviours stay; the presentation layer is rebuilt.

## What the product is

A local studio that turns a six-pass ComfyUI workflow into finished images:
1. **Create** — tune a preparation (prompts, characters, models, LoRA, seed) and run it pass by pass.
2. **Casting** — try many characters cheaply (pass 1 only), keep the good ones, finish them.
3. **Gallery** — the images the user actually keeps.
Everything else (models, workflows, pipelines, modules, A/B lab, scenes, characters) supports these three.

## Principles

- **The image is the interface.** The largest, darkest surface on every production screen is the image. Controls sit at the edges.
- **Three-zone workspace** (InvokeAI): controls left, image center, context right (results, inspector). Same skeleton everywhere, so the user never relearns a page.
- **Quiet chrome** (Draw Things): the app icon's slightly dark blue as the only surface hue, hairline separators instead of cards, one accent used only for the primary action and the current selection. No gradients, glow, glass, emoji, decorative badges or marketing headers.
- **Engine state is global.** Connection, queue and progress live in one status strip visible from every screen; a running batch is never hidden by navigation.
- **Dense but legible.** 13px UI text, 28px controls, tabular numerals for seeds/counts, labels above fields, no paragraphs of help text on screen (help in tooltips and an optional guide).

## Shell

- **Activity rail** (48px, left, icons + tooltip): Create, Casting, Gallery, Characters, Models, Studio (workflows, graph, pipelines, modules, scenes, A/B lab). Bottom: Engine, Data, Settings.
- **Status strip** (top, 36px): page title · current preparation · engine status (dot + "ComfyUI connecté", VRAM) · queue count with progress of the active job · notifications.
- No second sidebar, no "Tools" disclosure: secondary tools become tabs inside *Studio*.

## Screens

**Create** — left panel (320px, scrollable sections): Preparation (workflow preset, save/load), Characters (P1/P2, solo/duo), Prompts (identity, anatomy, action, negative, background with options), Model & LoRA, Seed. Center: current/last image, pass strip `1 2 3 4 5 6` under it (state per pass, click to view that pass), primary `Générer` + `Continuer`/`Arrêter` during a run. Right (280px): session results filmstrip, pass comparison toggle. Node-level settings and bypass move to a "Nodes" drawer (per pass), not the main panel.

**Casting** — left panel: new batch (count, family, gender, targeting, rotations, script). Top of center: batch switcher `28/10 09:57 · 34/100` with state and `Terminer — 66 restantes`. Center: contact sheet (2/4/6 per row slider), checkbox selection, keyboard ←→ and space. Right inspector: large image of the focused trial, seed/family/model, actions: `Garder` (to Gallery), `Enregistrer le personnage`, `Finir les 6 passes`. Batch delete in the batch switcher menu, not as a row button.

**Gallery** — left: sources and filters (character, model, LoRA, capture, availability, favourites). Center: justified grid with size slider. Right inspector: metadata, provenance (job, pass, seed, config), actions (open folder, copy path, compare, retouch, trash). Full-screen viewer with ←→, metadata toggle.

**Models** — left: folder tree + kind (LoRA, checkpoints, embeddings). Center: grid of models. Right: detail (metadata, Civitai, triggers) and the LoRA stack with `Envoyer à Créer`. Civitai search/downloads as a tab.

**Studio** — tabs: Workflows, Graph, Pipelines, Modules, Scenes, A/B lab. Each tab is a full workspace with the same three zones.

## Visual system

Dark theme on a neutral black (chosen by the user, 2026-10-29). Colours from the app icon
(`src-tauri/icons/icon.png`): its light blue for the primary action, selection and progress (dark
text on it) and the focus ring; its orange for "where you are", kept, favourite and warnings.
Source of truth: the `:root` tokens at the top of `src/style.css`; never write a colour literal in a
component.

| Token | Value | Use |
|---|---|---|
| `--canvas` | #08090b | image areas, inputs |
| `--bg` | #0c0d10 | app background |
| `--panel` | #131418 | side panels |
| `--raised` | #1b1d22 | buttons, menus |
| `--hover` | #23252b | hover, active item background |
| `--line` / `--line-strong` | #1f2126 / #2e3138 | hairlines, control borders |
| `--text` / `--muted` / `--faint` | #ebecef / #999ba3 / #60636c | text levels |
| `--accent` / `--accent-hover` / `--accent-text` | #5ab0f2 / #7cc2f5 / #06122a | primary action, selection, progress |
| `--accent-light` / `--sel` | #7cbcf0 / #a4cae4 | accent text on dark surfaces / focus ring |
| `--accent-warm` | #f2a271 | where you are (active rail item, active tab, open list item), kept, favourite |
| `--ok` / `--warning` / `--danger` | #7fcaa6 / #f2a271 / #ee8a93 | states only |

Radius 4px (controls) / 6px (images). Spacing scale 4/8/12/16/24. Inter 13/12/11px, 600 for section titles (11px uppercase muted). Icons: one stroke icon set (Lucide), 16px.

## Implementation plan (after validation)

1. New shell (`AppShell`, rail, status strip, workspace layout primitive) behind the existing data layer (`src/lib/api.ts` calls unchanged).
2. Primitive components: Panel, Section, Field, Select, Slider, Toggle, Button (primary/secondary/quiet/danger), Tabs, Menu, Dialog, Tooltip, ImageViewer, Filmstrip, ContactSheet, Inspector.
3. Rebuild screens in order: Gallery (simplest, read-mostly) → Casting → Create → Models → Studio tabs → Engine/Data/Settings.
4. Old components removed screen by screen once the new one passes its native smoke (data-* hooks kept or smokes updated).
