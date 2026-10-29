<script lang="ts">
  import { t } from '../lib/i18n';
  import { onMount, onDestroy, type Snippet } from 'svelte';
  import { call, message } from '../lib/api';
  import { phaseRows, runControls } from '../lib/generation-state';
  import { castingDate } from '../lib/casting-progress';
  import RunProgress from './RunProgress.svelte';
  import PhaseComparisons from './PhaseComparisons.svelte';
  import Thumbnail from './Thumbnail.svelte';
  import Icon from './Icon.svelte';
  type Panel={id:string;label:string};
  // Workbench owner of the Create screen: the run state lives here, so the launch button, the stage,
  // the pass strip and the session results render here; the caller fills the left panel and extra tabs.
  let { expert = false, prepareSource = null, startRun = null, temporary = false, ready = true, active = true, onrun = (_run:any)=>{}, onsubmitted = async (_run:any) => {}, left, panels = [], panel, compareExtra, tab = $bindable('preview') }: { compact?: boolean; expert?: boolean; prepareSource?: (() => Promise<{path:string;seed:string}>) | null; startRun?: (()=>Promise<any>) | null; temporary?: boolean; ready?: boolean; active?:boolean; onrun?:(run:any)=>void; onsubmitted?: (run:any)=>Promise<void>; left?: Snippet<[Snippet]>; panels?: Panel[]; panel?: Snippet<[string]>; compareExtra?: Snippet; tab?: string } = $props();
  let livePreview=$state<string|null>(null);
  let previewPhase=$state(0);
  let sourcePath=$state(''),isolatedRoot=$state(''),seed=$state(''),busy=$state(false),syncing=$state(false),error=$state(''),note=$state('');
  let run=$state<any>(null),runs=$state<any[]>([]),selectedRun=$state(''),loaded=$state(false);
  let rightOpen=$state(true),info=$state(false);
  const previewAsset=$derived(previewPhase ? phaseRows(run).find(p=>p.n===previewPhase)?.asset : [...phaseRows(run)].reverse().find(p=>p.asset)?.asset);
  const showingLive=$derived(!!run&&!!livePreview&&!previewPhase);
  let timer:ReturnType<typeof setInterval>|null=null,alive=true;
  const controls=$derived(runControls(run));
  const phases=$derived(phaseRows(run));
  const done=$derived(phases.filter(p=>p.state==='succeeded').length);
  const current=$derived(tab==='preview'||tab==='compare'||panels.some(p=>p.id===tab)?tab:'preview');
  const sessionRuns=$derived(temporary?(run?[run]:[]):runs.map(r=>r.id===run?.id?run:r));
  const lastAsset=(r:any)=>[...phaseRows(r)].reverse().find(p=>p.asset)?.asset as string|undefined;
  $effect(()=>{onrun(run)});
  // End of phase awaiting a decision: popup with the image, not just two small buttons.
  let dismissed=$state('');
  const reviewKey=$derived(run?`${run.id}:${controls.next}`:'');
  const reviewAsset=$derived(controls.continue&&controls.next===2?phaseRows(run).find(p=>p.n===controls.next-1)?.asset:undefined);
  const reviewOpen=$derived(active&&!!reviewAsset&&dismissed!==reviewKey);
  const focusOnOpen=(node:HTMLElement)=>{node.focus()};
  const labels:Record<string,string>={waiting:'En attente',submitted:'Soumise',submitting:'Soumission',queued:'En file',running:'En cours',succeeded:'Réussie',failed:'Erreur',cancelled:'Annulée',interrupted:'Arrêtée',submission_unknown:'À synchroniser',awaiting_review:'À valider'};
  // Casting trials live on the Casting page. Stopped runs are hidden except the selected
  // one, which can still be resumed.
  const listedRun=(r:any)=>!r.casting&&r.state!=='stopped';
  const runLabel=(r:any)=>r?.state==='completed'?$t("Terminée"):r?.state==='stopped'?$t("Arrêtée"):$t("Exécution en cours");
  async function refresh(){
    if(!run?.id||syncing)return;const id=run.id;syncing=true;
    try{
      let value;
      if(run.singleJob){
        const jobs=await call<any[]>('sync_jobs'),job=jobs.find(j=>j.id===id);
        if(!job)throw Error($t("Génération introuvable"));
        value={...run,state:job.state==='succeeded'?'completed':['failed','cancelled','interrupted'].includes(job.state)?'stopped':'running',phases:{'1':{job:id,state:job.state,asset:job.outputs?.find((o:any)=>o.assetId)?.assetId}},autoError:job.error?JSON.stringify(job.error):''};
      }else value=await call<any>('saya_status',{run:id});
      if(alive&&run?.id===id){run=value;error=value.autoError??''}
    }
    catch(e){if(alive)error=$t("Synchronisation impossible ; dernier état conservé. {detail}",{detail:message(e)})}finally{syncing=false}
  }
  async function selectRun(id:string){
    previewPhase=0;livePreview=null;selectedRun=id;run=runs.find(r=>r.id===id)??null;
    if(run){seed=run.masterSeed??'';sourcePath=run.sourcePath;isolatedRoot=run.isolatedRoot;await call('set_preference',{key:'selectedSayaRun',value:id});await refresh()}
  }
  async function recover(){
    try{const [list,prefs]=await Promise.all([call<any[]>('saya_runs'),call<any>('preferences')]);if(!alive)return;runs=list.filter(r=>listedRun(r)||(!r.casting&&r.id===prefs.selectedSayaRun));await selectRun(list.some(r=>r.id===prefs.selectedSayaRun)?prefs.selectedSayaRun:list[0]?.id??'')}
    catch(e){error=message(e)}finally{loaded=true}
  }
  async function start(){
    if(!startRun&&!prepareSource&&!sourcePath.trim()){error=$t("Choisir le workflow UI Saya.");return}
    if(busy)return;busy=true;error='';note='';
    try{
      if(startRun){run=await startRun();}else{
      if(prepareSource){const prepared=await prepareSource();sourcePath=prepared.path;seed=prepared.seed;}
      run=await call<any>('saya_start',{source_path:sourcePath.trim(),master_seed:seed.trim()||null,isolated_root:isolatedRoot.trim()||`saya-tests/ui-${crypto.randomUUID()}`});
      }
      previewPhase=0;livePreview=null;if(!temporary)runs=[run,...runs];selectedRun=run.id;await onsubmitted(run);if(!temporary)await call('set_preference',{key:'selectedSayaRun',value:run.id});await refresh();note=$t("Génération lancée.");
    }catch(e){error=message(e)}finally{busy=false}
  }
  async function advance(action:'continue'|'restart'|'stop'){
    if(!run?.id||busy)return;busy=true;error='';note='';
    try{
      if(action!=='stop'){await refresh();if(error&&!run.autoError)throw Error(error);}
      const allowed=runControls(run);if(!allowed[action])throw Error($t("Commande indisponible dans cet état."));
      if(action==='restart'&&!window.confirm($t("Relancer la phase 1 avec une nouvelle seed ? Le suivi courant repartira en phase 1.")))return;
      if(run.singleJob&&action==='stop'){const state=run.phases['1'].state;await call('control_job',{id:run.id,operation:state==='queued'?'cancel_queue':'interrupt',confirmed:true})}else await call(`saya_${action}`,{run:run.id});if(action!=='stop'){previewPhase=0;livePreview=null;dismissed=''}await refresh();note=action==='stop'?$t("Arrêt demandé ; résultats conservés."):action==='restart'?$t("Phase 1 relancée avec une nouvelle seed ({seed}).",{seed:run.masterSeed}):$t("Phase suivante soumise.");
    }catch(e){error=message(e)}finally{busy=false}
  }
  async function openFolder(){if(!previewAsset)return;try{await call('open_asset_directory',{id:previewAsset})}catch(e){error=message(e)}}
  function showPhase(n:number){previewPhase=n;tab='preview'}
  onMount(()=>{if(temporary)loaded=true;else void recover();timer=setInterval(()=>{if(!busy)void refresh()},2500)});
  onDestroy(()=>{alive=false;if(timer)clearInterval(timer)});
</script>
{#snippet advanced()}{#if expert}<label class="stack-field"><span>{$t("Stockage intermédiaire isolé")}</span><input data-saya-root bind:value={isolatedRoot} placeholder={$t("Automatique")}/></label>{/if}{/snippet}
<div class="wb gen-wb" class:right-closed={!rightOpen} data-saya-couple data-temporary-job={temporary&&run?.singleJob?run.id:undefined}>
  <aside class="wb-panel left">
    <div class="go">
      <button class="primary launch" data-saya-start disabled={!loaded||busy||controls.stop||!ready} onclick={start} title={note||undefined}><Icon name="play"/><span>{busy?$t("Préparation…"):$t("Générer")}</span></button>
    </div>
    {#if error}<p class="error run-error" role="alert">{error}</p>{/if}
    {@render left?.(advanced)}
  </aside>
  <main class="wb-center">
    <div class="wb-bar center-tabs" role="tablist">
      <button class="tab" role="tab" aria-selected={current==='preview'} class:on={current==='preview'} onclick={()=>tab='preview'}>{$t("Aperçu")}</button>
      <button class="tab" role="tab" aria-selected={current==='compare'} class:on={current==='compare'} onclick={()=>tab='compare'}>{$t("Comparer les passes")}</button>
      {#each panels as p (p.id)}<button class="tab" role="tab" aria-selected={current===p.id} class:on={current===p.id} onclick={()=>tab=p.id}>{p.label}</button>{/each}
      <span class="grow"></span>
      {#if run}<span class="muted num run-state" data-saya-state><strong>{runLabel(run)}</strong> · {done}/{phases.length}</span>{/if}
      <button class="icon-btn quiet" aria-pressed={rightOpen} aria-label={$t("Session")} title={$t("Session")} onclick={()=>rightOpen=!rightOpen}><Icon name="queue"/></button>
    </div>
    <div class="panes">
      <div class="pane stage" hidden={current!=='preview'}>
        <div class="run-preview" data-preview-phase={previewPhase||'live'}>
          {#if showingLive}<img class="stage-live" src={livePreview} alt={$t("Aperçu progressif de la passe en cours")}/>
          {:else if run&&previewAsset}<Thumbnail id={previewAsset} preview alt={previewPhase?$t("Résultat de la phase {n}",{n:previewPhase}):$t("Dernier résultat disponible")}/>
          {:else}<span class="stage-empty">{run?(previewPhase?$t("Phase {n} · {state} · aucun aperçu disponible",{n:previewPhase,state:$t(labels[phases[previewPhase-1]?.state]??phases[previewPhase-1]?.state)}):$t("L’aperçu apparaîtra ici")):$t("Aperçu de la génération")}</span>{/if}
        </div>
        <div class="stage-tools">
          <button class="icon-btn" aria-pressed={info} aria-label={$t("Informations")} title={$t("Informations")} disabled={!run} onclick={()=>info=!info}><Icon name="info"/></button>
          <button class="icon-btn" aria-label={$t("Comparer les passes")} title={$t("Comparer les passes")} onclick={()=>tab='compare'}><Icon name="compare"/></button>
          <button class="icon-btn" aria-label={$t("Ouvrir le dossier")} title={$t("Ouvrir le dossier")} disabled={!run||!previewAsset||showingLive} onclick={openFolder}><Icon name="folder"/></button>
        </div>
        {#if info&&run}<dl class="kv stage-info"><dt>{$t("Seed")}</dt><dd class="num">{run.masterSeed??$t("workflow")}</dd><dt>{$t("État")}</dt><dd>{runLabel(run)}</dd><dt>{$t("Exécution")}</dt><dd class="num">{String(run.id).slice(0,8)}</dd></dl>{/if}
        {#if run}
          <div class="float">
            <div class="passes">
              <button class="pass live" data-preview-live aria-pressed={previewPhase===0} onclick={()=>showPhase(0)}>{$t("Direct")}</button>
              {#each phases as p (p.n)}<span class="saya-phase" data-saya-phase={p.n} data-state={p.state}><button class="pass" class:done={p.state==='succeeded'} class:running={['running','queued','submitted','submitting'].includes(p.state)} class:failed={p.state==='failed'} aria-pressed={previewPhase===p.n} aria-label={$t("Voir la phase {n}",{n:p.n})} title={$t("Phase {n} · {state}",{n:p.n,state:$t(labels[p.state]??p.state)})} onclick={()=>showPhase(p.n)}>{p.n}</button></span>{/each}
            </div>
            <span class="vsep"></span>
            <RunProgress {run} compact onpreview={image=>livePreview=image}/>
            {#if controls.continue||controls.restart||controls.stop}<span class="vsep"></span>{/if}
            {#if controls.continue}<button class="primary" data-saya-continue disabled={busy||syncing} onclick={()=>advance('continue')}>{$t("Continuer · phase {n}",{n:controls.next})}</button>{/if}
            {#if controls.restart}<button data-saya-restart disabled={busy||syncing} title={$t("Restart · nouvelle seed")} onclick={()=>advance('restart')}>{$t("Relancer")}</button>{/if}
            {#if controls.stop}<button class="danger" data-saya-stop disabled={busy||syncing} title={$t("■ Arrêter la génération")} onclick={()=>advance('stop')}>{$t("Arrêter")}</button>{/if}
          </div>
        {/if}
      </div>
      <div class="pane compare-pane" hidden={current!=='compare'}>{@render compareExtra?.()}<PhaseComparisons {run}/></div>
      {#each panels as p (p.id)}<div class="pane extra-pane" hidden={current!==p.id}>{@render panel?.(p.id)}</div>{/each}
    </div>
  </main>
  <aside class="wb-panel right" hidden={!rightOpen}>
    <div class="wb-head"><h2>{$t("Session")}</h2><span class="muted num">{sessionRuns.length}</span>{#if run}<button class="icon-btn quiet" data-saya-refresh aria-label={$t("Actualiser")} title={$t("Actualiser")} disabled={busy||syncing} onclick={refresh}><Icon name="redo"/></button>{/if}</div>
    {#if runs.length}<div class="run-pick"><select data-saya-runs aria-label={$t("Exécution enregistrée")} value={selectedRun} disabled={busy||syncing} onchange={e=>selectRun(e.currentTarget.value)}>{#each runs as r}<option value={r.id}>{r.id.slice(0,8)} · seed {r.masterSeed??$t("workflow")}</option>{/each}</select></div>{/if}
    <div class="wb-scroll">
      <div class="thumbs">{#each sessionRuns.slice(0,60) as r (r.id)}{@const a=lastAsset(r)}
        <button class="thumb" class:cur={r.id===run?.id} disabled={busy||syncing} title={`${String(r.id).slice(0,8)} · ${runLabel(r)}`} onclick={()=>{if(r.id!==run?.id)void selectRun(r.id);else tab='preview'}}>{#if a}<Thumbnail id={a} alt={$t("Dernier résultat disponible")}/>{:else}<span class="thumb-empty">{runLabel(r)}</span>{/if}</button>
      {/each}</div>
    </div>
    {#if run}<dl class="kv run-kv"><dt>{$t("Seed")}</dt><dd class="num">{run.masterSeed??$t("workflow")}</dd><dt>{$t("État")}</dt><dd>{runLabel(run)}</dd><dt>{$t("Passes")}</dt><dd class="num">{done}/{phases.length}</dd>{#if run.createdAt}<dt>{$t("Créée")}</dt><dd class="num">{castingDate(run.createdAt)}</dd>{/if}</dl>{/if}
  </aside>
  {#if reviewOpen&&run}
    <div class="review-backdrop" role="presentation" onclick={e=>{if(e.target===e.currentTarget)dismissed=reviewKey}}>
      <div class="review-popup" role="dialog" aria-modal="true" aria-label={$t("Phase {n} terminée",{n:controls.next-1})} data-saya-review-popup tabindex="-1" use:focusOnOpen onkeydown={e=>{if(e.key==='Escape')dismissed=reviewKey}}>
        <header><h2>{$t("Phase {n} terminée",{n:controls.next-1})}</h2><span class="num">seed {run.masterSeed??$t("workflow")}</span></header>
        <div class="review-image"><Thumbnail id={reviewAsset!} preview alt={$t("Résultat de la phase {n}",{n:controls.next-1})}/></div>
        <div class="review-actions">
          <button data-review-later class="quiet" onclick={()=>dismissed=reviewKey}>{$t("Plus tard")}</button>
          <button class="danger" data-review-stop disabled={busy||syncing} onclick={()=>advance('stop')}>{$t("■ Arrêter ici")}</button>
          {#if controls.restart}<button data-review-restart disabled={busy||syncing} onclick={()=>advance('restart')}>{$t("Restart · nouvelle seed")}</button>{/if}
          <button class="primary" data-review-continue disabled={busy||syncing} onclick={()=>advance('continue')}>{$t("Valider · enchaîner les passes 2 à 6")}</button>
        </div>
      </div>
    </div>
  {/if}
</div>
<style>
  .gen-wb{--wb-left:340px;--wb-right:280px;position:relative}
  .gen-wb.right-closed{grid-template-columns:var(--wb-left) minmax(0,1fr) 0}
  .wb-panel[hidden]{display:none}
  .left{overflow:hidden}
  .go{display:flex;gap:6px;padding:10px 12px;border-bottom:1px solid var(--line);flex:none}
  .launch{flex:1;min-height:34px;font-size:13px;display:flex;align-items:center;justify-content:center;gap:8px}
  .run-error{margin:0;padding:6px 12px;border-bottom:1px solid var(--line);font-size:12px;overflow-wrap:anywhere}
  .stack-field{display:grid;gap:4px}.stack-field span{color:var(--muted)}
  .center-tabs .tab{border:0;background:transparent;color:var(--muted);min-height:26px}
  .center-tabs .tab.on{background:var(--raised);color:var(--text)}
  .run-state{font-size:12px;white-space:nowrap}.run-state strong{font-weight:500;color:var(--text)}
  .panes{position:relative;flex:1;min-height:0}
  .pane{position:absolute;inset:0;min-width:0;min-height:0}
  .pane[hidden]{display:none}
  .compare-pane,.extra-pane{overflow:auto;background:var(--bg)}
  .compare-pane{padding:16px}
  .stage{overflow:hidden;background:var(--canvas)}
  .run-preview{position:absolute;inset:16px 16px 76px;display:grid;place-items:center}
  .run-preview :global(img),.stage-live{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;display:block;border-radius:var(--r-img)}
  .run-preview :global(.image-placeholder),.stage-empty{color:var(--faint);font-size:12px;text-align:center}
  .stage-tools{position:absolute;top:14px;inset-inline-end:14px;display:flex;flex-direction:column;gap:4px}
  .stage-tools button{background:var(--panel)}
  .stage-info{position:absolute;top:14px;inset-inline-end:52px;background:var(--panel);border:1px solid var(--line-strong);border-radius:6px;padding:8px 10px;font-size:12px;max-width:300px}
  .float{position:absolute;left:50%;transform:translateX(-50%);bottom:16px;display:flex;align-items:center;gap:10px;background:var(--panel);border:1px solid var(--line-strong);border-radius:8px;padding:6px 8px;max-width:calc(100% - 24px)}
  .passes{display:flex;gap:4px}
  .saya-phase{display:contents}
  .pass{width:30px;min-height:30px;padding:0;position:relative;display:grid;place-items:center;color:var(--muted);font-size:12px}
  .pass.live{width:auto;padding:0 8px}
  .pass.done{color:var(--text)}.pass.done::after,.pass.running::after,.pass.failed::after{content:"";position:absolute;bottom:3px;left:50%;transform:translateX(-50%);width:12px;height:2px;border-radius:1px;background:var(--ok)}
  .pass.running{border-color:var(--accent);color:var(--text)}.pass.running::after{background:var(--accent)}
  .pass.failed::after{background:var(--danger)}
  .pass[aria-pressed="true"]{outline:1px solid var(--sel);outline-offset:1px}
  .vsep{width:1px;height:22px;background:var(--line-strong);flex:none}
  .right .wb-head h2{flex:1}
  .run-pick{padding:8px 12px;border-bottom:1px solid var(--line)}.run-pick select{width:100%;font-size:12px}
  .thumbs{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:6px;padding:10px}
  .thumb{padding:0;border:0;min-height:0;background:var(--canvas);border-radius:var(--r-img);aspect-ratio:832/1216;overflow:hidden;outline:1px solid transparent;outline-offset:1px;display:grid;place-items:center}
  .thumb.cur{outline-color:var(--sel)}
  .thumb :global(img){width:100%;height:100%;object-fit:cover;display:block}
  .thumb :global(.image-placeholder),.thumb-empty{font-size:10.5px;color:var(--faint);padding:4px;text-align:center}
  .run-kv{border-top:1px solid var(--line);padding:10px 12px;font-size:12px;flex:none}
  .review-backdrop{position:fixed;inset:0;z-index:50;background:#000c;display:grid;place-items:center;padding:16px}
  .review-popup{width:min(1100px,96vw);max-height:94vh;display:flex;flex-direction:column;gap:12px;background:var(--panel);border:1px solid var(--line-strong);border-radius:6px;padding:14px}
  .review-popup header{display:flex;justify-content:space-between;align-items:baseline;gap:12px}.review-popup h2{margin:0;font-size:14px;font-weight:600}.review-popup header span{color:var(--muted);font-size:12px}
  .review-image{flex:1;min-height:0;display:grid;place-items:center;background:var(--canvas);border-radius:var(--r-img)}.review-image :global(img){max-width:100%;max-height:72vh;object-fit:contain;display:block}
  .review-actions{display:flex;gap:8px;justify-content:flex-end;flex-wrap:wrap}
</style>
