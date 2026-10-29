<script lang="ts">
  import { t, tn } from './lib/i18n';
  import { bindTranslator } from './lib/translate-now';
  import { onMount } from 'svelte';
  import { pageGuides } from './lib/page-guides';
  import Settings from './components/Settings.svelte';
  import { applyLocale, locale, currentSystemLocale, LOCALES } from './lib/i18n';
  import { call, message, native, type Asset } from './lib/api';
  import Gallery from './components/Gallery.svelte';
  import DataMaintenance from './components/DataMaintenance.svelte';
  import Characters from './components/Characters.svelte';
  import CharacterGallery from './components/CharacterGallery.svelte';
  import Generation from './components/Generation.svelte';
  import Casting from './components/Casting.svelte';
  import Engine from './components/Engine.svelte';
  import Workflows from './components/Workflows.svelte';
  import PromptEngine from './components/PromptEngine.svelte';
  import SceneComposer from './components/SceneComposer.svelte';
  import AbLab from './components/AbLab.svelte';
  import Modules from './components/Modules.svelte';
  import ImageEditor from './components/ImageEditor.svelte';
  import GraphEditor from './components/GraphEditor.svelte';
  import Pipelines from './components/Pipelines.svelte';
  import LoraManager from './components/LoraManager.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from './components/Icon.svelte';
  import appIcon from './assets/app-icon.png';
  import { parseBootstrap } from './lib/contracts';
  let selected = $state('gallery'), error = $state(''), appVersion = $state('0.1.0');
  $effect.pre(()=>{bindTranslator($t,$tn)});
  let navigated=false;
  let languageChoice=$state(false), preselectedLanguage=$state('fr'), languageBusy=$state(false), languageError=$state('');
  let generationMounted=$state(false);
  $effect(()=>{if(selected==='generation')generationMounted=true});
  let generationImage=$state<string|null>(null);
  let characterTarget=$state<string|null>(null),galleryTarget=$state<{character:string;asset:string|null}|null>(null);
  let editorTarget=$state<Asset|null>(null);
  function showCharacter(id:string){characterTarget=id;galleryTarget=null;navigate('characters',true);}
  function showGallery(character:string,asset:string|null=null){galleryTarget={character,asset};characterTarget=null;navigate('gallery',true);}
  let comparisonTarget=$state<string|null>(null);
  function showComparison(job:string){comparisonTarget=job;navigate('generation',true);}
  function showEditor(asset:Asset){editorTarget={...asset};characterTarget=null;galleryTarget=null;navigate('editor',true);}
  // V1 shell: an icon rail with the production workspaces; tools are grouped as tabs.
  const primary = [{id:'generation',label:'Créer',icon:'create'},{id:'casting',label:'Casting',icon:'casting'},{id:'gallery',label:'Galerie',icon:'gallery'},{id:'characters',label:'Persos',icon:'characters'},{id:'lora',label:'Modèles',icon:'models'},{id:'workflows',label:'Atelier',icon:'studio'}];
  const system = [{id:'data',label:'Données',icon:'data'},{id:'settings',label:'Réglages',icon:'settings'}];
  const studioTabs = [{id:'workflows',label:'Workflows'},{id:'graph',label:'Éditeur graphique'},{id:'pipelines',label:'Pipelines'},{id:'modules',label:'Modules'},{id:'scenes',label:'Scènes'},{id:'ablab',label:'Labo A/B'},{id:'assistant',label:'Agents & API'}];
  const characterTabs = [{id:'characters',label:'Fiches'},{id:'people',label:'Galerie personnages'}];
  const tools = [...studioTabs,...characterTabs.slice(1),{id:'engine',label:'Moteur'},...system];
  const fullBleed = new Set(['generation','casting','gallery','lora','graph','pipelines','modules','characters','scenes','workflows','ablab','assistant']);
  const railGroup = (id:string) => studioTabs.some(p=>p.id===id) ? 'workflows' : characterTabs.some(p=>p.id===id) ? 'characters' : id;
  const tabsFor = (id:string) => studioTabs.some(p=>p.id===id) ? studioTabs : characterTabs.some(p=>p.id===id) ? characterTabs : null;
  let help = $state(false), engine = $state<{connected:boolean;running:number|null;pending:number|null}|null>(null);
  onMount(() => { if (!native) return; const poll = async () => { try { engine = await call('connection'); } catch { engine = null; } }; void poll(); const timer = setInterval(poll, 10000); return () => clearInterval(timer); });
  const pages = [...primary,...tools];
  onMount(async () => { try { const prefs = await call<Record<string, unknown>>('preferences'); if (!navigated && prefs.selectedTab === 'browser') selected = 'gallery'; if (!navigated && pages.some(p => p.id === prefs.selectedTab)) selected = String(prefs.selectedTab); if (typeof prefs.language === 'string') applyLocale(prefs.language); else { preselectedLanguage = currentSystemLocale(); languageChoice = true; } } catch (e) { error = message(e); } if (!native) return; try { appVersion = parseBootstrap(await invoke('get_bootstrap')).appVersion; } catch { /* fallback: v0.1.0 */ } });
  async function confirmLanguage(){
    if(languageBusy)return;languageBusy=true;languageError='';
    try{await call('set_preference',{key:'language',value:preselectedLanguage});await call('set_preference',{key:'language_setup_completed',value:true});applyLocale(preselectedLanguage);languageChoice=false;}
    catch(e){languageError=message(e)}finally{languageBusy=false}
  }
  async function navigate(id: string, contextual=false) { navigated=true; if(!contextual){generationImage=null;characterTarget=null;galleryTarget=null;comparisonTarget=null;} selected = id; if (native) try { await call('set_preference', { key:'selectedTab',value:id }); } catch(e) { error = message(e); } }
</script>
<div class="shell v1">
  <nav class="rail" aria-label={$t("Navigation principale")}>
    <button class="rail-logo" title="Saya Studio" onclick={() => navigate('gallery')}><img src={appIcon} alt="Saya Studio"/></button>
    {#each primary as page}<button class="rail-item" class:active={railGroup(selected) === page.id} data-nav={page.id} aria-current={railGroup(selected) === page.id ? 'page' : undefined} onclick={() => navigate(page.id)}><Icon name={page.icon} size={18}/><span>{$t(page.label)}</span></button>{/each}
    <span class="rail-grow"></span>
    <button class="rail-item" class:active={selected==='engine'} data-nav="engine" title={engine?.connected ? $t("Connecté") : $t("Hors ligne")} onclick={() => navigate('engine')}><i class="engine-dot" class:off={!engine?.connected}></i><span>{$t("Moteur")}</span>{#if engine?.connected && (engine.running || engine.pending)}<b class="rail-badge">{(engine.running ?? 0) + (engine.pending ?? 0)}</b>{/if}</button>
    {#each system as page}<button class="rail-item" class:active={selected === page.id} data-nav={page.id} aria-current={selected === page.id ? 'page' : undefined} onclick={() => navigate(page.id)}><Icon name={page.icon} size={18}/><span>{$t(page.label)}</span></button>{/each}
    {#if pageGuides[selected]}<button class="rail-item" class:active={help} data-page-guide-toggle onclick={() => help = !help}><Icon name="help" size={18}/><span>{$t("Aide")}</span></button>{/if}
    <small class="rail-version">v{appVersion}</small>
  </nav>
  <main id="main-content" class:full={fullBleed.has(selected)} class="workbench">
    {#if tabsFor(selected)}{@const tabs = tabsFor(selected)!}<div class="subtabs" role="tablist">{#each tabs as page}<button role="tab" class:on={selected === page.id} aria-selected={selected === page.id} data-nav={page.id} onclick={() => navigate(page.id)}>{$t(page.label)}</button>{/each}</div>{/if}
    {#if !native}<p class="notice" role="status">{$t("Aperçu navigateur : ouvrir le binaire Saya Studio pour lire les données locales.")}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if help && pageGuides[selected]}<aside class="guide-pop" aria-label={$t("Mode d’emploi")} data-page-guide={selected}><header><strong>{$t(pageGuides[selected].title)}</strong><button class="quiet icon-btn" aria-label={$t("Fermer")} onclick={() => help = false}><Icon name="close"/></button></header><p>{$t(pageGuides[selected].next)}</p></aside>{/if}
    <div class="screen" class:page={!fullBleed.has(selected)} hidden={selected === 'generation'}>
      {#if selected === 'gallery'}<Gallery target={galleryTarget} oncharacter={showCharacter} onedit={showEditor} oncompare={showComparison} />{:else if selected === 'lora'}<LoraManager />{:else if selected === 'generation'}{:else if selected === 'casting'}<Casting onconfigure={()=>navigate('generation')} onbrowse={()=>navigate('gallery')} />{:else if selected === 'people'}<CharacterGallery onimage={(character,asset)=>showGallery(character,asset)} />{:else if selected === 'characters'}<Characters initialId={characterTarget} onimage={(character,asset)=>showGallery(character,asset)} onbrowse={id=>showGallery(id)} />{:else if selected === 'engine'}<Engine />{:else if selected === 'workflows'}<Workflows />{:else if selected === 'scenes'}<SceneComposer />{:else if selected === 'modules'}<Modules />{:else if selected === 'ablab'}<AbLab />{:else if selected === 'data'}<DataMaintenance />{:else if selected === 'settings'}<Settings />{:else if selected === 'editor'}<ImageEditor target={editorTarget} onclose={()=>navigate('gallery')} />{:else if selected === 'graph'}<GraphEditor />{:else if selected === 'pipelines'}<Pipelines />{:else}<PromptEngine />{/if}
    </div>
    {#if generationMounted}<div class="screen" hidden={selected!=='generation'}><Generation active={selected==='generation'} imagePath={generationImage} comparisonJob={comparisonTarget} oncloseComparison={()=>comparisonTarget=null} /></div>{/if}
  </main>
</div>
{#if languageChoice}<div class="langue-backdrop" data-langue-setup role="presentation"><div class="langue-fenetre" role="dialog" aria-modal="true" aria-label="Choose the interface language"><h2>Choose the interface language</h2><div class="langue-grille">{#each LOCALES as l}<button class:chosen={preselectedLanguage===l.code} data-langue={l.code} disabled={languageBusy} onclick={()=>preselectedLanguage=l.code}>{l.name}</button>{/each}</div><button class="primary" data-langue-confirmer disabled={languageBusy} onclick={confirmLanguage}>Confirm</button>{#if languageError}<p class="error" role="alert">{languageError}</p>{/if}</div></div>{/if}

<style>
.langue-backdrop{position:fixed;inset:0;z-index:90;background:#000c;display:grid;place-items:center}.langue-fenetre{width:min(620px,94vw);display:flex;flex-direction:column;gap:14px;background:var(--panel);border:1px solid var(--line-strong);border-radius:8px;padding:22px}.langue-fenetre h2{margin:0;font-size:16px}.langue-grille{display:grid;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));gap:6px}.langue-grille .chosen{border-color:var(--accent);color:var(--text);background:var(--hover)}
</style>
