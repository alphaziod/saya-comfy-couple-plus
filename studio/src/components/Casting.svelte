<script module lang="ts">
  let castingPrefsWrite:Promise<void>=Promise.resolve();
</script>
<script lang="ts">
  import { t, tn } from '../lib/i18n';
  import {castingDate,castingTimestamp} from '../lib/casting-progress';
  import { onMount, onDestroy, tick } from 'svelte';
  import { call, message, imageSource } from '../lib/api';
  import type { GraphView } from '../lib/graph/view';
  import { randomSeed, resolveSeed } from '../lib/generation-state';
  import Thumbnail from './Thumbnail.svelte';
  import RunProgress from './RunProgress.svelte';
  import Icon from './Icon.svelte';
  import ActionDrafts from './ActionDrafts.svelte';
  let {onconfigure=()=>{},onbrowse=()=>{}}:{onconfigure?:()=>void;onbrowse?:()=>void}=$props();
  // 1-to-1 casting (V0.1): the panel leaves Generation and lives in its own menu.
  // NO workflow config here: this menu CONSUMES the preparation set in Generation
  // (generationWorkspace preference: resumed session + pending drafts applied,
  // otherwise the source file reopened) then exports it via graph_save_ui — exactly the
  // mechanism casting_lancer used when it lived in GenerationWorkspace.
  // Pass 3 (Saya feedback 2026-10-26): the YES/NO sorting disappears from the UI (the
  // casting_choices data stay in the backend). On clicking an image: TWO
  // separate and independent actions — "Enregistrer ce personnage" (casting_enregistrer_perso,
  // reusable sheet, NEVER launches passes 2-6) and "Lancer le workflow complet"
  // (casting_decide keep:true, 6 passes on THIS result, existing mechanism).
  // Cancel / Pause / Resume drive the current run (casting_stop / pause / resume);
  // the pause is honest: it stops at the end of the current attempt (no sampler suspend).
  type CastRow={stem:string;name:string;famille:string;model:string;seed:string;graine:string;run:string;state:string;asset:string|null;decision:''|'oui'|'non';shot:any;catalogue?:string;runState?:string;finalAsset?:string;phases?:any;error?:string;promptId?:string;createdAt?:string;published?:boolean;step?:boolean;released?:boolean};
  let castingFamilles=$state<string[]>([]),castingModes=$state<string[]>([]),castingGenre=$state('');
  let castingSubfamilies=$state<Record<string,string[]>>({}),castingSubfamily=$state('');
  let castingN=$state(5),castingFamille=$state(''),castingNote=$state('');
  const selectedSubfamily=$derived((castingSubfamilies[castingFamille]??[]).includes(castingSubfamily)?castingSubfamily:'');
  // Rotations (U2): Mode A "1 character × N times the same pose" (character + N),
  // Mode B "1 pose × N random characters" (fixed pose + N).
  let castingRotN=$state(2),castingPerso=$state(''),castingPoseN=$state(2),castingPoseFixe=$state('');
  // Character targeting (V1): large choice window — visual grid of the validated ones
  // (portraits of the app sheets joined by exact name to the engine registry), search
  // by name, multi-selection, validation. The chosen ids go to the engine as
  // `--persos`: EXACTLY these characters enter the casting, one shot each.
  type PersoLigne={negId:string;nom:string;slug:string;model:string;famille:string;source:string;ficheId:string|null;portrait:string|null};
  let cibler=$state(false),ciblerCharge=false,persosRows=$state<PersoLigne[]>([]),cards=$state<{id:string;portrait:string|null}[]>([]);
  let ciblerRecherche=$state(''),ciblerChoisis=$state<string[]>([]),ciblerErreur=$state('');
  // Sorts and filters V3 (first pass, iterative): views over the rows already read,
  // no extra engine command — family, checkpoint (model), worn LoRA
  // and arrival sort, in the grid as in the two large windows.
  let ciblerFamille=$state(''),ciblerModel=$state('');
  const fold=(v:string)=>v.normalize('NFD').replace(/\p{M}/gu,'').toLocaleLowerCase('fr');
  const charFamilies=$derived([...new Set(persosRows.map(p=>p.famille).filter(Boolean))].sort());
  const charModels=$derived([...new Set(persosRows.map(p=>p.model).filter(Boolean))].sort());
  const displayedTargets=$derived(
    persosRows.filter(p=>fold(p.nom).includes(fold(ciblerRecherche.trim()))&&(!ciblerFamille||p.famille===ciblerFamille)&&(!ciblerModel||p.model===ciblerModel))
      .map(l=>({...l,portrait:l.ficheId?cards.find(c=>c.id===l.ficheId)?.portrait??null:null})));
  const ciblageActif=$derived(ciblerChoisis.length>0);
  // Picker pictures: the latest casting trial of each action / character
  // (casting_previews) — the engine has no image of its own for an action.
  let imagesChoix=$state<{actions:Record<string,string>;persos:Record<string,string>}>({actions:{},persos:{}});
  // Rotations pick their character / action in the same large windows (one click = chosen).
  let choixRotation=$state<''|'perso'|'pose'>('');
  function chargerApercus(){call<typeof imagesChoix>('casting_previews').then(r=>{if(alive)imagesChoix=r}).catch(()=>{})}
  function choisirPourRotation(kind:'perso'|'pose'){choixRotation=kind;if(kind==='perso')void ouvrirCibler();else void ouvrirCiblerActions('cibler')}
  $effect(()=>{if(!cibler&&!targetActions)choixRotation=''});
  async function ouvrirCibler(){
    if(busy)return;
    busy=true;error='';ciblerErreur='';cibler=true;chargerApercus();
    try{
      if(!ciblerCharge){
        const [lignes,cartes]=await Promise.all([
          call<Omit<PersoLigne,'portrait'>[]>('casting_persos'),
          call<{id:string;portrait:string|null}[]>('character_cards').catch(()=>[]),
        ]);
        if(!alive)return;
        const portrait=(ficheId:string|null)=>ficheId?cartes.find(c=>c.id===ficheId)?.portrait??null:null;
        persosRows=lignes.map(l=>({...l,portrait:portrait(l.ficheId)}));cards=cartes;ciblerCharge=true;
      }
    }catch(e){ciblerErreur=message(e)}
    finally{busy=false}
  }
  function basculerPerso(ligne:PersoLigne){
    if(choixRotation==='perso'){castingPerso=ligne.negId;cibler=false;return}
    ciblerChoisis=ciblerChoisis.includes(ligne.negId)?ciblerChoisis.filter(id=>id!==ligne.negId):[...ciblerChoisis,ligne.negId];
  }
  function validerCiblage(){
    if(!ciblerChoisis.length)return;
    castingN=ciblerChoisis.length;
    castingNote=$tn("Ciblage actif : {n} perso choisi — ces persos entrent au casting, un shot chacun.","Ciblage actif : {n} persos choisis — ces persos entrent au casting, un shot chacun.",ciblerChoisis.length);
    cibler=false;
  }
  function retirerCiblage(){ciblerChoisis=[];castingNote='';}
  // Action targeting and EXCLUSION (V2 + pass 3): same large window, two modes.
  // "Cibler": the names go to `--actions a,b,c` (each OC carries one, in a
  // cycle). "Exclure du tirage": the names go to exclude[] of casting_lancer —
  // it reaches the REAL engine draw, not a masking. An unavailable action (old,
  // invalidated — `disponible` field of actions_lister) is no longer displayed.
  type ActionLigne={name:string;famille:string;add?:string;note?:string;lora?:string;valide?:boolean;pas_solo?:boolean;hors_test?:boolean;a_revoir?:boolean;disponible?:boolean};
  let targetActions=$state(false),actionsCharge=$state(false),actionsRows=$state<ActionLigne[]>([]);
  let actionsRecherche=$state(''),actionsChoisies=$state<string[]>([]),exclureChoisies=$state<string[]>([]),actionsErreur=$state('');
  let actionsFamille=$state(''),actionsLora=$state(''),actionsMode=$state<'cibler'|'exclure'>('cibler');
  const actionFamilies=$derived([...new Set(actionsRows.map(a=>a.famille).filter(Boolean))].sort());
  const actionLoras=$derived([...new Set(actionsRows.map(a=>a.lora).filter(Boolean))].sort());
  // Pass 3 feedback (§1): only AVAILABLE actions are visible — the `disponible`
  // field of actions_lister (backend slice: in the engine's historical order
  // AND without invalid flag hors_test/a_revoir/valide:false). Slice convergence:
  // the UI adds NO criterion to the backend field. Honest fallback if the backend
  // does not expose it yet (field absent everywhere): the old V2 rule (valide ∧ solo).
  const actionsLibre=(a:ActionLigne)=>a.valide===true&&!a.pas_solo&&!a.hors_test&&!a.a_revoir;
  const legacyBackendActions=$derived(actionsCharge&&actionsRows.length>0&&actionsRows.every(a=>a.disponible===undefined));
  const actionsUtilisable=(a:ActionLigne)=>legacyBackendActions?actionsLibre(a):a.disponible===true;
  const displayedActions=$derived(
    actionsRows.filter(a=>actionsUtilisable(a)&&fold(`${a.name} ${a.famille} ${a.note??''}`).includes(fold(actionsRecherche.trim()))&&(!actionsFamille||a.famille===actionsFamille)&&(!actionsLora||a.lora===actionsLora)));
  const actionsListeCourante=$derived(actionsMode==='cibler'?actionsChoisies:exclureChoisies);
  const ciblageActionsActif=$derived(actionsChoisies.length>0);
  const exclusionActive=$derived(exclureChoisies.length>0);
  async function ouvrirCiblerActions(mode:'cibler'|'exclure'){
    if(busy)return;
    busy=true;error='';actionsErreur='';actionsMode=mode;targetActions=true;chargerApercus();
    try{
      actionsRows=await call<ActionLigne[]>('actions_lister');if(!alive)return;actionsCharge=true;
      const disponibles=new Set(actionsRows.filter(actionsUtilisable).map(a=>a.name));
      actionsChoisies=actionsChoisies.filter(n=>disponibles.has(n));exclureChoisies=exclureChoisies.filter(n=>disponibles.has(n));
    }catch(e){actionsErreur=message(e)}
    finally{busy=false}
  }
  function basculerAction(ligne:ActionLigne){
    if(!actionsUtilisable(ligne))return;
    if(choixRotation==='pose'){castingPoseFixe=ligne.name;targetActions=false;return}
    if(actionsMode==='cibler'){
      // An action cannot be both targeted and excluded.
      exclureChoisies=exclureChoisies.filter(n=>n!==ligne.name);
      actionsChoisies=actionsChoisies.includes(ligne.name)?actionsChoisies.filter(n=>n!==ligne.name):[...actionsChoisies,ligne.name];
    }else{
      actionsChoisies=actionsChoisies.filter(n=>n!==ligne.name);
      exclureChoisies=exclureChoisies.includes(ligne.name)?exclureChoisies.filter(n=>n!==ligne.name):[...exclureChoisies,ligne.name];
    }
  }
  function validerCiblageActions(){
    const listing=actionsListeCourante;
    if(!listing.length)return;
    castingNote=actionsMode==='cibler'
      ?$tn("Ciblage actif : {n} action ciblée — chaque OC du casting portera une de ces actions (en cycle).","Ciblage actif : {n} actions ciblées — chaque OC du casting portera une de ces actions (en cycle).",listing.length)
      :$tn("Exclusion active : {n} action exclue du tirage — le moteur ne sortira pas ces actions.","Exclusion active : {n} actions exclues du tirage — le moteur ne sortira pas ces actions.",listing.length);
    targetActions=false;
  }
  function retirerCiblageActions(){actionsChoisies=[];castingNote='';}
  function retirerExclusion(){exclureChoisies=[];castingNote='';}
  const focusFenetre=(node:HTMLElement)=>node.focus();
  let castingRows=$state<CastRow[]>([]),preview=$state<CastRow|null>(null),syncError=$state('');
  let syncing=$state(false);
  let historyRevision=0;
  const labels:Record<string,string>={waiting:'En attente dans le lot',submitted:'En attente',submitting:'Préparation',queued:'En file',running:'En cours',succeeded:'Réussi',failed:'Échec',cancelled:'Annulé',interrupted:'Arrêté',submission_unknown:'À synchroniser'};
  // Casting grid (V3): filters family / checkpoint (model) / worn LoRA
  // (read from shot.casting.lora_loaders, already required by valider_shot) and
  // arrival sort. A new launch resets the filters: the fresh grid is
  // always seen in full.
  let grilleFamille=$state(''),grilleModel=$state(''),grilleLora=$state(''),grilleTri=$state('recents');
  let selectedLot=$state(''),lotFilter=$state(''),resultFilter=$state('images');
  const batchKey=(r:CastRow)=>r.catalogue||r.run;
  const lots=$derived.by(()=>{
    const grouped=new Map<string,CastRow[]>();
    for(const row of castingRows){const key=batchKey(row);grouped.set(key,[...(grouped.get(key)??[]),row]);}
    return [...grouped].map(([id,rows])=>{
      const active=rows.some(r=>r.runState==='running'&&Object.values(r.phases??{}).some((p:any)=>phaseActive(p.state)));
      const paused=rows.some(r=>r.runState==='paused');
      const done=rows.filter(r=>r.state==='succeeded'||!!r.asset).length;
      const pending=rows.some(r=>r.runState==='stopped'&&Object.values(r.phases??{}).some((p:any)=>phaseActive(p.state)));
      const retry=rows.find(r=>r.runState==='paused'||['cancelled','failed','interrupted'].includes(r.state));
      return {id,rows,createdAt:rows[0].createdAt??'',done,remaining:rows.length-done,cancelled:rows.filter(r=>r.state==='cancelled'||r.state==='interrupted').length,
        state:active?'active':paused?'paused':done<rows.length?'incomplete':'completed',retry:pending||done===rows.length?'':retry?.run??''};
    }).sort((a,b)=>(castingTimestamp(b.createdAt)??0)-(castingTimestamp(a.createdAt)??0));
  });
  const filteredLots=$derived(lots.filter(l=>!lotFilter||l.state===lotFilter));
  const visibleLots=$derived(filteredLots);
  const selectedLotId=$derived(lots.some(l=>l.id===selectedLot)?selectedLot:lots[0]?.id??'');
  const selectedRows=$derived(castingRows.filter(r=>batchKey(r)===selectedLotId));
  const lorasDuShot=(s:any):string[]=>{
    const out:string[]=[];
    for(const loader of Object.values(s?.casting?.lora_loaders??{}) as any[])
      for(const l of loader?.loras??[]) if(l?.name&&!out.includes(l.name)) out.push(l.name);
    return out;
  };
  const gridFamilies=$derived([...new Set(castingRows.map(r=>r.famille).filter(Boolean))].sort());
  const gridModels=$derived([...new Set(castingRows.map(r=>r.model).filter(Boolean))].sort());
  const gridLoras=$derived([...new Set(castingRows.flatMap(r=>lorasDuShot(r.shot)))].sort());
  const grilleAffichees=$derived.by(()=>{
    const vues=selectedRows.filter(r=>(resultFilter==='all'||resultFilter==='images'&&!!r.asset||resultFilter==='cancelled'&&['cancelled','interrupted'].includes(r.state)||resultFilter==='pending'&&phaseActive(r.state))&&(!grilleFamille||r.famille===grilleFamille)&&(!grilleModel||r.model===grilleModel)&&(!grilleLora||lorasDuShot(r.shot).includes(grilleLora)));
    return grilleTri==='anciens'?[...vues].reverse():vues;
  });
  function resetResultFilters(){grilleFamille='';grilleModel='';grilleLora='';resultFilter='images'}
  function resetGridFilters(){selectedLot='';resultFilter='images';grilleFamille='';grilleModel='';grilleLora='';grilleTri='recents'}
  // Scripts (pass 3 §7): the REAL modes of the fish function generez, encoded
  // EXACTLY like ENGINE_SCRIPTS in the backend (slice B): `attente`, `poses`
  // (id_perso required), `lcs`, `duo`, `bny20`. The engine handles its GPU submission
  // itself; the returned log (stdout/stderr, code, delay) is displayed as is —
  // never a fake "test" mode (Saya feedback: casting runs the real engine).
  const fishScripts=[
    {script:'attente',label:"Tests d'attente · 30 steps (--gpu)"},
    {script:'poses',label:"Poses d'un perso · ID requis"},
    {script:'lcs',label:'LCS · SOLO_LOT=lcs (--reprise)'},
    {script:'duo',label:'Duo (--gpu)'},
    {script:'bny20',label:'bny20 (--gpu)'},
  ];
  let scriptChoisi=$state(''),scriptPerso=$state(''),scriptSortie=$state(''),scriptNote=$state(''),scriptErreur=$state(''),scriptEnVol=$state(false);
  async function launchScript(){
    if(busy||scriptEnVol||!scriptChoisi)return;
    if(scriptChoisi==='poses'&&!scriptPerso.trim()){scriptErreur=$t("Poses d'un perso : donner l'ID du perso (CH--9009, CH-09999…).");return}
    scriptEnVol=true;scriptErreur='';scriptSortie='';
    scriptNote=scriptChoisi==='poses'?$t("Script réel « {script} » lancé sur {perso} — le moteur gère sa soumission GPU lui-même ; le journal s'affiche ici.",{script:scriptChoisi,perso:scriptPerso.trim()}):$t("Script réel « {script} » lancé — le moteur gère sa soumission GPU lui-même ; le journal s'affiche ici.",{script:scriptChoisi});
    try{
      const args:{script:string;id_perso?:string}=scriptChoisi==='poses'?{script:scriptChoisi,id_perso:scriptPerso.trim()}:{script:scriptChoisi};
      const r=await call<any>('engine_script',args);
      const journal=[r?.stdout,r?.stderr].filter((x:any)=>typeof x==='string'&&x.trim()).join('\n');
      scriptSortie=journal||$t("(aucune sortie capturée)");
      scriptNote=r?.timeout
        ?$t("Script interrompu au délai · journal partiel ci-dessous (jamais transformé en succès).")
        :r?.success
        ?$t("Script terminé (code 0) · sortie du moteur ci-dessous (suivi honnête : aucune progression inventée).")
        :$t("Script terminé en échec (code {code}) · journal ci-dessous — un échec reste un échec.",{code:r?.code??'?'});
    }catch(e){scriptErreur=message(e);scriptNote=''}
    finally{scriptEnVol=false}
  }
  let session=$state(''),view=$state<GraphView|null>(null),sourceName=$state('');
  let drafts=$state<Record<string,string>>({}),scopeDrafts=$state<Record<string,{path:string[];node:string;input:string;value:string}>>({});
  let seed=$state('');
  let busy=$state(false),error=$state(''),note=$state('');
  let alive=true,initialization=Promise.resolve();
  let castingTimer:ReturnType<typeof setInterval>|null=null;
  let preparation=$state<{active:boolean;stage?:string;completed?:number;total?:number;elapsedSeconds?:number}>({active:false}),preparationError=$state('');
  let preparationTimer:ReturnType<typeof setInterval>|null=null,preparationPolling=false;
  const preparing=$derived(preparation.active);
  async function readPreparation(){
    if(!alive||preparationPolling)return;preparationPolling=true;
    try{const value=await call<typeof preparation>('casting_preparation_status');if(!alive)return;const finished=preparation.active&&!value.active;preparation=value;preparationError='';if(finished&&!busy)void syncCasting();}
    catch(e){if(alive)preparationError=message(e)}finally{preparationPolling=false}
  }

  const phaseActive=(etat:string)=>['waiting','submitted','submitting','queued','running','submission_unknown'].includes(etat);
  const phaseFinished=(etat:string)=>['succeeded','failed','cancelled','interrupted'].includes(etat);
  const activeRows=$derived(castingRows.filter(r=>r.runState!=='stopped'));
  // Between two passes of a full workflow no phase is active yet (the next one is added
  // by the monitor): keep refreshing while a kept trial's run is still running.
  const finishing=$derived(castingRows.some(r=>r.decision==='oui'&&r.runState==='running'&&!!r.phases?.['2']));
  const pendingStops=$derived(castingRows.filter(r=>r.runState==='stopped'&&Object.values(r.phases??{}).some((p:any)=>phaseActive(p.state))));
  // An unreleased trial of a step-by-step batch waits for the user, it is not in flight.
  const held=(r:CastRow)=>r.step===true&&r.released!==true;
  let castingEnVol=$derived(activeRows.some(r=>!held(r)&&Object.values(r.phases??{'1':{state:r.state}}).some((p:any)=>phaseActive(p.state))));
  // Current run (pass 3 §3/§6): the most recent batch is recognized by its catalog
  // (each casting_lancer writes its own). Global bar = attempts done / total,
  // current attempt and pass, never an invented percentage.
  const batchRef=$derived(String((activeRows.find(r=>r.runState==='paused'||Object.values(r.phases??{}).some((p:any)=>phaseActive(p.state)))??activeRows[0])?.catalogue??''));
  const batchRows=$derived(batchRef?activeRows.filter(r=>r.catalogue===batchRef):[]);
  const batchInFlight=$derived(batchRows.some(r=>phaseActive(r.state)));
  const batchFinitions=$derived(batchRows.filter(r=>r.decision==='oui'&&r.runState==='running'&&r.phases?.['2']));
  const batchDone=$derived(batchRows.filter(r=>phaseFinished(r.state)).length);
  const runBarVisible=$derived(batchRows.length>0&&(batchInFlight||batchFinitions.length>0||batchRows.some(r=>r.runState==='paused')));
  const runCourantId=$derived(batchRows.find(r=>phaseActive(r.state))?.run??batchRows.find(r=>r.runState==='paused')?.run??batchFinitions[0]?.run??'');
  const progressRow=$derived(batchRows.find(r=>Object.values(r.phases??{}).some((p:any)=>['submitted','submitting','queued','running'].includes(p.state)))??batchRows.find(r=>phaseActive(r.state)));
  const runEnPause=$derived(batchRows.some(r=>r.runState==='paused'));
  // The trial really handed to ComfyUI first; a merely waiting one only when none is.
  const executing=(r:CastRow)=>Object.values(r.phases??{}).some((p:any)=>['submitted','submitting','queued','running'].includes(p.state));
  const currentAttempt=$derived(batchRows.find(executing)?.name??batchRows.find(r=>!held(r)&&phaseActive(r.state))?.name??batchRows.find(r=>r.runState==='paused')?.name??'');
  // Nothing of this batch on ComfyUI but another run is: say what the batch waits for.
  const waitingFor=$derived.by(()=>{
    if(batchRows.some(executing))return '';
    const other=activeRows.find(r=>r.catalogue!==batchRef&&executing(r));
    if(!other)return '';
    const pass=[1,2,3,4,5,6].find(n=>['submitted','submitting','queued','running'].includes(other.phases?.[String(n)]?.state))??1;
    return other.decision==='oui'?$t("En file : workflow complet de {name} en cours (passe {n}/6)",{name:other.name,n:pass}):$t("En file : essai {name} en cours",{name:other.name});
  });
  const currentPass=$derived.by(()=>{
    const fin=batchFinitions[0];
    if(fin){
      const inProgress=[2,3,4,5,6].find(n=>fin.phases?.[String(n)]?.state!=='succeeded')??6;
      return $t("Finition · passe {n}/6",{n:inProgress});
    }
    return $t("Passe 1 · essais du casting");
  });
  // Current run controls: casting_stop / casting_pause / casting_resume contract.
  async function actionRun(action:'stop'|'pause'|'resume',runId=runCourantId){await task(async()=>{
    if(!runId)throw Error($t("Aucun run de casting en cours."));
    const result=await call<{runs?:any[];online?:boolean}>(action==='stop'?'casting_stop':action==='pause'?'casting_pause':'casting_resume',{run:runId});
    const failures=(result.runs??[]).flatMap(r=>(r.controles??[]).filter((c:any)=>c.envoye===false).map((c:any)=>c.erreur));

    castingNote=action==='stop'
      ?$t("Annulation demandée : l’arrêt du calcul ComfyUI est demandé et les essais en attente sont annulés.")
      :action==='pause'
      ?$t("Pause demandée : aucun nouvel essai ne sera mis en file après l'essai en cours.")
      :$t("Reprise : la file des essais restants repart, ceux déjà obtenus ne sont pas recommencés.");
    if(result.online===false)castingNote=$t("Essais locaux annulés. ComfyUI est indisponible ; l’arrêt distant reste à confirmer.");
    else if(failures.length)castingNote=$t("Essais locaux annulés. {detail}",{detail:[...new Set(failures.map(message))].join(' · ')});
    if(alive){castingRows=await call<CastRow[]>('casting_history',{sync:false});syncError=''};
  })}
  // Image detail (pass 3 §4): two SEPARATE and independent actions.
  // Saving goes outside task(): its errors must stay READABLE in the
  // modal window (the global error banner is behind the backdrop).
  let saveName=$state(''),enregistrerNote=$state(''),zoom=$state(false);
  // Casting -> gallery: explicit validation of one or more images; the
  // backend marks the representative image without copying (no duplicate possible).
  let choix=$state<string[]>([]);
  async function publier(runs:string[]){if(!runs.length)return;await task(async()=>{const r=await call<{published:number}>('casting_publish',{runs});castingNote=$tn("{n} image validée dans la galerie","{n} images validées dans la galerie",runs.length)+(r.published<runs.length?' '+$tn("({n} déjà présente)","({n} déjà présentes)",runs.length-r.published):'')+'.';choix=[];castingRows=await call<CastRow[]>('casting_history',{sync:false});if(preview)preview=castingRows.find(x=>x.run===preview!.run&&x.stem===preview!.stem)??preview})}
  async function supprimerLot(run:string,images:number){if(!confirm($tn("Supprimer ce lot du casting ({n} image) ? Les images validées dans la galerie ou enregistrées comme personnage sont conservées ; les autres sont effacées du disque.","Supprimer ce lot du casting ({n} images) ? Les images validées dans la galerie ou enregistrées comme personnage sont conservées ; les autres sont effacées du disque.",images)))return;await task(async()=>{const r=await call<{removedRuns:number;archivedRuns:number;removedFiles:number}>('casting_delete_lot',{run});castingNote=$t("Lot supprimé · {runs} essai(s) et {files} fichier(s) effacés · {archived} conservé(s) en galerie.",{runs:r.removedRuns,files:r.removedFiles,archived:r.archivedRuns});choix=[];castingRows=await call<CastRow[]>('casting_history',{sync:false});selectedLot=''})}
  function ouvrirApercu(row:CastRow){zoom=false;saveName='';enregistrerNote='';focusKey=rowKey(row);preview=row}
  // Preview: navigation among the displayed images, neighbours preloaded in the
  // preview cache (no re-decoding on previous/next).
  const apercus=$derived(grilleAffichees.filter(r=>!!r.asset));
  const apercuIndex=$derived(preview?apercus.findIndex(r=>r.run===preview!.run&&r.stem===preview!.stem):-1);
  function apercuVoisin(delta:number){const next=apercus[apercuIndex+delta];if(next)ouvrirApercu(next)}
  $effect(()=>{if(apercuIndex<0)return;for(const r of [apercus[apercuIndex-1],apercus[apercuIndex+1]])if(r)void imageSource(r.finalAsset||r.asset!,true).catch(()=>{})});
  async function saveCharacter(row:CastRow){
    if(busy)return;
    busy=true;error='';
    try{
      await initialization;if(!alive)return;
      if(!row.asset)throw Error($t("Attendre l'image de la passe 1 avant d'enregistrer le personnage."));
      const nom=saveName.trim();
      if(!nom){enregistrerNote=$t("Donner un nom à la fiche personnage.");return}
      const r=await call<{characterId:string}>('casting_enregistrer_perso',{run:row.run,stem:row.stem,nom});
      enregistrerNote=$t("Fiche personnage enregistrée : {nom} ({id}) — réutilisable dans Personnages. Le workflow complet n'est PAS lancé.",{nom,id:r.characterId});
    }catch(e){enregistrerNote=$t("Enregistrement impossible : {detail}",{detail:message(e)})}
    finally{if(alive)busy=false}
  }
  async function launchFull(row:CastRow){await task(async()=>{
    if(!row.asset)throw Error($t("Attendre l'image de la passe 1 avant de lancer le workflow complet."));
    const rows=await call<CastRow[]>('casting_decide',{run:row.run,keep:true});
    if(alive){castingRows=rows;preview=null;castingNote=$t("Workflow complet lancé sur {name} · les passes 2 à 6 s'enchaînent sur CE résultat (seed et réglages préservés).",{name:row.name})}
  })}
  const key=(id:string,input:string)=>`${id}\u0000${input}`;
  async function flush(){
    if(!session)return;
    const master=view?.document.nodes.find(n=>n.classType==='Seed (rgthree)');
    if(master&&/^\d+$/.test(seed.trim())&&view?.paramText[master.id]?.seed!==seed.trim()){
      const value=resolveSeed('fixed',seed,'');drafts={...drafts,[key(master.id,'seed')]:value};
    }
    const sid=session;
    for(const [k,value] of Object.entries(drafts)){if(!alive)return;const [node,input]=k.split('\u0000');view=await call<GraphView>('graph_edit',{session:sid,op:{op:'set_param',node,input,value}});if(drafts[k]===value){const copy={...drafts};delete copy[k];drafts=copy}}
    for(const [k,edit] of Object.entries(scopeDrafts)){if(!alive)return;const r=await call<{root:GraphView;view:GraphView}>('graph_scoped_widget',{session:sid,path:edit.path,edit:{op:'set_param',node:edit.node,input:edit.input,value:edit.value}});view=r.root;const copy={...scopeDrafts};delete copy[k];scopeDrafts=copy}
  }
  async function task(fn:()=>Promise<void>){if(busy)return;historyRevision++;busy=true;error='';try{await initialization;if(!alive)return;await fn()}catch(e){if(alive){error=message(e);try{castingRows=await call<CastRow[]>('casting_history',{sync:false})}catch{}}}finally{if(alive)busy=false}}
  // draftTest: test of action drafts (Brouillons), drawn from the engine test copy.
  async function lancerCasting(draftTest:{ids:string[];n:number}|null=null){await task(async()=>{
    if(!session)throw Error($t("Règle ta config dans Generation d'abord : ouvre ton workflow là-bas, puis reviens lancer le casting ici."));
    await flush();if(!alive)return;resetGridFilters();
    const graine=randomSeed();
    const exported=await call<{path:string}>('graph_save_ui',{session,name:`${sourceName||'Casting'} · base passe 1`});
    if(!alive)return;castingNote=$t("Le moteur tire le casting… (jusqu'à quelques minutes, aucune image envoyée par le moteur)");
    const cibles=draftTest?null:ciblageActif?ciblerChoisis:null;
    const visees=draftTest?draftTest.ids:ciblageActionsActif?actionsChoisies:null;
    // Exclude[]: reaches the REAL engine draw. Backend semantics: free draw,
    // fixed pose and targeted actions accept it (exclusion wins over --actions);
    // it is refused with character targeting — the buttons are already exclusive,
    // this guard makes the refusal readable if it were bypassed.
    if(exclusionActive&&cibles)throw Error($t("Exclusion d'actions : incompatible avec le ciblage de persos — retire le ciblage pour exclure des actions du tirage."));
    const exclues=!draftTest&&exclusionActive?exclureChoisies:null;
    const r=await call<any>('casting_lancer',{graine,subfamily:cibles||draftTest?null:(selectedSubfamily||null),genre:cibles?null:(castingGenre||null),nombre:cibles?cibles.length:draftTest?draftTest.n:Math.max(1,Number(castingN)||5),famille:castingFamille||null,source_path:exported.path,...(cibles?{persos:cibles}:{}),...(visees?{actions:visees}:{}),...(exclues?{exclude:exclues}:{}),...(draftTest?{drafts:true}:{})});
    if(!alive)return;castingRows=await call<CastRow[]>('casting_history',{sync:false});
    castingNote=cibles
      ?$tn("{n} perso ciblé en passe 1 seule · graine moteur {seed} · config « {config} »","{n} persos ciblés en passe 1 seule · graine moteur {seed} · config « {config} »",r.shots.length,{seed:r.graine,config:sourceName||'Generation'})
      :visees
      ?$t("{n} OC en passe 1 seule · actions ciblées : {actions} · graine moteur {seed} · config « {config} »",{n:r.shots.length,actions:visees.join(', '),seed:r.graine,config:sourceName||'Generation'})
      :$t("{n} OC en passe 1 seule · graine moteur {seed} · famille {famille}{exclues} · config « {config} »",{n:r.shots.length,seed:r.graine,famille:castingFamille||$t("tirage normal"),exclues:exclues?' · '+$tn("{n} action exclue du tirage","{n} actions exclues du tirage",exclues.length):'',config:sourceName||'Generation'});
    void syncCasting();
  })}
  // "Une par une": a step-by-step batch of STEP_BATCH drawn trials, only one released at a
  // time. The current trial is the last released one; Full sends it through passes 2-6,
  // Suivante releases the next (a new batch is drawn when this one is exhausted).
  const STEP_BATCH=20;
  let stepView=$state(false);
  const stepRows=$derived(castingRows.filter(r=>r.step===true&&r.runState!=='stopped'));
  const stepCurrent=$derived([...stepRows].filter(r=>r.released).sort((a,b)=>Number(a.shot?.index??0)-Number(b.shot?.index??0)).filter(r=>r.catalogue===stepRows.find(x=>x.released)?.catalogue).at(-1)??null);
  const stepLatest=$derived(stepRows.filter(r=>r.released).at(0)??null);
  async function stepStart(){
    if(!session)throw Error($t("Règle ta config dans Generation d'abord : ouvre ton workflow là-bas, puis reviens lancer le casting ici."));
    await flush();if(!alive)return;
    const exported=await call<{path:string}>('graph_save_ui',{session,name:`${sourceName||'Casting'} · une par une`});
    if(!alive)return;
    await call<any>('casting_lancer',{graine:randomSeed(),subfamily:selectedSubfamily||null,genre:castingGenre||null,nombre:STEP_BATCH,famille:castingFamille||null,source_path:exported.path,step:true});
    if(!alive)return;castingRows=await call<CastRow[]>('casting_history',{sync:false});stepView=true;
    castingNote=$t("Une par une : la première image se génère. Full = 6 passes, Suivante = une nouvelle image.");
  }
  async function stepLaunch(){await task(stepStart)}
  async function stepNext(){await task(async()=>{
    const current=stepCurrent??stepLatest;
    if(!current){await stepStart();return}
    try{const r=await call<{rows:CastRow[]}>('casting_next',{run:current.run});if(alive)castingRows=r.rows}
    catch(e){if(/épuisé/.test(message(e)))await stepStart();else throw e}
  })}
  async function stepFull(){const row=stepCurrent;if(!row||!row.asset||row.decision==='oui')return;await launchFull(row)}
  async function stepQuit(){const row=stepCurrent??stepLatest;stepView=false;if(row)await actionRun('stop',row.run)}
  function stepKeys(e:KeyboardEvent){
    if(!stepView||busy||(e.target as HTMLElement)?.closest?.('input,textarea,select'))return;
    if(e.key==='f'||e.key==='F'){e.preventDefault();void stepFull()}
    else if(e.key==='n'||e.key==='N'||e.key==='ArrowRight'){e.preventDefault();void stepNext()}
    else if(e.key==='Escape'){stepView=false}
  }
  // mode 'actions': the chosen character on every validated action (or the targeted ones),
  // one trial per action, drawn by the engine's --poses mode through the bridge.
  async function lancerRotations(mode:'perso'|'pose'|'actions',valeur:string){await task(async()=>{
    if(!session)throw Error($t("Règle ta config dans Generation d'abord : ouvre ton workflow là-bas, puis reviens lancer le casting ici."));
    await flush();if(!alive)return;resetGridFilters();
    const graine=randomSeed();
    const exported=await call<{path:string}>('graph_save_ui',{session,name:`${sourceName||'Casting'} · base passe 1`});
    if(!alive)return;castingNote=$t("Le moteur tire les rotations… (jusqu'à quelques minutes, aucune image envoyée par le moteur)");
    const n=mode==='actions'?Math.max(1,actionsChoisies.length):Math.max(1,Number(mode==='perso'?castingRotN:castingPoseN)||2);
    // Rotations: the exclusion combines with the fixed pose (Mode B) but is refused
    // by the backend with character rotation (Mode A) — never sent there.
    const exclues=mode==='pose'&&exclusionActive?exclureChoisies:null;
    const r=await call<any>('casting_lancer',{graine,subfamily:mode==='pose'?(selectedSubfamily||null):null,genre:mode==='pose'?(castingGenre||null):null,nombre:n,famille:castingFamille||null,source_path:exported.path,...(mode==='pose'?{pose_fixe:valeur}:{perso:valeur}),...(mode==='actions'?{all_actions:true,...(actionsChoisies.length?{actions:actionsChoisies}:{})}:{}),...(exclues?{exclude:exclues}:{})});
    if(!alive)return;castingRows=await call<CastRow[]>('casting_history',{sync:false});
    castingNote=$tn("{n} rotation en passe 1 seule · graine moteur {seed} · {cible}{famille}","{n} rotations en passe 1 seule · graine moteur {seed} · {cible}{famille}",r.shots.length,{seed:r.graine,cible:mode==='pose'?$t("pose {valeur}",{valeur}):$t("perso {valeur}",{valeur}),famille:castingFamille&&mode==='pose'?' · '+$t("famille {famille}",{famille:castingFamille}):''});
    void syncCasting();
  })}
  async function syncCasting(){
    if(!alive||syncing||busy)return;syncing=true;const revision=historyRevision;
    try{const rows=await call<CastRow[]>('casting_history',{sync:true});if(alive&&revision===historyRevision){castingRows=rows;syncError=''}}
    catch(e){if(alive&&revision===historyRevision)syncError=message(e)}finally{syncing=false}
  }
  const showDialog=(node:HTMLDialogElement)=>{node.showModal();return{destroy:()=>node.close()}};
  // Persistence (pass 3 §7): EVERY option change is saved immediately
  // (set_preference castingWorkspace), not only on component destruction.
  // Returning to the page = exact state restored; the preference is re-read on mount.
  let prefsLoaded=$state(false),dernierePrefs='';
  function savePrefs(value:string){castingPrefsWrite=castingPrefsWrite.catch(()=>{}).then(()=>call('set_preference',{key:'castingWorkspace',value:JSON.parse(value)})).then(()=>{}).catch(e=>{if(alive)syncError=$t("Options non enregistrées : {detail}",{detail:message(e)})});}
  const prefsSnapshot=$derived(JSON.stringify({castingSubfamily,selectedLot,resultFilter,nombre:castingN,famille:castingFamille,persos:ciblerChoisis,actions:actionsChoisies,exclues:exclureChoisies,perso:castingPerso,pose:castingPoseFixe,rotN:castingRotN,poseN:castingPoseN,grilleFamille:grilleFamille,grilleModel:grilleModel,grilleLora:grilleLora,grilleTri:grilleTri,script:scriptChoisi,castingGenre,scriptPerso,ciblerRecherche,ciblerFamille,ciblerModel,actionsRecherche,actionsFamille,actionsLora,actionsMode}));
  $effect(()=>{
    // The reactive snapshot is read BEFORE any early return: a `return` before the
    // read would leave the effect without dependency, hence never re-run (the
    // immediate persistence would be dead — pass 3 smoke: "tri persisté").
    const valeur=prefsSnapshot;
    if(!prefsLoaded)return;
    if(valeur===dernierePrefs)return;
    dernierePrefs=valeur;
    savePrefs(valeur);
  });
  onMount(()=>{
    void readPreparation();preparationTimer=setInterval(()=>void readPreparation(),1000);
    castingTimer=setInterval(()=>{if(!busy&&(castingEnVol||pendingStops.length>0||finishing))void syncCasting()},3000);
    initialization=(async()=>{try{
      await castingPrefsWrite;
      const [prefs,history]=await Promise.all([call<any>('preferences'),call<CastRow[]>('casting_history',{sync:false})]);if(!alive)return;castingRows=history;
      const saved=prefs.castingWorkspace;
      if(saved){
        castingSubfamily=saved.castingSubfamily??'';
        selectedLot=saved.selectedLot??'';resultFilter=['images','all','cancelled','pending'].includes(saved.resultFilter)?saved.resultFilter:'images';
        castingN=saved.nombre??5;castingFamille=saved.famille??'';ciblerChoisis=saved.persos??[];
        actionsChoisies=saved.actions??[];exclureChoisies=saved.exclues??[];
        castingPerso=saved.perso??'';castingPoseFixe=saved.pose??'';castingRotN=saved.rotN??2;castingPoseN=saved.poseN??2;
        grilleFamille=saved.grilleFamille??'';grilleModel=saved.grilleModel??'';grilleLora=saved.grilleLora??'';grilleTri=saved.grilleTri??'recents';
        castingGenre=saved.castingGenre??'';scriptChoisi=saved.script??'';scriptPerso=saved.scriptPerso??'';
        ciblerRecherche=saved.ciblerRecherche??'';ciblerFamille=saved.ciblerFamille??'';ciblerModel=saved.ciblerModel??'';
        actionsRecherche=saved.actionsRecherche??'';actionsFamille=saved.actionsFamille??'';actionsLora=saved.actionsLora??'';actionsMode=saved.actionsMode==='exclure'?'exclure':'cibler';
      }
      dernierePrefs=prefsSnapshot;prefsLoaded=true;
      const capabilities=await call<{families:string[];modes:string[];subfamilies?:Record<string,string[]>}>('casting_capabilities');
      if(!alive)return;castingFamilles=capabilities.families;castingModes=capabilities.modes;castingSubfamilies=capabilities.subfamilies??{};
      const state=prefs.generationWorkspace;
      if(state?.session){
        const r=await call<{session:string;view:GraphView}>('graph_resume',{id:state.session});
        if(!alive)return;session=r.session;view=r.view;sourceName=state.name??'Préparation';
        drafts=state.drafts??{};scopeDrafts=state.scopeDrafts??{};seed=state.seed??'';
        note=$t("Config consommée · préparation « {name} » de Generation.",{name:sourceName});
        await flush();
      }else if(state?.sourcePath){
        const r=await call<{session:string;view:GraphView}>('graph_open_file',{path:state.sourcePath});
        if(!alive)return;session=r.session;view=r.view;sourceName=state.sourcePath.split('/').pop()?.replace(/\.json$/,'')??'Préparation';
        note=$t("Config consommée · workflow « {name} » de Generation.",{name:sourceName});
      }else{
        note=$t("Règle ta config dans Generation d'abord : ouvre ton workflow Saya là-bas, puis reviens ici.");
      }
    }catch(e){error=message(e)}})();
  });
  onDestroy(()=>{alive=false;if(preparationTimer)clearInterval(preparationTimer);if(castingTimer)clearInterval(castingTimer);if(prefsLoaded)savePrefs(prefsSnapshot)});
  // V1 presentation (contact sheet): focus of a tile (inspector on the right),
  // keyboard navigation ←/→, Space = selection, Enter = large preview. Tile
  // size: local comfort (localStorage), never an engine preference.
  const rowKey=(r:CastRow)=>r.run+':'+r.stem;
  let focusKey=$state('');
  const focused=$derived(grilleAffichees.find(r=>rowKey(r)===focusKey)??grilleAffichees.find(r=>!!r.asset)??grilleAffichees[0]??null);
  const selLot=$derived(lots.find(l=>l.id===selectedLotId)??null);
  const passesFaites=(r:CastRow)=>[1,2,3,4,5,6].filter(n=>r.phases?.[String(n)]?.state==='succeeded').length;
  const lotEtat:Record<string,string>={active:'En cours',paused:'En pause',incomplete:'Incomplet',completed:'Terminé'};
  function focaliser(row:CastRow){if(focusKey===rowKey(row))return;focusKey=rowKey(row);saveName='';enregistrerNote=''}
  function toggleChoice(run:string){choix=choix.includes(run)?choix.filter(r=>r!==run):[...choix,run]}
  async function tuileClavier(e:KeyboardEvent,row:CastRow){
    if(e.key==='ArrowRight'||e.key==='ArrowLeft'){
      e.preventDefault();
      const i=grilleAffichees.findIndex(r=>rowKey(r)===rowKey(row)),next=grilleAffichees[i+(e.key==='ArrowRight'?1:-1)];
      if(!next)return;focaliser(next);await tick();
      (document.querySelector(`[data-tile-key="${CSS.escape(rowKey(next))}"] .tile-img`) as HTMLElement|null)?.focus();
    }else if(e.key===' '){e.preventDefault();focaliser(row);if(row.asset&&!row.published&&!busy)toggleChoice(row.run)}
    else if(e.key==='Enter'){e.preventDefault();focaliser(row);if(row.asset)ouvrirApercu(row)}
  }
  let tile=$state(170);
  try{const v=Number(localStorage.getItem('saya.casting.tile'));if(v>=110&&v<=360)tile=v}catch{}
  $effect(()=>{const v=String(tile);try{localStorage.setItem('saya.casting.tile',v)}catch{}});
  const fermerMenu=(e:Event)=>(e.currentTarget as HTMLElement).closest('details')?.removeAttribute('open');
</script>
<svelte:window onkeydown={stepKeys}/>
<section class="wb casting-wb" data-casting-panel style="--wb-left:280px;--wb-right:340px">
  <aside class="wb-panel left">
    <div class="wb-head"><h2>{$t("Nouveau lot")}</h2></div>
    <div class="wb-scroll">
      <div class="new-lot">
        <label class="field-row"><span>{$t("Essais")}</span><input class="num" data-casting-n type="number" min="1" bind:value={castingN} disabled={busy||preparing||castingEnVol||ciblageActif}/></label>
        <label class="field-row"><span>{$t("Famille")}</span><select data-casting-famille bind:value={castingFamille} disabled={busy||preparing||castingEnVol||ciblageActif}><option value="">{$t("Tirage normal")}</option>{#each castingFamilles as f}<option value={f}>{f}</option>{/each}</select></label>
        {#if castingSubfamilies[castingFamille]?.length}<label class="field-row"><span>{$t("Sous-famille")}</span><select data-casting-subfamily bind:value={castingSubfamily} disabled={busy||preparing||castingEnVol||ciblageActif}><option value="">{$t("Tirage normal")}</option>{#each castingSubfamilies[castingFamille] as sub}<option value={sub}>{sub}</option>{/each}</select></label>{/if}
        <label class="field-row"><span>{$t("Genre")}</span><select data-casting-genre bind:value={castingGenre} disabled={busy||preparing||castingEnVol||ciblageActif}><option value="">{$t("Défaut du moteur")}</option>{#each castingModes as mode}<option value={mode}>{$t(({girls:'Femme',futa:'Futa',both:'Les deux'} as Record<string,string>)[mode]??mode)}</option>{/each}</select></label>
        <div class="field-row top"><span>{$t("Ciblage")}</span>
          <div class="targets">
            <button class="quiet" data-casting-cibler onclick={ouvrirCibler} disabled={busy||preparing||castingEnVol||ciblageActionsActif||exclusionActive}>{$t("Cibler des personnages")}</button>
            {#if ciblageActif}<span class="target-chip" data-casting-ciblage>{$tn("{n} perso ciblé","{n} persos ciblés",ciblerChoisis.length)}<button class="quiet icon-btn" data-casting-ciblage-retirer onclick={retirerCiblage} title={$t("Retirer le ciblage : retour au tirage aléatoire")} aria-label={$t("Retirer le ciblage : retour au tirage aléatoire")}><Icon name="close" size={12}/></button></span>{/if}
            <button class="quiet" data-casting-cibler-actions onclick={()=>ouvrirCiblerActions('cibler')} disabled={busy||preparing||castingEnVol||ciblageActif}>{$t("Cibler des actions")}{ciblageActionsActif?` (${actionsChoisies.length})`:''}</button>
            {#if ciblageActionsActif}<span class="target-chip" data-casting-actions-cible data-actions-compte={actionsChoisies.length}>{$tn("{n} action ciblée","{n} actions ciblées",actionsChoisies.length)}<button class="quiet icon-btn" data-casting-actions-retirer onclick={retirerCiblageActions} title={$t("Retirer le ciblage : retour au tirage d'actions normal")} aria-label={$t("Retirer le ciblage : retour au tirage d'actions normal")}><Icon name="close" size={12}/></button></span>{/if}
            <button class="quiet" data-casting-exclure-actions onclick={()=>ouvrirCiblerActions('exclure')} disabled={busy||preparing||castingEnVol||ciblageActif}>{$t("Exclure des actions")}{exclusionActive?` (${exclureChoisies.length})`:''}</button>
            {#if exclusionActive}<span class="target-chip exclusion" data-casting-actions-exclues data-actions-exclues-compte={exclureChoisies.length}>{$tn("{n} action exclue","{n} actions exclues",exclureChoisies.length)}<button class="quiet icon-btn" data-casting-actions-exclues-retirer onclick={retirerExclusion} title={$t("Retirer l'exclusion : ces actions reviennent dans le tirage")} aria-label={$t("Retirer l'exclusion : ces actions reviennent dans le tirage")}><Icon name="close" size={12}/></button></span>{/if}
          </div>
        </div>
        <button class="primary launch" data-casting-lancer onclick={()=>lancerCasting()} disabled={busy||preparing||!session||castingEnVol}>{castingEnVol?$t("Casting en cours…"):$tn("Lancer {n} essai","Lancer {n} essais",ciblageActif?ciblerChoisis.length:Math.max(1,Number(castingN)||5))}</button>
        <button class="launch" data-casting-step-start onclick={()=>stepRows.length&&!stepView?(stepView=true):stepLaunch()} disabled={busy||preparing||!session||(castingEnVol&&!stepRows.length)} title={$t("Une image à la fois : tu choisis Full (6 passes) ou Suivante")}>{stepRows.length&&!stepView?$t("Reprendre « une par une »"):$t("Une par une")}</button>
        {#if !preparing}<p class="casting-note" role="status">{castingNote}</p>{/if}
        <p class="source"><span data-casting-source title={note}>{note}</span><button class="quiet icon-btn" data-casting-config onclick={onconfigure} title={$t("Réglages dans Génération")} aria-label={$t("Réglages dans Génération")}><Icon name="settings" size={14}/></button></p>
      </div>
      <details class="sec"><summary><span class="sec-title">{$t("Rotations · rejouer un personnage ou une action")}</span></summary>
        <div class="sec-body" data-casting-rotations>
          <div class="inline choix-rot"><button class="quiet" data-casting-rot-choisir onclick={()=>choisirPourRotation('perso')} disabled={busy||preparing||castingEnVol} title={$t("Rotations · 1 perso × N fois la même pose")}>{castingPerso.trim()?castingPerso.trim():$t("Choisir un perso…")}</button>{#if castingPerso.trim()}<span class="target-chip" data-casting-rot-perso={castingPerso.trim()}>{castingPerso.trim()}<button class="quiet icon-btn" onclick={()=>castingPerso=''} disabled={busy||preparing||castingEnVol} title={$t("Effacer le choix")} aria-label={$t("Effacer le choix")}><Icon name="close" size={12}/></button></span>{/if}</div>
          <div class="inline"><input class="num n" data-casting-rot-n type="number" min="1" aria-label="N" bind:value={castingRotN} disabled={busy||preparing||castingEnVol}/><button data-casting-rot-lancer onclick={()=>lancerRotations('perso',castingPerso.trim())} disabled={busy||preparing||!session||castingEnVol||!castingPerso.trim()||exclusionActive} title={exclusionActive?$t("Exclusion d'actions : incompatible avec la rotation de perso — retire l'exclusion."):$t("Rotations · 1 perso × N fois la même pose")}>{$t("Rejouer le perso × N")}</button></div>
          <button data-casting-rot-actions onclick={()=>lancerRotations('actions',castingPerso.trim())} disabled={busy||preparing||!session||castingEnVol||!castingPerso.trim()||exclusionActive} title={$t("Un essai par action, ce perso inchangé")}>{ciblageActionsActif?$tn("Ce perso sur l'action ciblée","Ce perso sur les {n} actions ciblées",actionsChoisies.length):$t("Ce perso sur toutes les actions")}</button>
          <div class="inline choix-rot"><button class="quiet" data-casting-pose-choisir onclick={()=>choisirPourRotation('pose')} disabled={busy||preparing||castingEnVol} title={$t("Rotations · 1 pose × N persos aléatoires")}>{castingPoseFixe.trim()?castingPoseFixe.trim():$t("Choisir une action…")}</button>{#if castingPoseFixe.trim()}<span class="target-chip" data-casting-pose-fixe={castingPoseFixe.trim()}>{castingPoseFixe.trim()}<button class="quiet icon-btn" onclick={()=>castingPoseFixe=''} disabled={busy||preparing||castingEnVol} title={$t("Effacer le choix")} aria-label={$t("Effacer le choix")}><Icon name="close" size={12}/></button></span>{/if}</div>
          <div class="inline"><input class="num n" data-casting-pose-n type="number" min="1" aria-label="N" bind:value={castingPoseN} disabled={busy||preparing||castingEnVol}/><button data-casting-pose-lancer onclick={()=>lancerRotations('pose',castingPoseFixe.trim())} disabled={busy||preparing||!session||castingEnVol||!castingPoseFixe.trim()} title={$t("Rotations · 1 pose × N persos aléatoires")}>{$t("Tirer N persos sur la pose")}</button></div>
        </div>
      </details>
      <details class="sec"><summary><span class="sec-title">{$t("Brouillons · tester de nouvelles actions")}</span></summary>
        <ActionDrafts disabled={busy||preparing||!session||castingEnVol} onlaunch={(ids,n)=>lancerCasting({ids,n})}/>
      </details>
      <details class="sec" title={$t("Modes réels de la fonction fish generez — le moteur gère sa soumission GPU lui-même ; aucun mode test factice.")}><summary><span class="sec-title">{$t("Scripts · modes réels du moteur")}</span></summary>
        <div class="sec-body" data-casting-scripts>
          <select data-casting-script aria-label={$t("Scripts (modes réels)")} bind:value={scriptChoisi} disabled={busy||scriptEnVol}><option value="">{$t("— choisir un script —")}</option>{#each fishScripts as s}<option value={s.script}>{$t(s.label)}</option>{/each}</select>
          {#if scriptChoisi==='poses'}<input data-casting-script-perso aria-label={$t("ID du perso")} placeholder={$t("CH--9009, CH-09999, banque CH-…")} bind:value={scriptPerso} disabled={busy||scriptEnVol}/>{/if}
          <button data-casting-script-lancer onclick={launchScript} disabled={busy||scriptEnVol||!scriptChoisi||(scriptChoisi==='poses'&&!scriptPerso.trim())}>{scriptEnVol?$t("Script en cours…"):$t("Lancer le script réel")}</button>
          {#if scriptNote}<p class="small muted" role="status" data-casting-script-note>{scriptNote}</p>{/if}
          {#if scriptErreur}<p class="error small" role="alert">{scriptErreur}</p>{/if}
          {#if scriptSortie}<pre class="script-out" data-casting-script-sortie>{scriptSortie}</pre>{/if}
        </div>
      </details>
      <div class="lots-head"><h2>{$t("Lots")}</h2><select aria-label={$t("État du lot")} bind:value={lotFilter}><option value="">{$t("Tous")}</option><option value="active">{$t("En cours")}</option><option value="paused">{$t("En pause")}</option><option value="incomplete">{$t("Incomplets")}</option><option value="completed">{$t("Terminés")}</option></select><button class="quiet icon-btn" disabled={syncing||busy} onclick={syncCasting} title={$t("Actualiser")} aria-label={$t("Actualiser")}><Icon name="redo" size={14}/></button></div>
      {#if castingRows.length}
      <div class="lot-list" data-casting-lots>
        {#each visibleLots as lot (lot.id)}
          <button class="lot" class:on={lot.id===selectedLotId} data-casting-lot={lot.id} data-lot-state={lot.state} onclick={()=>{selectedLot=lot.id;resetResultFilters()}}>
            <b class="num">{lot.createdAt?castingDate(lot.createdAt):$t("Lot")}</b>
            <span class="num count">{lot.done}/{lot.rows.length}</span>
            <span class="bar"><i style:width={`${Math.round(100*lot.done/Math.max(1,lot.rows.length))}%`}></i></span>
            <small>{$t(lotEtat[lot.state])}{lot.remaining&&lot.state!=='completed'?` · ${lot.remaining} ${$t("restantes")}`:''}</small>
          </button>
        {/each}
      </div>
      {/if}
    </div>
  </aside>

  <main class="wb-center">
    <div class="wb-bar">
      <b class="lot-title">{selLot?(selLot.createdAt?$t("Lot du {date}",{date:castingDate(selLot.createdAt)}):$t("Lot")):$t("Casting")}</b>
      {#if selLot}<span class="muted num">{selLot.done}/{selLot.rows.length} {$t("images générées")}</span>{/if}
      {#if selLot?.retry}{@const lot=selLot}<button data-casting-retry disabled={busy||activeRows.some(r=>batchKey(r)!==lot.id&&Object.values(r.phases??{}).some((p:any)=>phaseActive(p.state)))} onclick={()=>actionRun('resume',lot.retry)}>{$t("Terminer le lot")} — {lot.remaining} {$t("restantes")}</button>{/if}
      <span class="grow"></span>
      {#if castingRows.length}
      <div class="filters" data-casting-filtres>
        <select aria-label={$t("Résultats du lot")} title={$t("Résultats du lot")} bind:value={resultFilter}><option value="images">{$t("Images obtenues")}</option><option value="all">{$t("Tous les essais")}</option><option value="pending">{$t("En attente")}</option><option value="cancelled">{$t("Annulés")}</option></select>
        <select data-casting-filtre-famille aria-label={$t("Filtre · famille")} title={$t("Filtre · famille")} bind:value={grilleFamille} disabled={busy||preparing||castingEnVol}><option value="">{$t("Toutes")}</option>{#each gridFamilies as f}<option value={f}>{f}</option>{/each}</select>
        <select data-casting-filtre-model aria-label={$t("Filtre · checkpoint")} title={$t("Filtre · checkpoint")} bind:value={grilleModel} disabled={busy||preparing||castingEnVol}><option value="">{$t("Tous")}</option>{#each gridModels as m}<option value={m}>{m}</option>{/each}</select>
        <select data-casting-filtre-lora aria-label={$t("Filtre · LoRA")} title={$t("Filtre · LoRA")} bind:value={grilleLora} disabled={busy||preparing||castingEnVol}><option value="">{$t("Toutes")}</option>{#each gridLoras as l}<option value={l}>{l}</option>{/each}</select>
        <select data-casting-tri aria-label={$t("Tri")} title={$t("Tri")} bind:value={grilleTri} disabled={busy||preparing||castingEnVol}><option value="recents">{$t("Plus récents d'abord")}</option><option value="anciens">{$t("Plus anciens d'abord")}</option></select>
        <span class="muted num small" data-casting-filtre-note>{$tn("{n} affiché sur {total}","{n} affichés sur {total}",grilleAffichees.length,{total:castingRows.length})}</span>
      </div>
      {/if}
      <input class="size" type="range" min="110" max="360" step="10" bind:value={tile} aria-label={$t("Taille des vignettes")} title={$t("Taille des vignettes")}/>
      <details class="menu">
        <summary class="icon-btn" title={$t("Actions du lot")} aria-label={$t("Actions du lot")}><Icon name="more"/></summary>
        <div class="menu-pop">
          <button class="quiet" onclick={e=>{fermerMenu(e);onbrowse()}}>{$t("Voir la galerie")}</button>
          <button class="quiet" onclick={e=>{fermerMenu(e);onconfigure()}}>{$t("Réglages dans Génération")}</button>
          {#if selLot}{@const lot=selLot}<button class="danger" data-casting-delete disabled={busy||lot.state==='active'} title={lot.state==='active'?$t("Annuler le lot avant de le supprimer"):''} onclick={e=>{fermerMenu(e);supprimerLot(lot.rows[0].run,lot.done)}}><Icon name="trash" size={14}/>{$t("Supprimer le lot")}</button>{/if}
        </div>
      </details>
    </div>
    {#if preparing}
    <div class="strip" data-casting-preparation role="status" aria-live="polite" aria-busy="true">
      <b>{preparation.stage==='prepare'?$t("Préparation"):preparation.stage==='start'?$t("Lancement"):preparation.stage==='catalogue'?$t("ComfyUI · catalogue"):preparation.stage==='characters'?$t("Personnages"):$t("Casting · tirage")}</b>
      {#if preparation.stage==='prepare'}<span class="num" data-casting-prepared-count>{preparation.completed} / {preparation.total}</span><progress max={preparation.total||1} value={preparation.completed??0}></progress>{:else}<progress></progress>{/if}
      <span class="num muted">{preparation.elapsedSeconds??0} s</span>
      {#if preparationError}<span class="error">{preparationError}</span>{/if}
    </div>
    {/if}
    {#if runBarVisible}
    <div class="strip runbar" data-casting-runbar data-run-etat={runEnPause?'pause':'en-vol'}>
      <div class="run-line">
        <b data-casting-runbar-essai>{currentAttempt||$t("Essai du casting")}</b>
        <span class="num muted" data-casting-runbar-compte>{$tn("{done} / {n} essai terminé","{done} / {n} essais terminés",batchRows.length,{done:batchDone})}</span>
        <span class="muted" data-casting-runbar-phase>{currentPass}</span>
        {#if waitingFor}<span class="waiting-for" data-casting-runbar-waiting>{waitingFor}</span>{/if}
        {#if runEnPause}<span class="paused" data-casting-runbar-pause>{$t("En pause")}</span>{/if}
        <progress data-casting-runbar-progress value={batchDone} max={Math.max(1,batchRows.length)} aria-label={$t("Essais terminés")}></progress>
        {#if !runEnPause}<button data-casting-pause disabled={busy} onclick={()=>actionRun('pause')}>{$t("Pause")}</button>{/if}
        {#if runEnPause}<button data-casting-resume disabled={busy} onclick={()=>actionRun('resume')}>{$t("Reprendre")}</button>{/if}
        <button class="danger" data-casting-stop disabled={busy} onclick={()=>actionRun('stop')}>{$t("Annuler")}</button>
        <span class="hint" data-casting-pause-note title={$t("Pause : s'arrêtera à la fin de l'essai en cours — ComfyUI ne permet pas de suspendre un sampler. Annuler interrompt le calcul courant et annule les essais en attente.")}><Icon name="info" size={14}/><span class="sr">{$t("Pause : s'arrêtera à la fin de l'essai en cours — ComfyUI ne permet pas de suspendre un sampler. Annuler interrompt le calcul courant et annule les essais en attente.")}</span></span>
      </div>
      {#if progressRow}<RunProgress run={{id:progressRow.run,promptId:progressRow.promptId,state:progressRow.runState,phases:progressRow.phases,phaseCount:progressRow.phases?.['2']?6:1}}/>{/if}
    </div>
    {/if}
    {#if pendingStops.length}<details class="strip-details" data-previous-stop><summary>{$t("Arrêt à confirmer · anciens essais ({n})",{n:pendingStops.length})}</summary><p class="muted" data-casting-stop-pending>{$t("Lot annulé localement. Arrêt distant non confirmé ; le moteur peut encore terminer un calcul.")} <button disabled={busy} onclick={()=>actionRun('stop',pendingStops[0].run)}>{$t("Réessayer l’arrêt distant")}</button></p></details>{/if}
    {#if syncError}<p class="strip notice" role="status">{$t("Synchronisation interrompue : {detail}. Les derniers résultats restent affichés.",{detail:syncError})}</p>{/if}
    {#if error}<p class="strip error" role="alert">{error}</p>{/if}
    {#if choix.length}
    <div class="strip selbar" data-casting-selection>
      <span class="num">{$tn("{n} image sélectionnée","{n} images sélectionnées",choix.length)}</span>
      <button class="primary" data-casting-publish-selection disabled={busy} onclick={()=>publier(choix)}>{$t("Garder dans la galerie")}</button>
      <button class="quiet" disabled={busy} onclick={()=>choix=[]}>{$t("Tout désélectionner")}</button>
      <span class="grow"></span>
      <span class="muted small">{$t("Espace : sélectionner · ← → : naviguer · Entrée : plein écran")}</span>
    </div>
    {/if}
    {#if stepView}
    {@const row=stepCurrent}
    <section class="step-stage" data-casting-step>
      <div class="step-image">
        {#if row?.finalAsset&&passesFaites(row)===6}<Thumbnail id={row.finalAsset} preview alt={row.name}/>
        {:else if row?.asset}<Thumbnail id={row.asset} preview alt={row.name}/>
        {:else}<p class="muted">{$t("Génération de l'image…")}</p>{/if}
      </div>
      <div class="step-bar">
        {#if row}<b>{row.name}</b><span class="muted">{row.famille}</span>{/if}
        <span class="muted small" data-casting-step-state>{!row||!row.asset?$t("Passe 1 en cours…"):row.decision==='oui'?(passesFaites(row)===6?$t("Full terminé · 6/6 passes"):$t("Full en cours · {done}/6 passes",{done:passesFaites(row)})):$t("Full ou Suivante ?")}</span>
        <span class="grow"></span>
        <button class="primary" data-casting-step-full disabled={busy||!row?.asset||row?.decision==='oui'} onclick={stepFull}>{$t("Full · 6 passes")} <kbd>F</kbd></button>
        <button data-casting-step-next disabled={busy||(!!row&&!row.asset&&!row.error)} onclick={stepNext}>{$t("Suivante")} <kbd>N</kbd></button>
        <button class="quiet" data-casting-step-quit disabled={busy} onclick={stepQuit}>{$t("Terminer")}</button>
      </div>
    </section>
    {/if}
    <div class="sheet casting-grid" style:--tile={`${tile}px`} hidden={stepView}>
      {#each grilleAffichees as row (row.run+':'+row.stem)}
        {@const k=rowKey(row)}
        <article class="tile" class:focus={focused&&rowKey(focused)===k} class:sel={choix.includes(row.run)} data-tile-key={k} data-casting-shot={row.stem} data-famille={row.famille} data-model={row.model} data-loras={lorasDuShot(row.shot).join(',')} data-state={row.state} data-decision={row.decision}>
          <div class="frame">
            {#if row.asset}
              <button class="tile-img" onclick={()=>focaliser(row)} ondblclick={()=>ouvrirApercu(row)} onkeydown={e=>tuileClavier(e,row)} aria-label={row.name} title={`${row.name} · ${row.famille} · ${row.model} · seed ${row.seed}`}><Thumbnail id={row.finalAsset||row.asset} alt={row.finalAsset?$t("Finition · {name}",{name:row.name}):$t("Passe 1 · {name}",{name:row.name})}/></button>
              {#if row.published}<span class="kept" data-casting-published>{$t("Gardée")}</span>{:else}<input class="pick" type="checkbox" data-casting-select={row.run} checked={choix.includes(row.run)} disabled={busy} aria-label={$t("Sélectionner")} onchange={e=>choix=e.currentTarget.checked?[...choix,row.run]:choix.filter(r=>r!==row.run)}/>{/if}
              <button class="open icon-btn" data-casting-preview onclick={()=>ouvrirApercu(row)} aria-label={$t("Ouvrir {name} · enregistrer le personnage ou lancer le workflow complet",{name:row.name})} title={$t("Ouvrir {name} · enregistrer le personnage ou lancer le workflow complet",{name:row.name})}><Icon name="expand" size={14}/></button>
            {:else}
              <button class="tile-img ph" onclick={()=>focaliser(row)} onkeydown={e=>tuileClavier(e,row)} aria-label={row.name}><span>{['submitted','submitting','queued','running'].includes(row.state)?$t("Passe 1 en cours…"):row.state==='succeeded'?$t("Image indisponible"):$t("Passe 1 · {state}",{state:$t(labels[row.state]??row.state)})}</span></button>
            {/if}
          </div>
          <div class="meta"><h2>{row.name}</h2><span>{row.famille}</span></div>
          {#if row.decision==='oui'&&row.phases?.['2']}
            {#if row.runState==='running'}<p class="tile-progress">{$t("Finition en cours · {done}/6 passes",{done:passesFaites(row)})}</p>
            {:else}<p class="tile-progress" data-casting-progress>{row.runState==='completed'?$t("Workflow complet terminé · {done}/6 passes",{done:passesFaites(row)}):$t("Workflow complet interrompu · {done}/6 passes",{done:passesFaites(row)})}</p>{/if}
          {:else if row.decision==='oui'}<p class="tile-progress" data-casting-progress>{$t("Enregistré · workflow complet à lancer depuis l'image")}</p>
          {:else if row.decision==='non'}<p class="tile-progress" data-casting-progress>{$t("Écarté · ancien essai")}</p>{/if}
          {#if row.error}<p class="error tile-progress" title={row.error}>{row.error}</p>{/if}
        </article>
      {:else}<p class="empty-prompts">{castingRows.length?$t("Aucun shot ne correspond à ces filtres (famille, checkpoint, LoRA) — remets « Toutes »/« Tous » dans la barre de filtres."):(session?$t("Aucun lot pour l'instant : règle le lot à gauche puis lance les essais."):$t("Aucun casting lancé. Ce menu ne règle pas le workflow : ouvre ta préparation dans Generation, puis reviens ici tirer ton casting."))}</p>{/each}
    </div>
  </main>

  <aside class="wb-panel right">
    <div class="wb-head"><h2>{focused?focused.name:$t("Casting")}</h2>{#if focused?.asset}{@const f=focused}<button class="quiet icon-btn" onclick={()=>ouvrirApercu(f)} title={$t("Ouvrir {name} · enregistrer le personnage ou lancer le workflow complet",{name:f.name})} aria-label={$t("Ouvrir {name} · enregistrer le personnage ou lancer le workflow complet",{name:f.name})}><Icon name="expand"/></button>{/if}</div>
    <div class="wb-scroll">
      {#if focused}{@const fiche=focused}
        <div class="inspector-image">
          {#if fiche.asset}<button class="insp-img" onclick={()=>ouvrirApercu(fiche)} aria-label={$t("Ouvrir {name} · enregistrer le personnage ou lancer le workflow complet",{name:fiche.name})}>{#key fiche.finalAsset||fiche.asset}<Thumbnail id={fiche.finalAsset||fiche.asset!} preview alt={fiche.name}/>{/key}</button>
          {:else}<div class="insp-ph">{$t(labels[fiche.state]??fiche.state)}</div>{/if}
        </div>
        <div class="insp-kv"><dl class="kv">
          <dt>{$t("Famille")}</dt><dd>{fiche.famille||'—'}</dd>
          <dt>{$t("Modèle")}</dt><dd>{fiche.model||'—'}</dd>
          <dt>{$t("Seed")}</dt><dd class="num">{fiche.seed}</dd>
          <dt>{$t("Passe")}</dt><dd class="num">{passesFaites(fiche)} / 6</dd>
          {#if lorasDuShot(fiche.shot).length}<dt>{$t("LoRA")}</dt><dd>{lorasDuShot(fiche.shot).join(', ')}</dd>{/if}
          {#if fiche.state!=='succeeded'}<dt>{$t("État")}</dt><dd>{$t(labels[fiche.state]??fiche.state)}</dd>{/if}
        </dl>{#if fiche.error}<p class="error small">{fiche.error}</p>{/if}</div>
        <div class="wb-actions">
          {#if fiche.published}<span class="in-gallery" data-casting-inspector-published><Icon name="check" size={14}/>{$t("Déjà dans la galerie")}</span>
          {:else}<button class="primary" data-casting-publish disabled={busy||!fiche.asset} onclick={()=>publier([fiche.run])}>{$t("Garder dans la galerie")}</button>{/if}
          <div class="save-row" title={$t("Crée une fiche personnage réutilisable depuis ce résultat. Ne lance JAMAIS les passes 2 à 6.")}>
            <input data-casting-save-nom placeholder={$t("Nom de la fiche…")} aria-label={$t("Nom de la fiche…")} bind:value={saveName} disabled={busy||!fiche.asset}/>
            <button data-casting-enregistrer disabled={busy||!fiche.asset||!saveName.trim()} onclick={()=>saveCharacter(fiche)}>{$t("Enregistrer comme personnage")}</button>
          </div>
          {#if enregistrerNote}<p class="small muted" data-casting-enregistre-note role="status">{enregistrerNote}</p>{/if}
          {#if fiche.decision==='oui'}<p class="small muted">{fiche.runState==='completed'?$t("Workflow complet déjà lancé sur cet essai · terminé."):$t("Workflow complet déjà lancé sur cet essai · suivi dans la grille.")}</p>
          {:else}<button data-casting-complet disabled={busy||!fiche.asset} onclick={()=>launchFull(fiche)} title={$t("Lance les 6 passes sur CE résultat — seed et réglages préservés, mécanique existante.")}>{$t("Finir les 6 passes")}</button>{/if}
        </div>
      {/if}
    </div>
  </aside>
</section>
{#if cibler}
<div class="cibler-backdrop" data-cibler-fenetre role="presentation" onclick={e=>{if(e.target===e.currentTarget)cibler=false}}>
  <div class="cibler-fenetre" role="dialog" aria-modal="true" aria-label={$t("Cibler des personnages")} tabindex="-1" use:focusFenetre onkeydown={e=>{if(e.key==='Escape')cibler=false}}>
    <header>
      <div><h2>{$t("Cibler des personnages")}</h2><small>{$t("{n} validées du registre moteur · clique les tuiles à entrer au casting (un shot chacune, dans l'ordre choisi)",{n:persosRows.length})}</small></div>
      <input data-cibler-recherche placeholder={$t("Rechercher par nom…")} bind:value={ciblerRecherche}/>
      <label class="cibler-filtre">{$t("Famille")}<select data-cibler-filtre-famille bind:value={ciblerFamille}><option value="">{$t("Toutes")}</option>{#each charFamilies as f}<option value={f}>{f}</option>{/each}</select></label>
      <label class="cibler-filtre">{$t("Checkpoint")}<select data-cibler-filtre-model bind:value={ciblerModel}><option value="">{$t("Tous")}</option>{#each charModels as m}<option value={m}>{m}</option>{/each}</select></label>
      <button data-cibler-fermer onclick={()=>cibler=false}>{$t("Fermer")}</button>
    </header>
    {#if ciblerErreur}<p class="error" role="alert">{ciblerErreur}</p>{/if}
    <div class="cibler-grille" data-cibler-grille>
      {#each displayedTargets as ligne (ligne.negId)}
        <button type="button" class="cibler-tuile" data-cibler-perso={ligne.negId} data-selected={ciblerChoisis.includes(ligne.negId)?'oui':'non'} onclick={()=>basculerPerso(ligne)} title={`${ligne.nom} · ${ligne.negId} · ${ligne.model}${ligne.famille?` · ${ligne.famille}`:''}`}>
          <span class="cibler-portrait">{#if ligne.portrait??imagesChoix.persos[ligne.negId]}<Thumbnail id={ligne.portrait??imagesChoix.persos[ligne.negId]} alt={$t("Portrait · {name}",{name:ligne.nom})}/>{:else}<span class="cibler-sans-portrait">{$t("Pas encore d'image")}</span>{/if}</span>
          <strong>{ligne.nom}</strong>
          <small>{ligne.negId}{ligne.model?` · ${ligne.model}`:''}{ligne.source?` · ${ligne.source}`:''}</small>
        </button>
      {:else}
        <p class="empty-prompts">{$t("Aucun perso ne correspond à ces filtres (recherche, famille, checkpoint).")}</p>
      {/each}
    </div>
    <footer>
      <span data-cibler-compteur>{$tn("{n} sélectionné","{n} sélectionnés",ciblerChoisis.length)}{ciblerChoisis.length?` : ${ciblerChoisis.join(', ')}`:''}</span>
      <button data-cibler-valider class="primary" onclick={validerCiblage} disabled={!ciblerChoisis.length||busy}>{$tn("Utiliser {n} perso au casting","Utiliser {n} persos au casting",ciblerChoisis.length)}</button>
    </footer>
  </div>
</div>
{/if}
{#if targetActions}
<div class="cibler-backdrop" data-actions-fenetre role="presentation" onclick={e=>{if(e.target===e.currentTarget)targetActions=false}}>
  <div class="cibler-fenetre" role="dialog" aria-modal="true" aria-label={actionsMode==='cibler'?$t("Cibler des actions"):$t("Exclure des actions du tirage")} tabindex="-1" use:focusFenetre onkeydown={e=>{if(e.key==='Escape')targetActions=false}}>
    <header>
      <div><h2>{actionsMode==='cibler'?$t("Cibler des actions"):$t("Exclure du tirage")}</h2><small>{actionsMode==='cibler'?$t("{n} actions disponibles · clique les actions que le casting doit porter (chaque OC en prend une, en cycle, dans l'ordre choisi)",{n:actionsRows.filter(actionsUtilisable).length}):$t("{n} actions disponibles · clique les actions à retirer du tirage du moteur (exclude[])",{n:actionsRows.filter(actionsUtilisable).length})}</small></div>
      <div class="actions-modes" role="tablist" aria-label={$t("Mode du choix d'actions")}>
        <button type="button" role="tab" data-actions-mode-cibler aria-selected={actionsMode==='cibler'} class:chosen={actionsMode==='cibler'} onclick={()=>actionsMode='cibler'}>{$t("Cibler")}</button>
        <button type="button" role="tab" data-actions-mode-exclure aria-selected={actionsMode==='exclure'} class:chosen={actionsMode==='exclure'} onclick={()=>actionsMode='exclure'}>{$t("Exclure du tirage")}</button>
      </div>
      <input data-actions-recherche placeholder={$t("Rechercher (nom, famille, note)…")} bind:value={actionsRecherche}/>
      <label class="cibler-filtre">{$t("Famille")}<select data-actions-filtre-famille bind:value={actionsFamille}><option value="">{$t("Toutes")}</option>{#each actionFamilies as f}<option value={f}>{f}</option>{/each}</select></label>
      <label class="cibler-filtre">{$t("LoRA")}<select data-actions-filtre-lora bind:value={actionsLora}><option value="">{$t("Toutes")}</option>{#each actionLoras as l}<option value={l}>{l}</option>{/each}</select></label>
      <button data-actions-fermer onclick={()=>targetActions=false}>{$t("Fermer")}</button>
    </header>
    {#if actionsErreur}<p class="error" role="alert">{actionsErreur}</p>{/if}
    <div class="cibler-grille actions-grille" data-actions-liste>
      {#each displayedActions as ligne (ligne.name)}
        <button type="button" class="cibler-tuile action-tuile" data-action-item={ligne.name} data-selectionne={actionsListeCourante.includes(ligne.name)?'oui':'non'} data-lora={ligne.lora??''} onclick={()=>basculerAction(ligne)} title={`${ligne.name} · ${ligne.famille}${ligne.lora?` · LoRA ${ligne.lora}`:''}${' · '+(actionsMode==='cibler'?$t("au casting, chaque OC portera une des actions ciblées"):$t("cette action sera retirée du tirage du moteur"))}`}>
          <span class="cibler-portrait">{#if imagesChoix.actions[ligne.name]}<Thumbnail id={imagesChoix.actions[ligne.name]} alt={ligne.name}/>{:else}<span class="cibler-sans-portrait">{$t("Pas encore d'image")}</span>{/if}</span>
          <strong>{ligne.name}</strong>
          <small>{ligne.famille}{ligne.lora?` · LoRA ${ligne.lora}`:''}</small>
          {#if ligne.add}<em>{ligne.add}</em>{/if}
        </button>
      {:else}
        <p class="empty-prompts">{actionsCharge?$t("Aucune action disponible ne correspond à ces filtres (recherche, famille, LoRA) — les actions indisponibles ne sont plus affichées."):$t("Lecture de la banque…")}</p>
      {/each}
    </div>
    <footer>
      <span data-actions-compteur data-actions-compte={actionsListeCourante.length}>{actionsMode==='cibler'?$tn("{n} sélectionnée","{n} sélectionnées",actionsListeCourante.length):$tn("{n} exclue","{n} exclues",actionsListeCourante.length)}{actionsListeCourante.length?` : ${actionsListeCourante.join(', ')}`:''}</span>
      <button data-actions-valider class="primary" onclick={validerCiblageActions} disabled={!actionsListeCourante.length||busy}>{actionsMode==='cibler'?$tn("Utiliser {n} action au casting","Utiliser {n} actions au casting",actionsListeCourante.length):$tn("Exclure {n} action du tirage","Exclure {n} actions du tirage",actionsListeCourante.length)}</button>
    </footer>
  </div>
</div>
{/if}
{#if preview}{@const fiche=preview}<dialog class="casting-preview" data-casting-preview-dialog use:showDialog onclose={()=>preview=null} onkeydown={e=>{if(e.key==='ArrowLeft'){e.preventDefault();apercuVoisin(-1)}else if(e.key==='ArrowRight'){e.preventDefault();apercuVoisin(1)}}}>
  <header><h2>{fiche.name}</h2><span class="muted small">{fiche.famille} · {fiche.model} · seed <span class="num">{fiche.seed}</span></span><span class="grow"></span><button class="icon-btn" data-casting-preview-prev disabled={apercuIndex<=0} onclick={()=>apercuVoisin(-1)} aria-label={$t("Image précédente")} title={$t("Image précédente")}><Icon name="left"/></button><span class="num muted" data-casting-preview-position>{apercuIndex+1} / {apercus.length}</span><button class="icon-btn" data-casting-preview-next disabled={apercuIndex<0||apercuIndex>=apercus.length-1} onclick={()=>apercuVoisin(1)} aria-label={$t("Image suivante")} title={$t("Image suivante")}><Icon name="right"/></button><button class="quiet" data-casting-preview-fermer onclick={()=>preview=null}><Icon name="close" size={14}/>{$t("Fermer")}</button></header>
  <div class="casting-preview-layout"><button class:zoom class="casting-large-image" data-casting-zoom onclick={()=>zoom=!zoom} aria-label={$t("Ajuster à la fenêtre ou taille réelle")} title={zoom?$t("Taille réelle · clic pour ajuster"):$t("Ajusté à la fenêtre · clic pour la taille réelle")}>{#key fiche.finalAsset||fiche.asset}<Thumbnail id={fiche.finalAsset||fiche.asset!} preview alt={fiche.name}/>{/key}</button><aside>
  {#if fiche.published}<p class="in-gallery" data-casting-published><Icon name="check" size={14}/>{$t("Déjà dans la galerie")}</p>{:else}<button class="primary" data-casting-publish disabled={busy} onclick={()=>publier([fiche.run])}>{$t("Garder dans la galerie")}</button>{/if}
  <div class="casting-detail-actions" data-casting-detail-actions>
    <div class="save-row" title={$t("Crée une fiche personnage réutilisable depuis ce résultat. Ne lance JAMAIS les passes 2 à 6.")}>
      <input data-casting-save-nom placeholder={$t("Nom de la fiche…")} aria-label={$t("Nom de la fiche…")} bind:value={saveName} disabled={busy}/>
      <button data-casting-enregistrer disabled={busy||!fiche.asset||!saveName.trim()} onclick={()=>saveCharacter(fiche)}>{$t("Enregistrer comme personnage")}</button>
    </div>
    {#if enregistrerNote}<p class="small muted" data-casting-enregistre-note role="status">{enregistrerNote}</p>{/if}
    {#if fiche.decision==='oui'}
      <p class="small muted" data-casting-complet-etat>{fiche.runState==='completed'?$t("Workflow complet déjà lancé sur cet essai · terminé."):$t("Workflow complet déjà lancé sur cet essai · suivi dans la grille.")}</p>
    {:else}
      <button data-casting-complet disabled={busy||!fiche.asset} onclick={()=>launchFull(fiche)} title={$t("Lance les 6 passes sur CE résultat — seed et réglages préservés, mécanique existante.")}>{$t("Finir les 6 passes")}</button>
    {/if}
  </div>
</aside></div></dialog>{/if}
<style>
  .casting-wb{height:100%;min-height:0}
  .casting-wb details{margin:0}
  .grow{flex:1}
  .small{font-size:11.5px}
  .sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
  /* Left panel: new batch + batches */
  .new-lot{display:grid;grid-template-columns:minmax(0,1fr);gap:8px;padding:10px 12px 12px;border-bottom:1px solid var(--line)}
  .new-lot .field-row{display:grid;grid-template-columns:84px minmax(0,1fr);margin:0;flex-direction:row;font-size:inherit;color:inherit}
  .new-lot .field-row.top{align-items:start}.new-lot .field-row.top>span:first-child{padding-top:5px}
  .new-lot input,.new-lot select{width:100%}
  .targets{display:grid;gap:2px;justify-items:start;min-width:0}
  .targets>button{min-height:24px;padding:0 6px;font-size:12px;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .target-chip{display:inline-flex;align-items:center;gap:2px;font-size:11.5px;color:var(--text);border:1px solid var(--line-strong);border-radius:3px;padding:0 0 0 6px;margin:0 0 4px 6px}
  .target-chip.exclusion{color:var(--danger)}
  .target-chip .icon-btn{min-height:20px;width:20px}
  .launch{height:32px;margin-top:2px}
  .casting-note{margin:0;font-size:11.5px;color:var(--muted);overflow-wrap:anywhere}.casting-note:empty{display:none}
  .source{display:flex;align-items:center;gap:4px;margin:0;font-size:11.5px;color:var(--muted)}
  .source>span{flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .source .icon-btn{min-height:22px;width:22px}
  .sec-body .inline{display:flex;gap:6px}.sec-body .n{width:56px;flex:none}.sec-body .inline button{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .sec-body input,.sec-body select{width:100%}
  .sec-body p{margin:0}
  .script-out{max-height:220px;overflow:auto;background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:8px;font-size:11px;color:var(--muted);white-space:pre-wrap;margin:0}
  .lots-head{display:flex;align-items:center;gap:6px;min-height:40px;padding:0 12px;border-bottom:1px solid var(--line)}
  .lots-head h2{flex:1;margin:0;font-size:13px;font-weight:600}
  .lots-head select{min-height:24px;font-size:11.5px}.lots-head .icon-btn{min-height:24px;width:24px}
  .lot{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:4px 8px;width:100%;min-height:0;padding:9px 12px;border:0;border-bottom:1px solid var(--line);border-radius:0;background:transparent;text-align:start}
  .lot:hover:not(:disabled){background:var(--raised)}
  .lot.on{background:var(--raised);box-shadow:inset 2px 0 var(--accent-warm)}
  .lot b{font-weight:500}.lot .count{font-size:11.5px;color:var(--muted)}
  .lot small{grid-column:1/-1;color:var(--muted);font-size:11px}
  .bar{grid-column:1/-1;display:block;height:3px;background:var(--line-strong);border-radius:2px;overflow:hidden}
  .bar i{display:block;height:100%;background:var(--muted)}
  .lot[data-lot-state=active] .bar i{background-color:var(--accent)}.lot[data-lot-state=active] .count{color:var(--accent-light)}
  .lot[data-lot-state=active] .count{background:none}
  .lot[data-lot-state=completed] .bar i{background:var(--ok)}.lot[data-lot-state=completed] .count{color:var(--ok)}
  .lot[data-lot-state=incomplete] .bar i{background:var(--warning)}.lot[data-lot-state=incomplete] .count{color:var(--warning)}
  /* Center: batch bar, banners, contact sheet */
  .lot-title{font-weight:600;white-space:nowrap}
  .filters{display:flex;align-items:center;gap:4px;flex-wrap:wrap}
  .filters select{max-width:150px}
  .size{width:100px;min-height:0;padding:0;background:none;border:0;accent-color:var(--accent)}
  .menu{position:relative}
  .menu>summary{list-style:none;display:inline-grid;place-items:center;min-height:26px;width:28px;border-radius:var(--radius);color:var(--muted);cursor:pointer}
  .menu>summary::-webkit-details-marker{display:none}
  .menu>summary:hover,.menu[open]>summary{background:var(--raised);color:var(--text)}
  .menu-pop{position:absolute;z-index:20;top:calc(100% + 4px);inset-inline-end:0;min-width:200px;display:grid;gap:2px;padding:4px;background:var(--panel);border:1px solid var(--line-strong);border-radius:var(--radius-lg)}
  .menu-pop button{justify-content:flex-start;display:flex;align-items:center;gap:6px;text-align:start;width:100%}
  .menu-pop button.quiet{color:var(--text)}
  .strip{display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin:0;padding:7px 16px;border-bottom:1px solid var(--line);background:var(--bg);flex:none}
  .strip progress{flex:1;min-width:80px;height:4px;accent-color:var(--accent)}
  .strip.error{color:var(--danger)}
  .runbar{display:grid;gap:6px}
  .run-line{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
  .run-line progress{flex:1;min-width:100px}
  .paused{color:var(--warning)}
  .hint{display:inline-grid;place-items:center;color:var(--faint);cursor:help}
  .runbar :global(.run-progress){display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:4px 14px;margin:0}
  .runbar :global(.run-progress label){color:var(--muted);font-size:11.5px}
  .runbar :global(.run-progress span),.runbar :global(.run-progress small){color:var(--muted)}
  .runbar :global(.run-progress progress){height:3px;accent-color:var(--accent)}
  .runbar :global(.run-progress small){grid-column:1/-1}
  .strip-details{padding:6px 16px;border-bottom:1px solid var(--line);background:var(--bg);flex:none}
  .strip-details summary{color:var(--warning);font-size:12px}.strip-details p{margin:6px 0 0}
  .selbar .muted{font-size:11.5px}
  .sheet{flex:1;min-height:0;overflow:auto;padding:16px;display:grid;grid-template-columns:repeat(auto-fill,minmax(var(--tile,170px),1fr));gap:14px 12px;align-content:start}
  .tile{position:relative;min-width:0}
  .frame{position:relative}
  .tile-img{display:block;width:100%;min-height:0;padding:0;border:0;border-radius:var(--r-img);background:var(--panel);overflow:hidden;aspect-ratio:832/1216;cursor:pointer}
  .tile-img:hover:not(:disabled){background:var(--panel)}
  .tile-img :global(img),.tile-img :global(.image-placeholder){width:100%;height:100%;object-fit:cover;display:block;min-height:0;border-radius:var(--r-img);background:var(--panel);color:var(--faint);font-size:11px}
  .tile-img:focus-visible{outline:none}
  .tile.focus .tile-img{outline:2px solid var(--sel);outline-offset:2px}
  .tile-img.ph{display:grid;place-items:center;background:transparent;border:1px dashed var(--line-strong);color:var(--faint);font-size:11px;padding:8px;text-align:center;cursor:default}
  .tile.focus .tile-img.ph{border-color:var(--sel)}
  .pick{position:absolute;top:6px;inset-inline-start:6px;width:18px;height:18px;min-height:0;margin:0;padding:0;accent-color:var(--accent);cursor:pointer}
  .kept{position:absolute;top:6px;inset-inline-end:6px;font-size:10.5px;background:rgba(12,13,16,.85);border-radius:3px;padding:1px 6px;color:var(--accent-warm)}
  .open{position:absolute;bottom:6px;inset-inline-end:6px;min-height:24px;width:24px;background:rgba(12,13,16,.8);border-color:transparent;color:var(--text);opacity:0}
  .tile:hover .open,.tile.focus .open,.open:focus-visible{opacity:1}
  .tile.sel .tile-img{outline:2px solid var(--accent);outline-offset:2px}
  .meta{display:flex;justify-content:space-between;gap:6px;margin-top:5px;font-size:11.5px;color:var(--muted);min-width:0}
  .meta h2{margin:0;font-size:11.5px;font-weight:500;color:var(--text);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .meta span{flex:none}
  .tile-progress{margin:2px 0 0;font-size:11px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .tile-progress.error{color:var(--danger)}
  .empty-prompts{grid-column:1/-1;margin:40px auto;max-width:460px;text-align:center;color:var(--muted)}
  /* Inspector */
  .insp-img{display:block;width:100%;min-height:0;padding:0;border:0;background:var(--canvas);border-radius:var(--r-img);cursor:zoom-in;overflow:hidden}
  .insp-img:hover:not(:disabled){background:var(--canvas)}
  .insp-img :global(img){width:100%;max-height:60vh;object-fit:contain;display:block}
  .insp-ph{aspect-ratio:832/1216;max-height:60vh;display:grid;place-items:center;border:1px dashed var(--line-strong);border-radius:var(--r-img);color:var(--faint);font-size:11.5px}
  .insp-kv{padding:12px;border-bottom:1px solid var(--line)}
  .insp-kv .error{margin:8px 0 0}
  .save-row{display:grid;gap:6px}.save-row input,.save-row button{width:100%}
  .wb-actions p{margin:0}
  .in-gallery{display:flex;align-items:center;justify-content:center;gap:6px;min-height:28px;margin:0;color:var(--ok);border:1px solid var(--line);border-radius:var(--radius)}
  /* Large preview */
  .casting-preview{width:98vw;max-width:none;height:96vh;max-height:96vh;background:var(--canvas);color:var(--text);border:1px solid var(--line-strong);border-radius:var(--radius-lg);padding:0;overflow:hidden}
  .casting-preview[open]{display:flex;flex-direction:column}
  .casting-preview::backdrop{background:#000d}
  .casting-preview header{display:flex;align-items:center;gap:8px;min-height:44px;padding:0 12px;border-bottom:1px solid var(--line);background:var(--bg)}
  .casting-preview header h2{margin:0;font-size:13px;font-weight:600}
  .casting-preview-layout{display:grid;grid-template-columns:minmax(0,1fr) 300px;flex:1;min-height:0}
  .casting-preview-layout aside{overflow:auto;min-height:0;display:grid;gap:8px;align-content:start;padding:12px;background:var(--panel);border-inline-start:1px solid var(--line)}
  .casting-detail-actions{display:grid;gap:8px}.casting-detail-actions p{margin:0}
  .waiting-for{color:var(--warn);font-size:12px}
  .step-stage{flex:1 1 auto;min-height:0;display:flex;flex-direction:column;background:var(--canvas)}
  .step-image{flex:1 1 auto;min-height:0;display:flex;align-items:center;justify-content:center;padding:16px;overflow:hidden}
  .step-image :global(img){max-width:100%;max-height:100%;object-fit:contain;border-radius:6px}
  .step-bar{display:flex;align-items:center;gap:8px;padding:8px 12px;border-top:1px solid var(--line);background:var(--panel)}
  .step-bar kbd{font-size:11px;opacity:.7;margin-inline-start:4px}
  .casting-large-image{display:flex;align-items:center;justify-content:center;width:100%;height:100%;min-height:0;padding:0;border:0;border-radius:0;background:var(--canvas);overflow:auto;cursor:zoom-in}
  .casting-large-image:hover:not(:disabled){background:var(--canvas)}
  .casting-large-image :global(img){display:block;width:100%;height:100%;object-fit:contain}
  .casting-large-image.zoom{display:block;cursor:zoom-out}
  .casting-large-image.zoom :global(img){width:auto;height:auto;max-width:none;max-height:none;margin:auto}
  @media(max-width:900px){.casting-preview-layout{grid-template-columns:1fr;grid-template-rows:minmax(0,1fr) auto}.casting-preview-layout aside{max-height:30vh}}
  /* Choice windows (characters / actions) */
  .actions-modes{display:flex;gap:2px;background:var(--raised);border:1px solid var(--line);border-radius:var(--radius);padding:2px}
  .actions-modes button{min-height:22px;border:0;background:transparent;color:var(--muted);white-space:nowrap}
  .actions-modes .chosen{background:var(--hover);color:var(--text)}
  .cibler-backdrop{position:fixed;inset:0;z-index:50;background:rgba(0,0,0,.7);display:grid;place-items:center;padding:24px}
  .cibler-fenetre{width:min(1080px,96vw);max-height:88vh;display:flex;flex-direction:column;gap:10px;background:var(--panel);border:1px solid var(--line-strong);border-radius:var(--radius-lg);padding:14px}
  .cibler-fenetre header{display:flex;gap:8px;align-items:center}
  .cibler-fenetre header>div{flex:1;min-width:0}
  .cibler-fenetre h2{margin:0;font-size:14px;font-weight:600}
  .cibler-fenetre header small{display:block;color:var(--muted);font-size:11px;margin-top:2px}
  .cibler-fenetre header input{width:220px}
  .cibler-filtre{display:flex;flex-direction:row;align-items:center;gap:6px;font-size:11.5px;color:var(--muted);white-space:nowrap}
  .cibler-filtre select{width:140px}
  .cibler-grille{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:10px;overflow:auto;min-height:200px;flex:1;padding:2px;background:var(--canvas);border-radius:var(--radius)}
  .cibler-tuile{display:flex;flex-direction:column;gap:3px;padding:6px;min-height:0;border:1px solid transparent;border-radius:var(--radius);background:transparent;min-width:0;text-align:start}
  .cibler-tuile:hover:not(:disabled){background:var(--raised)}
  .cibler-tuile strong{font-size:12px;font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .cibler-tuile small{font-size:10.5px;color:var(--muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .cibler-tuile[data-selected="oui"]{border-color:var(--accent);background:var(--raised)}
  .action-tuile[data-selectionne="oui"]{border-color:var(--accent);background:var(--raised)}
  .cibler-portrait{display:block;aspect-ratio:832/1216;border-radius:var(--r-img);overflow:hidden;background:var(--panel)}
  .cibler-portrait :global(img){width:100%;height:100%;object-fit:cover}
  .cibler-sans-portrait{display:grid;place-items:center;height:100%;color:var(--faint);font-size:11px}
  .cibler-fenetre footer{display:flex;gap:10px;align-items:center}
  .cibler-fenetre footer span{flex:1;font-size:12px;color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .cibler-fenetre footer button{white-space:nowrap}
  .actions-grille{grid-template-columns:repeat(auto-fill,minmax(160px,1fr));align-content:start}
  .action-tuile em{display:-webkit-box;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;font-size:10.5px;color:var(--faint);font-style:normal;line-height:1.35}
  .cibler-grille .empty-prompts{margin:30px auto}
  @media(max-width:900px){.cibler-fenetre header{flex-wrap:wrap}.cibler-fenetre header input{width:100%}}
</style>
