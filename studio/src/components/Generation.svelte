<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount } from 'svelte';
  import { call, message } from '../lib/api';
  import { invoke } from '@tauri-apps/api/core';
  import UniversalPrepare from './UniversalPrepare.svelte';
  import Pipelines from './Pipelines.svelte';
  import GenerationWorkspace from './GenerationWorkspace.svelte';
  let tools=$state(false);
  let generationRoot=$state(''), rootNote=$state('');
  let { active = true, imagePath = null, comparisonJob = null, oncloseComparison = () => {} }: { active?:boolean; imagePath?:string|null; comparisonJob?: string | null; oncloseComparison?: () => void } = $props();
  onMount(async()=>{try{const p=await call<Record<string,unknown>>('preferences');generationRoot=String(p.generationRoot??'')}catch{}});
  async function pickRoot(){const p=await invoke<string[]>('pick_directories');if(p?.length){generationRoot=p[0];await saveRoot()}}
  async function saveRoot(){try{await call('set_preference',{key:'generationRoot',value:generationRoot});rootNote=$t("Destination enregistrée.")}catch(e){rootNote=message(e)}}
</script>
{#snippet advancedTools()}
  <details class="sec" data-generation-storage open>
    <summary title={$t("Dossier Images de l’OS, racine configurable :")+' '+$t("Images/Saya Studio/&lt;personnage&gt;/&lt;date_heure&gt;/&lt;image&gt;.png")+$t(". Vide = pas de publication automatique.")}><span class="sec-title">{$t("Stockage des images finales")}</span>{#if generationRoot}<span class="chip root-chip">{generationRoot}</span>{/if}</summary>
    <div class="sec-body">
      <label class="field-row"><span>{$t("Racine Images")}</span><span class="inline"><input data-generation-root bind:value={generationRoot} placeholder={$t("laisser vide pour ne pas publier")} /><button onclick={pickRoot}>{$t("Dossier…")}</button></span></label>
      <div class="inline"><button data-generation-root-save onclick={saveRoot}>{$t("Enregistrer la racine")}</button>{#if rootNote}<span class="notice">{rootNote}</span>{/if}</div>
    </div>
  </details>
  <details class="sec" data-generation-prepare><summary><span class="sec-title">{$t("Préparation universelle · personnages, prompts et LoRA")}</span></summary><div class="sec-body tool-body"><UniversalPrepare embedded /></div></details>
  <details class="sec" data-generation-pipelines><summary><span class="sec-title">{$t("Pipelines personnalisés")}</span></summary><div class="sec-body tool-body"><Pipelines embedded /></div></details>
{/snippet}
<div class="generation-screen">
  <GenerationWorkspace bind:expert={tools} {imagePath} {active} {comparisonJob} {oncloseComparison} tools={advancedTools}/>
</div>
<style>
  .generation-screen{height:100%;min-height:0;overflow:hidden}
  .inline{display:flex;gap:6px;align-items:center;min-width:0}.inline input{flex:1;min-width:0}
  .root-chip{max-width:320px;overflow:hidden;text-overflow:ellipsis}
  .tool-body{display:block}
</style>
