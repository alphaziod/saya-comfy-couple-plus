use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use studio_core::{
    ablab::CampaignSpec,
    comfy::Comfy,
    contracts::{ApiError, Bootstrap},
    couple::CoupleInput,
    gallery::{image_data, Filters},
    graph_session::{EditOp, GraphSession},
    modules::ModuleConnection,
    origins::OriginInput,
    workflow::{Edit, JobCharacter},
    Database, Result,
};
use tauri::Manager;

struct TemporaryImage {
    launch: Option<String>,
    run: Option<Value>,
    job: Option<studio_core::workflow::Prepared>,
}

struct CastingPreparation {
    stage: &'static str,
    completed: usize,
    total: usize,
    started: std::time::Instant,
}
struct CastingPreparationGuard<'a>(&'a Mutex<Option<CastingPreparation>>);
impl<'a> CastingPreparationGuard<'a> {
    fn start(state: &'a Mutex<Option<CastingPreparation>>, total: usize) -> Result<Self> {
        *state.lock().map_err(|_| "Casting progress lock")? = Some(CastingPreparation {
            stage: "draw",
            completed: 0,
            total,
            started: std::time::Instant::now(),
        });
        Ok(Self(state))
    }
    fn update(&self, stage: &'static str, completed: usize, total: usize) -> Result<()> {
        if let Some(progress) = self.0.lock().map_err(|_| "Casting progress lock")?.as_mut() {
            progress.stage = stage;
            progress.completed = completed;
            progress.total = total;
        }
        Ok(())
    }
}
impl Drop for CastingPreparationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            *state = None;
        }
    }
}

struct AppState {
    comfy_port: u16,
    engine_operations: Mutex<()>,
    saya_operations: Mutex<()>,
    casting_operations: Mutex<()>,
    casting_preparation: Mutex<Option<CastingPreparation>>,
    temporary_images: Mutex<std::collections::HashMap<String, TemporaryImage>>,
    db: Mutex<Database>,
    directory: PathBuf,
    index: Mutex<IndexState>,
    visual: Mutex<VisualState>,
    /// Interactive graph editing sessions (state kept in Rust, u64-exact).
    editors: Mutex<std::collections::HashMap<String, GraphSession>>,
    monitor: Mutex<Option<studio_core::live_transport::Monitor>>,
    bridge: Mutex<Option<std::process::Child>>,
    /// Node catalog (`node_catalog`), derived only from `/object_info`.
    /// Measured: 0.7 s per call (15.9 MB fetched, parsed, transformed), paid on EVERY
    /// mount of Generation/Characters/Graph editor. The 60 s TTL cache does not change
    /// perceived freshness: a newly installed ComfyUI node shows up within a minute at
    /// most, and the engine is not restarted during a session.
    node_catalog: Mutex<Option<(std::time::Instant, Value)>>,
}
#[derive(Default, serde::Serialize)]
struct IndexState {
    running: bool,
    cancel_requested: bool,
    completed: u32,
    total: u32,
    error: Option<String>,
}
#[derive(Default, serde::Serialize)]
struct VisualState {
    running: bool,
    cancel_requested: bool,
    completed: u32,
    total: u32,
    error: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Request {
    Gallery {
        filters: Filters,
    },
    VisualSearch {
        asset: String,
        #[serde(default)]
        limit: Option<u32>,
    },
    VisualPending {
        #[serde(default)]
        limit: Option<u32>,
    },
    VisualIndex {
        #[serde(default)]
        budget: Option<u32>,
    },
    VisualStatus,
    VisualCancel,
    GrokScenePreview {
        paths: Vec<String>,
    },
    GrokSceneImport {
        choices: Vec<studio_core::grok_scenes::SceneChoice>,
        confirmed: bool,
    },
    GrokScenes,
    GrokScene {
        id: String,
    },
    GrokDirectoryFiles {
        paths: Vec<String>,
    },
    GrokPreview {
        paths: Vec<String>,
    },
    GrokImport {
        choices: Vec<studio_core::grok::Choice>,
        confirmed: bool,
    },
    PortraitCandidates {
        character: String,
        offset: u32,
    },
    ChoosePortrait {
        character: String,
        asset: String,
    },
    GalleryNeighbors {
        filters: Filters,
        id: String,
        fallback: u32,
    },
    GenerationMetadata {
        id: String,
    },
    SetCatalogRemoved {
        ids: Vec<String>,
        removed: bool,
    },
    RefreshAssetPresence {
        id: String,
    },
    PrepareAssetTrash {
        ids: Vec<String>,
    },
    TrashAssets {
        plan: Vec<studio_core::asset_management::TrashFile>,
        confirmed: bool,
    },
    OpenAssetDirectory {
        id: String,
    },
    LibrarySources,
    AddLibrarySource {
        path: String,
    },
    SetLibrarySource {
        id: String,
        enabled: bool,
    },
    OpenDirectory {
        path: String,
    },
    IndexStatus,
    CancelIndex,
    IndexErrors {
        run: i64,
        offset: u32,
    },
    LiveState {
        #[serde(default)]
        since: Option<u64>,
    },
    Annotate {
        id: String,
        favorite: bool,
        note: String,
        review: String,
    },
    Facets,
    CharacterCards,
    CharacterSheet {
        id: String,
    },
    GalleryWindow {
        filters: Filters,
        limit: u32,
    },
    Characters {
        #[serde(default)]
        include_archived: bool,
    },
    ArchiveCharacter {
        id: String,
        archived: bool,
    },
    CharacterHistory {
        id: String,
    },
    RestoreCharacter {
        id: String,
        revision: u32,
    },
    Variants {
        id: String,
    },
    SaveVariant {
        character: String,
        id: Option<String>,
        name: String,
        blocks: Value,
        constraints: Value,
    },
    ExportCharacter {
        id: String,
    },
    ImportCharacter {
        text: String,
    },
    References {
        character: String,
    },
    ReferenceHistory {
        character: String,
        before: Option<i64>,
    },
    ReferenceEdit {
        character: String,
        variant: Option<String>,
        asset: String,
        operation: String,
    },
    ReferenceOrder {
        character: String,
        variant: Option<String>,
        assets: Vec<String>,
    },
    Preferences,
    DataPaths,
    BackupEstimate {
        include_pass_states: bool,
    },
    BackupDatabase {
        destination: String,
        #[serde(default)]
        include_pass_states: bool,
    },
    VerifyBackup {
        directory: String,
    },
    BackupDependencies {
        directory: String,
    },
    RestoreBackup {
        source: String,
        destination: String,
    },
    Checkpoints {
        job: String,
    },
    CaptureCheckpoint {
        job: String,
        asset: String,
        max_bytes: u64,
    },
    VerifyCheckpoint {
        id: String,
    },
    CheckpointImage {
        id: String,
    },
    ReviewCheckpoint {
        id: String,
        accept: bool,
        note: String,
    },
    Jobs,
    CharacterJobs {
        character: String,
        before: Option<String>,
    },
    JobDetails {
        id: String,
    },
    ControlJob {
        id: String,
        operation: String,
        confirmed: bool,
    },
    Workflows,
    Connection,
    Models,
    Index {
        root: String,
    },
    Image {
        id: String,
        #[serde(default)]
        preview: bool,
    },
    EditorLoad {
        id: String,
    },
    EditorExport {
        id: String,
        destination: String,
        format: String,
        image: String,
    },
    BlurDocument {
        asset: String,
    },
    BlurSave {
        asset: String,
        document: Value,
    },
    Detail {
        id: String,
    },
    SaveCharacter {
        id: Option<String>,
        name: String,
        identity: Value,
    },
    DeleteCharacter {
        id: String,
    },
    Associate {
        character: String,
        asset: String,
        add: bool,
    },
    /// Detected ComfyUI output folder and where it came from (probes the running ComfyUI briefly).
    ComfyOutputInfo,
    SetPreference {
        key: String,
        value: Value,
    },
    ImportWorkflow {
        name: String,
        text: String,
    },
    Workflow {
        id: String,
    },
    SaveBindings {
        id: String,
        fields: Vec<Edit>,
    },
    Prepare {
        id: String,
        edits: Vec<Edit>,
        #[serde(default)]
        characters: Vec<JobCharacter>,
        #[serde(default)]
        origin: Option<OriginInput>,
    },
    JobProvenance {
        id: String,
    },
    GraphOpenFile {
        path: String,
    },
    GenerationPreset {
        session: String,
        source_path: Option<String>,
    },
    GraphNew,
    NodeCatalog,
    LoraLabels,
    ModelLibrary {
        kind: String,
    },
    ModelThumbnail {
        kind: String,
        path: String,
    },
    ModelDetail {
        kind: String,
        path: String,
    },
    ModelFavorite {
        kind: String,
        path: String,
        favorite: bool,
    },
    ModelFetchMetadata {
        kind: String,
        path: String,
    },
    ModelOrganizePlan {
        kind: String,
    },
    ModelOrganizeApply {
        kind: String,
        moves: Vec<(String, String)>,
    },
    ModelOrganizeUndo {
        journal: String,
    },
    ModelImportDownload {
        kind: String,
        path: String,
    },
    CivitaiThumbnail {
        url: String,
    },
    ImageTemporaryOpen {
        path: String,
    },
    ImageTemporaryClose {
        session: String,
    },
    ImageTemporarySubmit {
        session: String,
        #[serde(default)]
        launch: Option<String>,
        #[serde(default)]
        master_seed: Option<String>,
        edits: Vec<studio_core::image_restore::TemporaryEdit>,
        confirmed: bool,
    },
    ImageInspect {
        path: String,
    },
    ImageImportWorkflow {
        path: String,
        name: String,
        #[serde(default)]
        source: Option<String>,
    },
    ImagePrepare {
        path: String,
        workflow: String,
        selection: Vec<(String, String)>,
    },
    CivitaiJobs,
    CivitaiSearch {
        query: String,
        types: String,
        base_models: String,
        limit: u32,
    },
    CivitaiDownload {
        version_id: u64,
        file_id: Option<u64>,
        destination: String,
    },
    CivitaiCancel {
        job: String,
    },
    CivitaiPause {
        job: String,
    },
    CivitaiResume {
        job: String,
    },
    GraphSaveUi {
        session: String,
        name: String,
    },
    GraphOpen {
        workflow: String,
    },
    SplitAnalysis {
        workflow: String,
    },
    SplitPreview {
        workflow: String,
        cuts: Vec<studio_core::workflow_split::Boundary>,
        output: String,
    },
    #[serde(rename_all = "camelCase")]
    SplitSave {
        workflow: String,
        cuts: Vec<studio_core::workflow_split::Boundary>,
        output: String,
        name: String,
        preview_hash: String,
        confirmed: bool,
    },
    Pipelines,
    SavePipeline {
        text: String,
    },
    StartPipelineRun {
        pipeline: String,
        #[serde(default)]
        reuse_models: bool,
    },
    PipelineRun {
        run: String,
    },
    PipelineRuns {
        pipeline: String,
    },
    MemoryStatus,
    AppMemory,
    SemanticStatus,
    SemanticDiagnose,
    PrepareProfiles,
    PrepareProfile {
        id: String,
    },
    SavePrepareProfile {
        id: Option<String>,
        name: String,
        workflow: String,
        mode: String,
        config: Value,
    },
    DeletePrepareProfile {
        id: String,
    },
    SaveSemanticBinding {
        id: String,
        key: String,
        node: String,
        input: String,
    },
    PrepareBindings {
        id: String,
    },
    PrepareUniversal {
        id: String,
    },
    LoraInventory {
        directory: String,
    },
    LoraLoaders {
        workflow: String,
    },
    LoraCover {
        path: String,
    },
    LoraMetadataSave {
        path: String,
        metadata: Value,
    },
    PreviewLora {
        workflow: String,
        config: Value,
    },
    SamplerSteps {
        job: String,
    },
    SayaReviewDecision {
        job: String,
        decision: String,
        confirmed: bool,
    },
    SayaStart {
        source_path: String,
        master_seed: Option<String>,
        isolated_root: String,
    },
    SayaRuns,
    LastQueuedSeed {
        workflow: String,
    },
    SayaStatus {
        run: String,
    },
    SayaContinue {
        run: String,
    },
    SayaRestart {
        run: String,
    },
    SayaStop {
        run: String,
    },
    /// 1-by-1 casting (U1): draws N OCs through the engine (runtime T1), fills the pass-1
    /// source of each OC and starts ONE Saya run per OC, pass 1 only.
    /// Rotations (U2): `perso` runs Mode A (1 character x N times the same pose,
    /// `--poses <ID> --casting N`); `pose_fixe` runs Mode B (1 pose x N random
    /// characters, `--casting N --pose-fixe <action>`). Character targeting (V1):
    /// `persos` runs `--casting N --persos ID1,ID2,...` - EXACTLY these characters, one
    /// shot each, in the requested order. Action targeting (V2): `actions` runs
    /// `--casting N --actions a,b,c` - the N OCs each carry ONE of the targeted actions,
    /// cycling, in the requested order. Without these fields: casting U1.
    CastingLancer {
        /// Engine draw seed, u64 as a string (app seed convention).
        graine: String,
        nombre: u32,
        #[serde(default)]
        famille: Option<String>,
        #[serde(default)]
        perso: Option<String>,
        #[serde(default)]
        pose_fixe: Option<String>,
        #[serde(default)]
        persos: Option<Vec<String>>,
        #[serde(default)]
        actions: Option<Vec<String>>,
        /// Actions EXCLUDED from the draw: reaches the REAL engine draw
        /// (`--sans-actions a,b,c`), never a UI-side mask. Free draw only
        /// (rejected with persos/actions targeting and perso/pose_fixe rotations).
        #[serde(default)]
        exclude: Option<Vec<String>>,
        #[serde(default)]
        genre: Option<String>,
        #[serde(default)]
        subfamily: Option<String>,
        source_path: String,
        /// Step-by-step mode: only the first trial is submitted, `casting_next` releases more.
        #[serde(default)]
        step: bool,
        /// Draft mode: the targeted `actions` are action drafts, drawn from the test copy
        /// of the engine (`action_drafts`), never from the real one.
        #[serde(default)]
        drafts: bool,
        /// With `perso`: that character on every validated action (or only the targeted
        /// `actions`), one trial each, instead of N copies of one pose.
        #[serde(default)]
        all_actions: bool,
    },
    /// Action drafts (Casting "Brouillons"): list, families and schema fields.
    ActionDrafts,
    /// Create or update a draft in the engine test copy (real engine untouched).
    ActionDraftSave {
        id: String,
        famille: String,
        /// The action fields (`action` is the IPC tag).
        entry: Value,
    },
    ActionDraftDelete {
        id: String,
    },
    /// The user's YES: add the draft to the real engine (`solo action add`, backup kept).
    ActionDraftPromote {
        id: String,
    },
    /// Rebuild the test copy from the real engine and replay pending drafts.
    ActionDraftsRefresh,
    /// Step-by-step casting: release the next trial of this run's batch.
    CastingNext {
        run: String,
    },
    /// Targetable casting characters (V1): engine registry of validated ones
    /// (`negatifs_figes.json`, read-only) joined with app cards of the exact same name.
    CastingPersos,
    CastingCapabilities,
    /// Casting -> gallery: chosen trials (multi-select), no copy.
    CastingPublish {
        runs: Vec<String>,
    },
    /// Delete a batch: manifests and Saya images not used elsewhere.
    CastingDeleteLot {
        run: String,
    },
    GalleryCleanup,
    CastingPreparationStatus,
    /// Targetable casting action bank (V2): engine `data/actions/**` files,
    /// READ-ONLY, in the historical `_ordre.json` order.
    ActionsLister,
    /// Persist a casting YES through the Database struct (never raw SQLite).
    CastingValider {
        run: String,
        graine: String,
        catalogue: String,
        shot: Value,
    },
    CastingChoices,
    /// Picture of each action / character in the casting pickers (latest trial).
    CastingPreviews,
    CastingHistory {
        #[serde(default)]
        sync: bool,
    },
    CastingDecide {
        run: String,
        keep: bool,
    },
    /// STOP of a casting trial: interrupts the running ComfyUI computation (qualified
    /// `/interrupt` endpoint), removes pending trials from the queue and marks them
    /// `cancelled`; state is persisted (survives restarts).
    CastingStop {
        run: String,
    },
    /// PAUSE of a casting trial AT THE NEXT BOUNDARY: the running sampler is NOT
    /// interrupted (ComfyUI has no pause; `/interrupt` is a cancel and is never sent
    /// here) but no NEW trial is queued. The run state becomes `paused` (persisted).
    CastingPause {
        run: String,
    },
    /// RESUME of a paused casting trial: the queue of remaining trials restarts
    /// without redoing those already obtained (the pause cancelled nothing).
    CastingResume {
        run: String,
    },
    /// Reusable SAVE of a casting result as a character card: creates/attaches the card
    /// by import key (run + stem); the pass-1 asset becomes its reference. NEVER starts
    /// passes 2-6. Returns {characterId}.
    CastingEnregistrerPerso {
        run: String,
        stem: String,
        nom: String,
    },
    /// Real engine script (`~/.config/fish/functions/generez.fish`): `attente`, `poses`
    /// (with `id_perso`), `lcs`, `duo`, `bny20`. Run without a shell, exact argv,
    /// PYTHONHASHSEED=0, captured log returned as is (failure = failure).
    EngineScript {
        script: String,
        #[serde(default)]
        id_perso: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
    },
    /// Generations browser (V5): STRICTLY read-only listing of the three image
    /// sources (generez engine, ComfyUI copies, app results), per folder, each
    /// folder oldest to newest, family read from the `saya_family` chunk /
    /// catalogue.json / the stem.
    GenerationBrowser,
    /// Render of an image listed by the browser (jailed to the roots, PNG only,
    /// app thumbnail cache - no listed folder is written).
    GenerationBrowserImage {
        path: String,
        #[serde(default)]
        preview: bool,
    },
    ExportPrepareProfile {
        id: String,
    },
    ImportPrepareProfile {
        text: String,
        workflow: String,
    },
    AuditPrepareProfile {
        id: String,
        workflow: String,
    },
    RemapPrepareProfile {
        id: String,
        workflow: String,
        mappings: Value,
        confirmed: bool,
    },
    SemanticIndex {
        budget: u32,
    },
    SemanticSearch {
        asset: String,
        limit: u32,
    },
    SemanticInvalidate,
    ReleasePipelineMemory {
        run: String,
        confirmed: bool,
    },
    RunNextPass {
        run: String,
        confirmed: bool,
        #[serde(default)]
        unload_after_pass: bool,
    },
    CancelPipelineRun {
        run: String,
        confirmed: bool,
    },
    GraphEdit {
        session: String,
        op: EditOp,
    },
    GraphView {
        session: String,
    },
    GraphEnterSubgraph {
        session: String,
        container: String,
    },
    GraphExitSubgraph {
        session: String,
        commit: bool,
    },
    GraphScopedWidget {
        session: String,
        path: Vec<String>,
        edit: Option<EditOp>,
    },
    GraphSessions,
    GraphResume {
        id: String,
    },
    GraphDeleteSession {
        id: String,
    },
    GraphCopy {
        session: String,
        nodes: Vec<String>,
    },
    GraphPaste {
        session: String,
        fragment: Value,
        offset: studio_core::graph_editor::Point,
    },
    GraphPrepareApi {
        session: String,
    },
    #[serde(rename_all = "camelCase")]
    GraphSaveApi {
        session: String,
        name: String,
        preview_hash: String,
        confirmed: bool,
    },
    GraphExport {
        session: String,
        name: String,
    },
    /// Visual extraction of a module from the edit session: the selection is
    /// translated to original ids, ports derived from crossing links, the versioned
    /// module carries the session provenance.
    #[serde(rename_all = "camelCase")]
    GraphExtractModule {
        session: String,
        nodes: Vec<String>,
        module: Option<String>,
        name: String,
        description: String,
    },
    GraphClose {
        session: String,
    },
    AssetProvenance {
        id: String,
    },
    CoupleInspection {
        id: String,
    },
    PrepareCouple {
        id: String,
        input: CoupleInput,
    },
    Scenes,
    Scene {
        id: String,
    },
    SaveScene {
        id: Option<String>,
        name: String,
        manifest: studio_core::scenes::SceneManifest,
    },
    DuplicateScene {
        id: String,
    },
    DeleteScene {
        id: String,
    },
    PreviewScene {
        manifest: studio_core::scenes::SceneManifest,
    },
    Modules,
    Module {
        id: String,
        #[serde(default)]
        version: Option<u32>,
    },
    ModuleCandidates {
        id: String,
        #[serde(default)]
        version: Option<u32>,
        workflow: String,
    },
    WorkflowNodes {
        id: String,
    },
    RecipeApplication {
        id: String,
        workflow: String,
    },
    ModuleInsertPreview {
        id: String,
        #[serde(default)]
        version: Option<u32>,
        workflow: String,
        instance: String,
        connection: ModuleConnection,
    },
    SaveModule {
        workflow: String,
        #[serde(default)]
        module: Option<String>,
        name: String,
        description: String,
        selection: Vec<String>,
    },
    CompareModuleVersions {
        id: String,
        a: u32,
        b: u32,
    },
    RollbackModule {
        id: String,
        version: u32,
    },
    ConnectModule {
        id: String,
        version: u32,
        instance: String,
        pipeline: Value,
        connection: ModuleConnection,
    },
    Submit {
        job: String,
        hash: String,
        confirmed: bool,
        allow_removals: bool,
    },
    GeneratorCatalog,
    PrepareGeneratedVariant {
        character: String,
        input: studio_core::generator::GeneratorInput,
    },
    SaveGeneratedVariant {
        character: String,
        name: String,
        input: studio_core::generator::GeneratorInput,
        revision: u32,
        confirmed: bool,
    },
    GenerateCharacter {
        input: studio_core::generator::GeneratorInput,
    },
    SuggestCharacter {
        constraints: Value,
        seed: String,
    },
    EngineTool {
        root: String,
        tool: String,
        args: Value,
    },
    Proposals,
    Proposal {
        id: String,
    },
    DecideProposal {
        id: String,
        decision: String,
        confirmed: bool,
        current: String,
    },
    Recipes,
    Recipe {
        id: String,
    },
    RecipeHistory {
        id: String,
    },
    CreateRecipe {
        job: String,
        name: String,
        #[serde(default)]
        description: String,
    },
    SaveRecipeEdit {
        id: String,
        content: Value,
        #[serde(default)]
        note: String,
        version: u32,
    },
    DuplicateRecipe {
        id: String,
        name: String,
    },
    CompareRecipes {
        left: String,
        right: String,
    },
    ExportRecipe {
        id: String,
    },
    ImportRecipe {
        text: String,
    },
    ApplyRecipe {
        id: String,
        workflow: String,
    },
    SyncJobs,
    GenerationTree {
        root: String,
    },
    GenerationPath {
        id: String,
    },
    GenerationBranches {
        id: String,
    },
    GenerationCompare {
        left: String,
        right: String,
    },
    RecordTreeRelation {
        parent: String,
        child: String,
        relation: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        checkpoint: Option<String>,
    },
    PlanTreeBranch {
        parent: String,
        relation: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        checkpoint: Option<String>,
    },
    LinkTreeBranch {
        relation: String,
        child: String,
    },
    AbCampaigns,
    AbCampaign {
        id: String,
    },
    SaveAbCampaign {
        name: String,
        workflow: String,
        model: String,
        loras: Value,
        prompts: Value,
        base_seed: String,
        steps: i64,
        cfg: f64,
        sampler: String,
        grid_limit: i64,
        #[serde(default)]
        notes: String,
    },
    AddAbParameter {
        campaign: String,
        slot: String,
        kind: String,
        label: String,
        values: Vec<Value>,
    },
    PrepareAbGrid {
        campaign: String,
    },
    AbPlan {
        campaign: String,
    },
    PrepareAbCells {
        campaign: String,
        cells: Vec<String>,
    },
    SyncAbResults {
        campaign: String,
    },
    AbSpecFromRecipe {
        recipe: String,
    },
    AbCellDiff {
        campaign: String,
        cell_a: String,
        cell_b: String,
    },
    SetAbVerdict {
        campaign: String,
        cell: String,
        favorite: bool,
        best: bool,
        rank: Option<i64>,
        #[serde(default)]
        note: String,
    },
    RegisterAbCellImage {
        campaign: String,
        cell: String,
        job: String,
        asset: String,
    },
    SaveAbWinnerPreset {
        campaign: String,
        cell: String,
        name: String,
    },
}
fn error(message: impl ToString) -> ApiError {
    ApiError {
        code: "operation_failed",
        message: message.to_string(),
        retryable: false,
    }
}
#[cfg(feature = "smoke-test")]
static SMOKE_ACTIONS: Mutex<Vec<String>> = Mutex::new(Vec::new());
#[cfg(feature = "smoke-test")]
#[tauri::command]
fn smoke_actions() -> Vec<String> {
    SMOKE_ACTIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}
/// Civitai API token stored next to the database; a missing file means "no token" (anonymous),
/// any other read failure is surfaced instead of silently downgrading to anonymous.
fn read_civitai_token(s: &AppState) -> Result<String> {
    match std::fs::read_to_string(s.directory.join("civitai-token")) {
        Ok(token) => Ok(token),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Lecture du jeton Civitai impossible : {e}").into()),
    }
}
fn persist_editor_session(
    db: &studio_core::Database,
    id: &str,
    snapshot: &Value,
) -> studio_core::Result<()> {
    if id.starts_with("temporary-") {
        return Ok(());
    }
    db.save_editor_session(
        id,
        snapshot["name"].as_str().unwrap_or("Session d'édition"),
        snapshot["workflow"].as_str().unwrap_or(""),
        snapshot["sourceFormat"].as_str().unwrap_or("comfy_api"),
        snapshot["document"]
            .as_str()
            .ok_or("Document de session absent")?,
        snapshot["uiSource"].as_str(),
        snapshot["originalApi"].as_str(),
        snapshot["originalIds"].as_str().unwrap_or("{}"),
    )
}

/// Perf instrumentation: server-side duration of each IPC action, traced on stderr
/// only when SAYA_STUDIO_TRACE=1 (the app behaves identically without the variable;
/// no cost when absent beyond a single Instant::now per action).
fn handle(s: &AppState, request: Request) -> Result<Value> {
    if std::env::var("SAYA_STUDIO_TRACE").as_deref() != Ok("1") {
        return handle_inner(s, request);
    }
    let action = match &request {
        Request::Image { .. } => "image",
        Request::ImageTemporaryOpen { .. } => "image_temporary_open",
        Request::NodeCatalog => "node_catalog",
        Request::SetPreference { .. } => "set_preference",
        Request::Preferences => "preferences",
        Request::Gallery { .. } => "gallery",
        Request::GalleryWindow { .. } => "gallery_window",
        Request::GalleryNeighbors { .. } => "gallery_neighbors",
        Request::Facets => "facets",
        Request::Detail { .. } => "detail",
        Request::Index { .. } => "index",
        Request::Characters { .. } => "characters",
        Request::CharacterCards => "character_cards",
        Request::Prepare { .. } => "prepare",
        Request::Submit { .. } => "submit",
        Request::Connection => "connection",
        Request::Jobs => "jobs",
        Request::CastingLancer { .. } => "casting_lancer",
        Request::CastingHistory { .. } => "casting_history",
        Request::CastingNext { .. } => "casting_next",
        Request::SayaStatus { .. } => "saya_status",
        Request::SayaRuns => "saya_runs",
        _ => "other",
    };
    let start = std::time::Instant::now();
    let result = handle_inner(s, request);
    eprintln!(
        "[perf] action={action} ms={:.1} ok={}{}",
        start.elapsed().as_secs_f64() * 1000.0,
        result.is_ok(),
        result.as_ref().err().map(|e| format!(" error={e} debug={e:?}")).unwrap_or_default()
    );
    result
}

/// Generation browser roots, with the configured capture root (deliveries) when set.
fn browser_roots(s: &AppState) -> Vec<studio_core::generation_browser::Root> {
    let captures = Database::open(&s.directory.join("studio.sqlite3"))
        .and_then(|db| db.capture_root())
        .ok()
        .flatten();
    studio_core::generation_browser::roots(&s.directory.join("results"), captures.as_deref())
}

/// Action drafts (Casting "Brouillons"): served without the database lock, under the Saya
/// operation lock (the engine copy and the real engine are written by subprocesses).
fn action_drafts_request(s: &AppState, request: Request) -> Result<Value> {
    use studio_core::action_drafts;
    let root = s.directory.join("action-drafts");
    let engine = studio_core::engine_runtime::engine_dir();
    let _operation = s
        .saya_operations
        .lock()
        .map_err(|_| "Saya operation lock")?;
    Ok(match request {
        Request::ActionDrafts => action_drafts::overview(&root, &engine)?,
        Request::ActionDraftSave { id, famille, entry } => {
            action_drafts::save(&root, &engine, &id, &famille, &entry)?
        }
        Request::ActionDraftDelete { id } => {
            action_drafts::delete(&root, &id)?;
            Value::Null
        }
        Request::ActionDraftPromote { id } => action_drafts::promote(&root, &engine, &id)?,
        Request::ActionDraftsRefresh => {
            action_drafts::ensure_sandbox(&root, &engine, true)?;
            action_drafts::overview(&root, &engine)?
        }
        _ => unreachable!("action_drafts_request receives action-draft requests only"),
    })
}

fn handle_inner(s: &AppState, request: Request) -> Result<Value> {
    #[cfg(feature = "smoke-test")]
    {
        let action = match &request {
            Request::SaveBindings { .. } => Some("save_bindings"),
            Request::Prepare { .. } => Some("prepare"),
            Request::PrepareCouple { .. } => Some("prepare_couple"),
            Request::Submit { .. } => Some("submit"),
            Request::SaveCharacter { .. } => Some("save_character"),
            Request::SaveVariant { .. } => Some("save_variant"),
            Request::SaveScene { .. } => Some("save_scene"),
            Request::DeleteScene { .. } => Some("delete_scene"),
            Request::SaveModule { .. } => Some("save_module"),
            Request::GraphExtractModule { .. } => Some("graph_extract_module"),
            Request::RollbackModule { .. } => Some("rollback_module"),
            Request::SetPreference { .. } => Some("set_preference"),
            Request::Index { .. } => Some("index"),
            _ => None,
        };
        if let Some(action) = action {
            SMOKE_ACTIONS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(action.into());
        }
        if matches!(request, Request::Preferences)
            && std::env::var("SAYA_STUDIO_SMOKE_SLOW_PREFERENCES").as_deref() == Ok("1")
        {
            let snapshot = s.db.lock().map_err(|_| "Database lock")?.preferences()?;
            std::thread::sleep(std::time::Duration::from_millis(700));
            return Ok(snapshot);
        }
    }
    match request {
        Request::CastingPreparationStatus => {
            let progress = s
                .casting_preparation
                .lock()
                .map_err(|_| "Casting progress lock")?;
            return Ok(match progress.as_ref() {
                Some(p) => {
                    json!({"active":true,"stage":p.stage,"completed":p.completed,"total":p.total,"elapsedSeconds":p.started.elapsed().as_secs()})
                }
                None => json!({"active":false}),
            });
        }
        Request::CaptureCheckpoint {
            job,
            asset,
            max_bytes,
        } => {
            let _operation = s
                .engine_operations
                .try_lock()
                .map_err(|_| "Une opération moteur est déjà en cours")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.capture_image_checkpoint(
                &s.directory.join("checkpoints"),
                &job,
                &asset,
                max_bytes,
            );
        }
        Request::VerifyCheckpoint { id } => {
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.verify_checkpoint(&s.directory.join("checkpoints"), &id);
        }
        Request::CheckpointImage { id } => {
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let manifest = db.verify_checkpoint(&s.directory.join("checkpoints"), &id)?;
            return Ok(json!(image_data(
                &s.directory
                    .join("checkpoints")
                    .join(manifest["file"].as_str().ok_or("Checkpoint invalide")?),
                &s.directory.join("thumbnails"),
                true
            )?));
        }
        Request::ReviewCheckpoint { id, accept, note } => {
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            db.review_checkpoint(&s.directory.join("checkpoints"), &id, accept, &note)?;
            return Ok(Value::Null);
        }
        Request::DataPaths => {
            return Ok(
                json!({"database":s.directory.join("studio.sqlite3"),"suggestedBackup":s.directory.join(format!("backup-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis()))}),
            )
        }
        Request::BackupEstimate {
            include_pass_states,
        } => {
            return studio_core::backup::estimate(
                &s.directory.join("studio.sqlite3"),
                include_pass_states,
            );
        }
        Request::BackupDatabase {
            destination,
            include_pass_states,
        } => {
            let _operation = s
                .engine_operations
                .try_lock()
                .map_err(|_| "Une opération moteur est déjà en cours")?;
            return Ok(json!(studio_core::backup::create_with_options(
                &s.directory.join("studio.sqlite3"),
                std::path::Path::new(&destination),
                include_pass_states
            )?));
        }
        Request::BackupDependencies { directory } => {
            // Read-only report on a backup or restored copy (never migrated).
            return studio_core::backup::dependency_report(std::path::Path::new(&directory));
        }
        Request::VerifyBackup { directory } => {
            return Ok(json!(studio_core::backup::verify(std::path::Path::new(
                &directory
            ))?))
        }
        Request::RestoreBackup {
            source,
            destination,
        } => {
            return Ok(json!(studio_core::backup::restore(
                std::path::Path::new(&source),
                std::path::Path::new(&destination)
            )?))
        }
        Request::LiveState { since } => {
            let monitor = s.monitor.lock().map_err(|_| "Monitor lock")?;
            let snapshot = monitor
                .as_ref()
                .ok_or("Suivi moteur non démarré")?
                .snapshot()?;
            return Ok(if since == Some(snapshot.revision) {
                Value::Null
            } else {
                json!(snapshot)
            });
        }
        Request::EngineTool { root, tool, args } => {
            let response = studio_core::engine::call(std::path::Path::new(&root), &tool, &args)?;
            if tool == "optimize" {
                let db = Database::open(&s.directory.join("studio.sqlite3"))?;
                let id = db.save_proposal(&root, &args, &response)?;
                return Ok(json!({"proposalId":id,"response":response}));
            }
            return Ok(response);
        }
        Request::CancelIndex => {
            let mut state = s.index.lock().map_err(|_| "Index lock unavailable")?;
            if state.running {
                state.cancel_requested = true;
            }
            return Ok(Value::Null);
        }
        Request::IndexStatus => {
            let mut state =
                serde_json::to_value(&*s.index.lock().map_err(|_| "Index lock unavailable")?)?;
            state["lastRun"] =
                s.db.lock()
                    .map_err(|_| "Database lock unavailable")?
                    .latest_index_run()?;
            return Ok(state);
        }
        Request::Index { root } => {
            {
                let mut status = s.index.lock().map_err(|_| "Index lock unavailable")?;
                if status.running {
                    return Err("Une indexation est déjà en cours".into());
                }
                *status = IndexState {
                    running: true,
                    ..Default::default()
                };
            }
            let result = (|| -> Result<Value> {
                let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
                let report = db.index_library_controlled(
                    std::path::Path::new(&root),
                    |completed, total| {
                        if let Ok(mut status) = s.index.lock() {
                            status.completed = completed;
                            status.total = total;
                        }
                    },
                    || {
                        s.index
                            .lock()
                            .map(|status| status.cancel_requested)
                            .unwrap_or(true)
                    },
                )?;
                db.set_preference("libraryRoot", json!(root))?;
                Ok(json!(report))
            })();
            let mut status = s.index.lock().map_err(|_| "Index lock unavailable")?;
            status.running = false;
            status.error = result.as_ref().err().map(ToString::to_string);
            return result;
        }
        Request::VisualCancel => {
            let mut state = s
                .visual
                .lock()
                .map_err(|_| "Visual index lock unavailable")?;
            if state.running {
                state.cancel_requested = true;
            }
            return Ok(Value::Null);
        }
        Request::VisualStatus => {
            let mut state = serde_json::to_value(
                &*s.visual
                    .lock()
                    .map_err(|_| "Visual index lock unavailable")?,
            )?;
            state["pending"] =
                s.db.lock()
                    .map_err(|_| "Database lock unavailable")?
                    .visual_pending(0)?["count"]
                    .clone();
            return Ok(state);
        }
        Request::VisualIndex { budget } => {
            {
                let mut status = s
                    .visual
                    .lock()
                    .map_err(|_| "Visual index lock unavailable")?;
                if status.running {
                    return Err("Une analyse visuelle est déjà en cours".into());
                }
                *status = VisualState {
                    running: true,
                    ..Default::default()
                };
            }
            let result = (|| -> Result<Value> {
                let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
                let report = db.visual_backfill(
                    budget,
                    |completed, total| {
                        if let Ok(mut status) = s.visual.lock() {
                            status.completed = completed;
                            status.total = total;
                        }
                    },
                    || {
                        s.visual
                            .lock()
                            .map(|status| status.cancel_requested)
                            .unwrap_or(true)
                    },
                )?;
                Ok(report)
            })();
            let mut status = s
                .visual
                .lock()
                .map_err(|_| "Visual index lock unavailable")?;
            status.running = false;
            status.error = result.as_ref().err().map(ToString::to_string);
            return result;
        }
        Request::VisualSearch { asset, limit } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.visual_search(&asset, limit.unwrap_or(24));
        }
        Request::VisualPending { limit } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.visual_pending(limit.unwrap_or(10));
        }
        Request::SaveModule {
            workflow,
            module,
            name,
            description,
            selection,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return Ok(json!(db.save_module_from_workflow(
                &workflow,
                module.as_deref(),
                &name,
                &description,
                &selection,
                &info,
            )?));
        }
        Request::Module { id, version } => {
            // Best effort: dependency check against the live engine schema;
            // an offline engine still shows the module, flagged unchecked.
            let info = Comfy::loopback(s.comfy_port)
                .and_then(|c| c.get("/object_info"))
                .ok();
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            let mut detail = db.module_detail(&id, version, info.as_ref())?;
            detail["engineChecked"] = json!(info.is_some());
            return Ok(detail);
        }
        Request::ModuleCandidates {
            id,
            version,
            workflow,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.module_candidates(&id, version, &workflow, &info);
        }
        Request::SemanticStatus => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.semantic_status();
        }
        Request::PrepareProfiles => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.prepare_profiles()?));
        }
        Request::PrepareProfile { id } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.prepare_profile(&id)?));
        }
        Request::SavePrepareProfile {
            id,
            name,
            workflow,
            mode,
            config,
        } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.save_prepare_profile(
                id.as_deref(),
                &name,
                &workflow,
                &mode,
                &config
            )?));
        }
        Request::DeletePrepareProfile { id } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            db.delete_prepare_profile(&id)?;
            return Ok(json!(true));
        }
        Request::SaveSemanticBinding {
            id,
            key,
            node,
            input,
        } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.save_semantic_binding(&id, &key, &node, &input)?));
        }
        Request::PrepareBindings { id } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.prepare_bindings(&id)?));
        }
        Request::ExportPrepareProfile { id } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.export_prepare_profile(&id)?));
        }
        Request::ImportPrepareProfile { text, workflow } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.import_prepare_profile(&text, &workflow)?));
        }
        Request::AuditPrepareProfile { id, workflow } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.audit_prepare_profile(&id, &workflow)?));
        }
        Request::RemapPrepareProfile {
            id,
            workflow,
            mappings,
            confirmed,
        } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(
                db.remap_prepare_profile(&id, &workflow, &mappings, confirmed)?
            ));
        }
        Request::SemanticDiagnose => return Ok(studio_core::semantic_search::diagnose()),
        Request::SemanticIndex { budget } => {
            // Embedding runs a sidecar for seconds: use a dedicated connection, not `s.db`.
            return side_database(s)?.semantic_index(budget);
        }
        Request::SemanticSearch { asset, limit } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.semantic_search(&asset, limit);
        }
        Request::SemanticInvalidate => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(serde_json::json!({"removed": db.semantic_invalidate()?}));
        }
        Request::MemoryStatus => {
            let client = Comfy::loopback(s.comfy_port)?;
            return Ok(serde_json::to_value(studio_core::resources::measure(
                Some(&client),
                s.comfy_port,
                Some(std::path::Path::new("/sys/class/drm")),
            ))?);
        }
        Request::AppMemory => {
            let (total, processes) = studio_core::resources::process_tree_rss(std::process::id())
                .ok_or("Mesure RSS indisponible")?;
            return Ok(serde_json::json!({
                "totalRss": total,
                "processes": processes.iter().map(|(pid, rss)| serde_json::json!({"pid": pid, "rss": rss})).collect::<Vec<_>>(),
            }));
        }
        Request::ReleasePipelineMemory { run, confirmed } => {
            let _operation = s
                .engine_operations
                .try_lock()
                .map_err(|_| "Une opération moteur est déjà en cours")?;
            let manager = studio_core::resources::ComfyMemoryManager {
                client: Comfy::loopback(s.comfy_port)?,
                port: s.comfy_port,
                drm: Some(std::path::PathBuf::from("/sys/class/drm")),
                settle: std::time::Duration::from_secs(3),
                threshold: 1024 * 1024,
            };
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.release_pipeline_memory(&run, &manager, confirmed);
        }
        Request::CancelPipelineRun { run, confirmed } => {
            let _operation = s
                .engine_operations
                .try_lock()
                .map_err(|_| "Une opération moteur est déjà en cours")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.cancel_pipeline_run(&run, &Comfy::loopback(s.comfy_port)?, confirmed);
        }
        Request::RunNextPass {
            run,
            confirmed,
            unload_after_pass,
        } => {
            let _operation = s
                .engine_operations
                .try_lock()
                .map_err(|_| "Une opération moteur est déjà en cours")?;
            // Each pass is a real engine generation: confirmation is required
            // by the core for every submission. Waits at most 60 s, then the
            // run stays resumable (the UI simply calls again).
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let manager = if unload_after_pass {
                Some(studio_core::resources::ComfyMemoryManager {
                    client: Comfy::loopback(s.comfy_port)?,
                    port: s.comfy_port,
                    drm: Some(std::path::PathBuf::from("/sys/class/drm")),
                    settle: std::time::Duration::from_secs(3),
                    threshold: 1024 * 1024,
                })
            } else {
                None
            };
            let memory: Option<&dyn studio_core::pass_states::PassMemoryManager> = manager
                .as_ref()
                .map(|m| m as &dyn studio_core::pass_states::PassMemoryManager);
            return db.run_next_pass_with_memory(
                &run,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
                confirmed,
                std::time::Duration::from_secs(60),
                memory,
                unload_after_pass,
            );
        }
        Request::SplitAnalysis { workflow } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.split_analysis(&workflow, &info);
        }
        Request::SplitPreview {
            workflow,
            cuts,
            output,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.split_preview(&workflow, &cuts, &output, &info);
        }
        Request::SplitSave {
            workflow,
            cuts,
            output,
            name,
            preview_hash,
            confirmed,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.split_save(
                &workflow,
                &cuts,
                &output,
                &name,
                &preview_hash,
                confirmed,
                &info,
            );
        }
        Request::CivitaiSearch {
            query,
            types,
            base_models,
            limit,
        } => {
            return studio_core::civitai::search(&query, &types, &base_models, limit);
        }
        Request::CivitaiDownload {
            version_id,
            file_id,
            destination,
        } => {
            let token = read_civitai_token(s)?;
            return studio_core::civitai::download(&token, version_id, file_id, &destination);
        }
        Request::CivitaiJobs => {
            let token = read_civitai_token(s)?;
            return studio_core::civitai::jobs(&token);
        }
        Request::CivitaiCancel { job } => {
            let token = read_civitai_token(s)?;
            return studio_core::civitai::cancel(&token, &job);
        }
        Request::CivitaiPause { job } => {
            let token = read_civitai_token(s)?;
            return studio_core::civitai::pause(&token, &job);
        }
        Request::CivitaiResume { job } => {
            let token = read_civitai_token(s)?;
            return studio_core::civitai::resume(&token, &job);
        }
        Request::ImageTemporaryOpen { path } => {
            let file = std::path::Path::new(&path);
            let mut inspected = if file
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("json"))
            {
                let document = studio_core::gallery::read_json(file, 16_000_000)?;
                if document.get("nodes").is_some() {
                    json!({"workflow":document})
                } else {
                    json!({"prompt":studio_core::workflow::api_graph(&document.to_string())?})
                }
            } else {
                studio_core::image_restore::inspect(file)?
            };
            let info = Comfy::loopback(s.comfy_port)?
                .get("/object_info")
                .unwrap_or_else(|_| json!({}));
            if inspected["workflow"]["nodes"].is_array() {
                let source = studio_core::saya::generation_preset_source(
                    &inspected["workflow"].to_string(),
                    Some(&path),
                )?;
                inspected["workflow"] = serde_json::from_str(&source)?;
                studio_core::image_restore::activate_workflow(&mut inspected["workflow"]);
            }
            let editor = if inspected["workflow"]["nodes"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty())
            {
                GraphSession::open_file(
                    &inspected["workflow"].to_string(),
                    &info,
                    "Image temporaire",
                )?
            } else if inspected["prompt"].is_object() {
                GraphSession::open("", &inspected["prompt"], &info, "Image temporaire")?
            } else {
                return Err("Ce fichier ne contient aucun workflow ComfyUI exploitable".into());
            };
            let view = editor.view();
            let id = format!("temporary-{}", studio_core::graph_editor::new_id());
            let mut images = s
                .temporary_images
                .lock()
                .map_err(|_| "Temporary image lock")?;
            let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
            if editors.len() >= 8 {
                return Err("Fermer une préparation avant d'en ouvrir une autre".into());
            }
            editors.insert(id.clone(), editor);
            images.insert(
                id.clone(),
                TemporaryImage {
                    launch: None,
                    job: None,
                    run: None,
                },
            );
            return Ok(json!({"session":id,"view":view}));
        }
        Request::ImageTemporaryClose { session } => {
            let mut images = s
                .temporary_images
                .lock()
                .map_err(|_| "Temporary image lock")?;
            if images.remove(&session).is_some() {
                s.editors
                    .lock()
                    .map_err(|_| "Editor lock unavailable")?
                    .remove(&session);
            }
            return Ok(Value::Null);
        }
        Request::ImageTemporarySubmit {
            session,
            launch,
            master_seed,
            edits,
            confirmed,
        } => {
            if !confirmed {
                return Err("Confirmer le lancement de cette image".into());
            }
            let mut images = s
                .temporary_images
                .lock()
                .map_err(|_| "Temporary image lock")?;
            let image = images
                .get_mut(&session)
                .ok_or("Préparation temporaire fermée")?;
            if launch.is_some() && image.launch != launch {
                image.launch = launch;
                image.job = None;
                image.run = None;
            }
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let client = Comfy::loopback(s.comfy_port)?;
            if let Some(run) = &image.run {
                return Ok(json!({"run":run}));
            }
            let ui = {
                let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                editors
                    .get(&session)
                    .ok_or("Workflow temporaire fermé")?
                    .export_ui()
                    // Not every temporary workflow exports to UI format; only Saya ones need it.
                    .ok()
            };
            if let Some(ui) = ui.filter(studio_core::image_restore::is_saya_workflow) {
                let _operation = s
                    .saya_operations
                    .lock()
                    .map_err(|_| "Saya operation lock")?;
                let run = db.saya_start_source(
                    &ui.to_string(),
                    &session,
                    master_seed.as_deref(),
                    &format!("saya-tests/{session}"),
                    &client,
                    &s.directory.join("results"),
                )?;
                image.run = Some(run.clone());
                return Ok(json!({"run":run}));
            }
            if image.job.is_none() {
                let info = client.get("/object_info")?;
                let (graph, ui) = {
                    let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                    let editor = editors.get(&session).ok_or("Workflow temporaire fermé")?;
                    editor.prepare_execution(&info)?
                };
                let mut graph = studio_core::image_restore::edited_temporary_graph(&graph, &edits)?;
                // Same closure as the engine (minimal_prompt): unwired nodes of the
                // embedded workflow (e.g. HiDream PRELOAD) are never submitted.
                studio_core::saya::prune_unreachable_nodes(&mut graph, &info);
                image.job = Some(db.prepare_temporary_image(&graph, &info, ui.as_ref())?);
            }
            // Reuse ownership after a lost response; never create another job on retry.
            let job = image.job.as_ref().ok_or("Brouillon absent")?;
            db.submit_job(&job.job_id, &job.snapshot_hash, true, true, &client)?;
            if let Some(monitor) = s.monitor.lock().map_err(|_| "Monitor lock")?.as_ref() {
                monitor.refresh();
            }
            return Ok(json!({"jobId":job.job_id}));
        }
        Request::ImageInspect { path } => {
            return studio_core::image_restore::inspect(std::path::Path::new(&path))
        }
        Request::ImagePrepare {
            path,
            workflow,
            selection,
        } => {
            let inspected = studio_core::image_restore::inspect(std::path::Path::new(&path))?;
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            let target: Value = serde_json::from_str(&db.workflow_document(&workflow)?.1)?;
            let patches = studio_core::image_restore::selected_patches(
                &inspected["prompt"],
                &target,
                &selection,
            )?;
            return Ok(json!(db.prepare_job_with_characters_extra(
                &workflow,
                &[],
                &info,
                &[],
                &patches
            )?));
        }
        Request::ImageImportWorkflow { path, name, source } => {
            let inspected = studio_core::image_restore::inspect(std::path::Path::new(&path))?;
            if source.as_deref() == Some("api") {
                let prompt = inspected["prompt"]
                    .as_object()
                    .ok_or("Image sans prompt API")?;
                let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
                studio_core::workflow::validate_graph(&Value::Object(prompt.clone()), &info)?;
                let id =
                    s.db.lock()
                        .map_err(|_| "Database lock unavailable")?
                        .import_workflow(&name, &Value::Object(prompt.clone()).to_string())?;
                return Ok(json!({"workflowId":id,"report":{"complete":true}}));
            }
            let workflow = inspected["workflow"]
                .as_object()
                .ok_or("Image sans workflow UI embarqué")?;
            let source = serde_json::Value::Object(workflow.clone()).to_string();
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let prepared = studio_core::workflow_ui_api::prepare_api(&source, Some(&info))?;
            if prepared["report"]["complete"] != true {
                return Err("Conversion UI incomplète : workflow refusé".into());
            }
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            let id = db.import_workflow(&name, &prepared["api"].to_string())?;
            return Ok(json!({"workflowId":id,"report":prepared["report"]}));
        }
        Request::ModelLibrary { kind } => {
            return studio_core::model_library::library(&model_roots(s, &kind)?);
        }
        Request::ModelThumbnail { kind, path } => {
            return Ok(json!(studio_core::model_library::thumbnail(
                &model_roots(s, &kind)?,
                &path
            )?));
        }
        Request::ModelDetail { kind, path } => {
            return studio_core::model_library::detail(&model_roots(s, &kind)?, &path);
        }
        Request::ModelFavorite {
            kind,
            path,
            favorite,
        } => {
            return studio_core::model_library::set_favorite(
                &model_roots(s, &kind)?,
                &path,
                favorite,
            );
        }
        Request::ModelFetchMetadata { kind, path } => {
            return studio_core::model_library::fetch_metadata(
                &model_roots(s, &kind)?,
                &path,
                &studio_core::civitai::api_token(),
            );
        }
        Request::ModelOrganizePlan { kind } => {
            return studio_core::model_library::organize_plan(&model_roots(s, &kind)?);
        }
        Request::ModelOrganizeApply { kind, moves } => {
            let result = studio_core::model_library::organize_apply(
                &model_roots(s, &kind)?,
                &moves,
                &s.directory.join("organize-journal"),
            )?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            db.set_preference("modelOrganizeJournal", result["journal"].clone())?;
            return Ok(result);
        }
        Request::ModelOrganizeUndo { journal } => {
            let journals = s
                .directory
                .join("organize-journal")
                .canonicalize()
                .map_err(|_| "Journal de rangement inconnu")?;
            let path = std::path::Path::new(&journal)
                .canonicalize()
                .map_err(|_| "Journal de rangement inconnu")?;
            if !path.starts_with(&journals) || !path.is_file() {
                return Err("Journal de rangement inconnu".into());
            }
            // Organize only ever moves inside these tabs' roots; undo is held to the same roots.
            let mut roots = Vec::new();
            for kind in ["loras", "checkpoints", "embeddings"] {
                if let Ok(found) = model_roots(s, kind) {
                    roots.extend(found);
                }
            }
            return studio_core::model_library::organize_undo(&roots, &path);
        }
        Request::ModelImportDownload { kind, path } => {
            // Only files the Civitai bridge wrote into its own download folders can be imported.
            let downloads = civitai_download_roots(s);
            let file = std::path::Path::new(&path).canonicalize()?;
            if !downloads.iter().any(|root| file.starts_with(root)) || !file.is_file() {
                return Err("Fichier hors du dossier de téléchargement Civitai".into());
            }
            let roots = model_roots(s, &kind)?;
            let token = studio_core::civitai::api_token();
            let sha = studio_core::model_library::file_sha256(&file)?;
            let (code, body) = studio_core::model_library::https_get(
                &format!("https://civitai.red/api/v1/model-versions/by-hash/{sha}"),
                &token,
                16 * 1024 * 1024,
            )?;
            let version: Value = if code == 200 {
                serde_json::from_slice(&body)?
            } else {
                Value::Null
            };
            let tags: Vec<String> = match version["modelId"].as_u64() {
                Some(id) => studio_core::model_library::https_get(
                    &format!("https://civitai.red/api/v1/models/{id}"),
                    &token,
                    16 * 1024 * 1024,
                )
                // Best effort: tags only enrich the import; log and continue without them.
                .map_err(|e| eprintln!("Civitai model tags unavailable for {id}: {e}"))
                .ok()
                .and_then(|(_, b)| serde_json::from_slice::<Value>(&b).ok())
                .map(|m| {
                    m["tags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
                None => Vec::new(),
            };
            let target = studio_core::model_library::import_download(
                &roots,
                &file,
                version["baseModel"].as_str().unwrap_or(""),
                &tags,
            )?;
            let metadata = studio_core::model_library::fetch_metadata(
                &roots,
                &target.to_string_lossy(),
                &token,
            )
            .unwrap_or_else(|e| json!({"found": false, "error": e.to_string()}));
            return Ok(json!({"path": target.to_string_lossy(), "metadata": metadata}));
        }
        Request::CivitaiThumbnail { url } => {
            return Ok(json!(studio_core::model_library::civitai_thumbnail(&url)?));
        }
        Request::LoraLabels => {
            // Read-only display names from the installed LoraManager; files stay the engine's list.
            let comfy = Comfy::loopback(s.comfy_port)?;
            let mut labels = Vec::new();
            for page in 1..=20 {
                let r = comfy.get(&format!("/api/lm/loras/list?page_size=500&page={page}"))?;
                for item in r["items"].as_array().into_iter().flatten() {
                    let (Some(file), Some(name)) =
                        (item["file_name"].as_str(), item["model_name"].as_str())
                    else {
                        continue;
                    };
                    let folder = item["folder"].as_str().unwrap_or("");
                    let path = if folder.is_empty() {
                        file.to_owned()
                    } else {
                        format!("{folder}/{file}")
                    };
                    labels.push(json!({"path":path,"label":name,"base":item["base_model"].as_str().unwrap_or("")}));
                }
                if r["total_pages"].as_u64().is_none_or(|t| t <= page) {
                    break;
                }
            }
            return Ok(json!({"labels":labels}));
        }
        Request::CharacterCards => {
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return Ok(json!(db.character_cards()?));
        }
        Request::NodeCatalog => {
            // Cache TTL (60 s): the catalog derives only from /object_info; the
            // refetch+transform cost 0.7 s per view mount (measured on the real
            // 15.9 MB capture).
            const NODE_CATALOG_TTL: std::time::Duration = std::time::Duration::from_secs(60);
            {
                let cached = s
                    .node_catalog
                    .lock()
                    .map_err(|_| "Node catalog lock unavailable")?
                    .clone();
                if let Some((at, catalog)) = cached {
                    if at.elapsed() < NODE_CATALOG_TTL {
                        return Ok(catalog);
                    }
                }
            }
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let mut nodes = Vec::new();
            for (class, schema) in info.as_object().into_iter().flatten() {
                let required = schema.pointer("/input/required").and_then(Value::as_object);
                let optional = schema.pointer("/input/optional").and_then(Value::as_object);
                let input = |map: Option<&serde_json::Map<String, Value>>| {
                    map.into_iter()
                        .flatten()
                        .map(|(name, decl)| {
                            // ComfyUI V3 combos declare ["COMBO", {"options": [...]}]; expose the real choices like V1 lists.
                            let config = decl.get(1).cloned().unwrap_or(Value::Null);
                            let kind = match (decl.get(0), config.get("options")) {
                                (Some(Value::String(k)), Some(options))
                                    if k == "COMBO" && options.is_array() =>
                                {
                                    options.clone()
                                }
                                (kind, _) => kind.cloned().unwrap_or(Value::Null),
                            };
                            json!({"name":name,"type":kind,"config":config})
                        })
                        .collect::<Vec<_>>()
                };
                let inputs: Vec<Value> =
                    input(required).into_iter().chain(input(optional)).collect();
                let outputs = schema.get("output").and_then(Value::as_array).into_iter().flatten().enumerate().map(|(i,t)| json!({"name":schema.get("output_name").and_then(Value::as_array).and_then(|n|n.get(i)).and_then(Value::as_str).unwrap_or("sortie"),"type":t})).collect::<Vec<_>>();
                nodes.push(json!({"name":class,"displayName":schema.get("display_name").and_then(Value::as_str).unwrap_or(class),"category":schema.get("category").and_then(Value::as_str).unwrap_or("Divers"),"description":schema.get("description").and_then(Value::as_str).unwrap_or(""),"inputs":inputs,"outputs":outputs,"outputNode":schema.get("output_node").and_then(Value::as_bool).unwrap_or(false)}));
            }
            let catalog = json!({"nodes":nodes});
            if let Ok(mut cache) = s.node_catalog.lock() {
                *cache = Some((std::time::Instant::now(), catalog.clone()));
            }
            return Ok(catalog);
        }
        Request::GraphNew => {
            let info = Comfy::loopback(s.comfy_port)?
                .get("/object_info")
                .unwrap_or_else(|_| json!({}));
            let text = json!({"version":0.4,"nodes":[],"links":[],"groups":[],"definitions":{"subgraphs":[]}}).to_string();
            let session = GraphSession::open_file(&text, &info, "Nouveau workflow UI")?;
            let view = session.view();
            let snapshot = session.snapshot()?;
            let id = studio_core::graph_editor::new_id();
            {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                if editors.len() >= 8 {
                    return Err("Huit éditeurs ouverts au maximum".into());
                }
                editors.insert(id.clone(), session);
            }
            {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &id, &snapshot)?;
            }
            return Ok(json!({"session":id,"view":view}));
        }
        Request::GraphOpenFile { path } => {
            use std::io::Read;
            let path = std::path::Path::new(&path).canonicalize()?;
            if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
                return Err("Sélectionner un fichier JSON ComfyUI".into());
            }
            let mut text = String::new();
            std::fs::File::open(&path)?
                .take(16_000_001)
                .read_to_string(&mut text)?;
            let info = Comfy::loopback(s.comfy_port)?
                .get("/object_info")
                .unwrap_or_else(|_| json!({}));
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Workflow");
            let session = GraphSession::open_file(&text, &info, name)?;
            let view = session.view();
            let snapshot = session.snapshot()?;
            let id = studio_core::graph_editor::new_id();
            {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                if editors.len() >= 8 {
                    return Err("Huit éditeurs ouverts au maximum".into());
                }
                editors.insert(id.clone(), session);
            }
            {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &id, &snapshot)?;
            }
            return Ok(json!({"session":id,"view":view}));
        }
        Request::GenerationPreset {
            session,
            source_path,
        } => {
            let info = Comfy::loopback(s.comfy_port)?
                .get("/object_info")
                .unwrap_or_else(|_| json!({}));
            let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
            let editor = editors.get_mut(&session).ok_or("Session inconnue")?;
            let source = editor.export_ui()?.to_string();
            let expanded =
                studio_core::saya::generation_preset_source(&source, source_path.as_deref())?;
            if expanded != source {
                let replacement =
                    GraphSession::open_file(&expanded, &info, "Preset Saya · 6 passes")?;
                let snapshot = replacement.snapshot()?;
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &session, &snapshot)?;
                *editor = replacement;
            }
            return Ok(editor.view());
        }
        Request::GraphSaveUi { session, name } => {
            use std::io::Write;
            if name.trim().is_empty() || name.len() > 300 {
                return Err("Nom invalide".into());
            }
            let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
            let graph = editors
                .get(&session)
                .ok_or("Session inconnue")?
                .export_ui()?;
            let folder = s.directory.join("exports");
            std::fs::create_dir_all(&folder)?;
            let slug: String = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(80)
                .collect();
            let path = folder.join(format!(
                "{}-{}.json",
                slug,
                studio_core::graph_editor::new_id()
            ));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(serde_json::to_string_pretty(&graph)?.as_bytes())?;
            file.sync_all()?;
            return Ok(json!({"path":path}));
        }
        Request::GraphOpen { workflow } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let (name, text): (String, String) = {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                db.workflow_document(&workflow)?
            };
            let session =
                GraphSession::open(&workflow, &serde_json::from_str(&text)?, &info, &name)?;
            let view = session.view();
            let snapshot = session.snapshot()?;
            let id = studio_core::graph_editor::new_id();
            {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                if editors.len() >= 8 {
                    return Err("Huit éditeurs ouverts au maximum : fermez-en un".into());
                }
                editors.insert(id.clone(), session);
            }
            {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &id, &snapshot)?;
            }
            return Ok(json!({"session": id, "view": view}));
        }
        Request::GraphEdit { session, op } => {
            let (view, snapshot) = {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors
                    .get_mut(&session)
                    .ok_or("Session d'édition inconnue")?;
                editor.apply(op)?;
                let snapshot = if editor.is_scoped() {
                    None
                } else {
                    Some(editor.snapshot()?)
                };
                (editor.view(), snapshot)
            };
            if let Some(snapshot) = snapshot {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &session, &snapshot)?;
            }
            return Ok(view);
        }
        Request::GraphView { session } => {
            let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
            return Ok(editors
                .get(&session)
                .ok_or("Session d'édition inconnue")?
                .view());
        }
        Request::GraphEnterSubgraph { session, container } => {
            let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
            return editors
                .get_mut(&session)
                .ok_or("Session d'édition inconnue")?
                .enter_subgraph(&container);
        }
        Request::GraphExitSubgraph { session, commit } => {
            let (view, snapshot) = {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors
                    .get_mut(&session)
                    .ok_or("Session d'édition inconnue")?;
                let view = editor.exit_subgraph(commit)?;
                let snapshot = if commit && !editor.is_scoped() {
                    Some(editor.snapshot()?)
                } else {
                    None
                };
                (view, snapshot)
            };
            if let Some(snapshot) = snapshot {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &session, &snapshot)?;
            }
            return Ok(view);
        }
        Request::GraphCopy { session, nodes } => {
            let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
            return editors
                .get(&session)
                .ok_or("Session d'édition inconnue")?
                .copy_fragment(&nodes);
        }
        Request::GraphScopedWidget {
            session,
            path,
            edit,
        } => {
            let changed = edit.is_some();
            let (view, root, snapshot) = {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors.get_mut(&session).ok_or("Session inconnue")?;
                let view = editor.scoped_widget(&path, edit)?;
                (
                    view,
                    if changed { editor.view() } else { Value::Null },
                    if changed {
                        Some(editor.snapshot()?)
                    } else {
                        None
                    },
                )
            };
            if let Some(snapshot) = snapshot {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &session, &snapshot)?;
            }
            return Ok(json!({"view":view,"root":root}));
        }
        Request::GraphSessions => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.editor_sessions()?));
        }
        Request::GraphDeleteSession { id } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            db.delete_editor_session(&id)?;
            return Ok(Value::Null);
        }
        Request::GraphResume { id } => {
            {
                let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                if let Some(editor) = editors.get(&id) {
                    return Ok(json!({"session": id, "view": editor.view()}));
                }
            }
            let record = {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                db.editor_session(&id)?
            };
            let info = Comfy::loopback(s.comfy_port)?
                .get("/object_info")
                .unwrap_or_else(|_| json!({}));
            let session = GraphSession::restore(
                record["workflow"].as_str().unwrap_or(""),
                record["document"]
                    .as_str()
                    .ok_or("Document de session absent")?,
                record["uiSource"].as_str(),
                record["originalApi"].as_str(),
                record["originalIds"].as_str().unwrap_or("{}"),
                info,
            )?;
            let view = session.view();
            {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                if editors.len() >= 8 {
                    return Err("Huit éditeurs ouverts au maximum : fermez-en un".into());
                }
                editors.insert(id.clone(), session);
            }
            return Ok(json!({"session": id, "view": view}));
        }
        Request::GraphPaste {
            session,
            fragment,
            offset,
        } => {
            let (result, snapshot) = {
                let mut editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors
                    .get_mut(&session)
                    .ok_or("Session d'édition inconnue")?;
                let result = editor.paste_fragment(&fragment, offset)?;
                let snapshot = if editor.is_scoped() {
                    None
                } else {
                    Some(editor.snapshot()?)
                };
                (result, snapshot)
            };
            if let Some(snapshot) = snapshot {
                let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
                persist_editor_session(&db, &session, &snapshot)?;
            }
            return Ok(result);
        }
        Request::GraphPrepareApi { session } => {
            let graph = {
                let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors.get(&session).ok_or("Session d'édition inconnue")?;
                if editor.view()["sourceFormat"] != json!("comfy_ui") {
                    if session.starts_with("temporary-") {
                        let api = editor.export()?;
                        return Ok(
                            json!({"api":api,"report":{"complete":true},"previewJson":serde_json::to_string_pretty(&api)?}),
                        );
                    }
                    return Err("Préparation API réservée aux workflows UI ComfyUI".into());
                }
                editor.export_ui()?.to_string()
            };
            // Without an engine, preview remains possible but the report states the
            // schema is unchecked; creation is blocked.
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info").ok();
            let source: Value = serde_json::from_str(&graph)?;
            let mut prepared = if source["extra"].get("saya_preset").is_some() {
                let info = info
                    .as_ref()
                    .ok_or("Connecter ComfyUI pour vérifier les six passes du preset")?;
                let phases = (1..=6)
                    .map(|phase| {
                        studio_core::saya::prepare_phase_api(&graph, info, phase, None, None)
                    })
                    .collect::<Result<Vec<_>>>()?;
                json!({"kind":"saya_preset","phases":phases,"report":{"complete":true}})
            } else {
                studio_core::workflow_ui_api::prepare_api(&graph, info.as_ref())?
            };
            prepared["previewJson"] =
                json!(serde_json::to_string_pretty(&prepared).map_err(|e| e.to_string())?);
            return Ok(prepared);
        }
        Request::GraphSaveApi {
            session,
            name,
            preview_hash,
            confirmed,
        } => {
            let graph = {
                let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors.get(&session).ok_or("Session d'édition inconnue")?;
                if editor.view()["sourceFormat"] != json!("comfy_ui") {
                    return Err("Préparation API réservée aux workflows UI ComfyUI".into());
                }
                editor.export_ui()?.to_string()
            };
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info").map_err(|_| {
                "Moteur ComfyUI injoignable : validation object_info requise pour créer la révision API"
            })?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return studio_core::workflow_ui_api::save_api_revision(
                &db,
                &graph,
                Some(&info),
                &name,
                &preview_hash,
                confirmed,
            );
        }
        Request::GraphExport { session, name } => {
            let graph = {
                let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                editors
                    .get(&session)
                    .ok_or("Session d'édition inconnue")?
                    .export()?
            };
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            let id = db.import_workflow(&name, &graph.to_string())?;
            return Ok(
                json!({"workflowId": id, "nodeCount": graph.as_object().map_or(0, |o| o.len())}),
            );
        }
        Request::GraphExtractModule {
            session,
            nodes,
            module,
            name,
            description,
        } => {
            let (draft, fingerprint, source_workflow) = {
                let editors = s.editors.lock().map_err(|_| "Editor lock unavailable")?;
                let editor = editors.get(&session).ok_or("Session d'édition inconnue")?;
                let draft = editor.extract_module_draft(&nodes)?;
                let fingerprint = editor.extract_module_fingerprint()?;
                let source = editor.workflow.clone();
                (draft, fingerprint, source)
            };
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let (id, version) = db.save_workflow_module(
                module.as_deref(),
                &name,
                &description,
                if source_workflow.is_empty() {
                    None
                } else {
                    Some(source_workflow.as_str())
                },
                &fingerprint,
                &draft,
            )?;
            return Ok(json!({
                "id": id,
                "version": version,
                "inputs": draft.inputs,
                "outputs": draft.outputs,
                "parameters": draft.parameters,
                "nodeCount": draft.document.as_object().map_or(0, Map::len),
            }));
        }
        Request::GraphClose { session } => {
            s.editors
                .lock()
                .map_err(|_| "Editor lock unavailable")?
                .remove(&session);
            return Ok(Value::Null);
        }
        Request::PrepareAbCells { campaign, cells } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.prepare_ab_cells(&campaign, &cells, &info);
        }
        Request::ModuleInsertPreview {
            id,
            version,
            workflow,
            instance,
            connection,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.module_insert_preview(
                &id,
                version,
                &workflow,
                &instance,
                &connection,
                &info,
            );
        }
        Request::ConnectModule {
            id,
            version,
            instance,
            pipeline,
            connection,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.connect_module(&id, version, &instance, &pipeline, &connection, &info);
        }
        Request::PrepareCouple { id, input } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let edits = db.couple_edits(&id, &input)?;
            return Ok(json!(db.prepare_job_with_characters(
                &id,
                &edits,
                &info,
                &input.characters
            )?));
        }
        Request::Prepare {
            id,
            edits,
            characters,
            origin,
        } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            // The origin is validated before preparing, so a bad source never
            // leaves an untraced draft behind.
            if let Some(origin) = &origin {
                db.check_job_origin(origin)?;
            }
            let prepared = db.prepare_job_with_characters(&id, &edits, &info, &characters)?;
            if let Some(origin) = &origin {
                db.record_job_origin(&prepared.job_id, origin)?;
            }
            return Ok(json!(prepared));
        }
        Request::Submit {
            job,
            hash,
            confirmed,
            allow_removals,
        } => {
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.submit_job(
                &job,
                &hash,
                confirmed,
                allow_removals,
                &Comfy::loopback(s.comfy_port)?,
            );
        }
        Request::ControlJob {
            id,
            operation,
            confirmed,
        } => {
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let result =
                db.control_job(&id, &operation, confirmed, &Comfy::loopback(s.comfy_port)?);
            if let Some(monitor) = s.monitor.lock().map_err(|_| "Monitor lock")?.as_ref() {
                monitor.refresh();
            }
            return result;
        }
        Request::SyncJobs => {
            if let Some(monitor) = s.monitor.lock().map_err(|_| "Monitor lock")?.as_ref() {
                monitor.refresh();
                return Ok(json!(s.db.lock().map_err(|_| "Database lock")?.jobs()?));
            }
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            db.sync_jobs(
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            )?;
            return Ok(json!(db.jobs()?));
        }
        Request::Connection => return Ok(Comfy::loopback(s.comfy_port)?.status()),
        Request::Models => return Comfy::loopback(s.comfy_port)?.models(),
        Request::Image { id, preview } => {
            let start = std::time::Instant::now();
            let path =
                s.db.lock()
                    .map_err(|_| "Database lock unavailable")?
                    .asset_path(&id)?;
            let resolved = start.elapsed();
            let data = image_data(&path, &s.directory.join("thumbnails"), preview);
            if std::env::var("SAYA_STUDIO_TRACE").as_deref() == Ok("1") {
                eprintln!(
                    "[perf:image] id={id} preview={preview} resolve_ms={:.1} image_data_ms={:.1} bytes={}",
                    resolved.as_secs_f64() * 1000.0,
                    start.elapsed().as_secs_f64() * 1000.0,
                    data.as_ref().map(|v| v.len()).unwrap_or(0)
                );
            }
            return Ok(json!(data?));
        }
        Request::EditorLoad { id } => {
            let path =
                s.db.lock()
                    .map_err(|_| "Database lock unavailable")?
                    .asset_path(&id)?;
            return studio_core::editor::load_original(&path).map(|value| json!(value));
        }
        Request::EditorExport {
            id,
            destination,
            format,
            image,
        } => {
            let path =
                s.db.lock()
                    .map_err(|_| "Database lock unavailable")?
                    .asset_path(&id)?;
            let saved = studio_core::editor::save_export(
                &path,
                std::path::Path::new(&destination),
                &format,
                &image,
            )?;
            return Ok(json!({"destination":saved}));
        }
        Request::BlurDocument { asset } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.blur_document(&asset);
        }
        Request::BlurSave { asset, document } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.save_blur_document(&asset, &document);
        }
        Request::PrepareUniversal { id } => {
            let info = Comfy::loopback(s.comfy_port)?.get("/object_info")?;
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return db.prepare_universal(&id, &info);
        }
        Request::LoraInventory { directory } => {
            return Ok(json!(studio_core::lora_inventory::inventory(&directory)?));
        }
        Request::LoraLoaders { workflow } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.lora_loaders(&workflow)?));
        }
        Request::LoraCover { path } => {
            return Ok(json!(studio_core::lora_inventory::cover(&path)?));
        }
        Request::LoraMetadataSave { path, metadata } => {
            return Ok(json!(studio_core::lora_inventory::save_metadata(
                &path, &metadata
            )?));
        }
        Request::PreviewLora { workflow, config } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.preview_lora(&workflow, &config)?));
        }
        Request::SamplerSteps { job } => {
            let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
            return Ok(json!(db.sampler_steps(&job)?));
        }
        Request::SayaReviewDecision {
            job,
            decision,
            confirmed,
        } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.saya_review_decision(
                &job,
                &decision,
                &Comfy::loopback(s.comfy_port)?,
                confirmed,
            );
        }
        Request::SayaStart {
            source_path,
            master_seed,
            isolated_root,
        } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.saya_start(
                &source_path,
                master_seed.as_deref(),
                &isolated_root,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            );
        }
        Request::SayaRuns => {
            return Ok(json!(s
                .db
                .lock()
                .map_err(|_| "Database lock unavailable")?
                .saya_runs()?));
        }
        Request::LastQueuedSeed { workflow } => {
            return s
                .db
                .lock()
                .map_err(|_| "Database lock unavailable")?
                .last_queued_seed(&workflow);
        }
        Request::SayaStatus { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.saya_status(
                &run,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            );
        }
        Request::SayaContinue { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.saya_continue(
                &run,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            );
        }
        Request::SayaRestart { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.saya_restart(
                &run,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            );
        }
        Request::SayaStop { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.saya_stop(
                &run,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            );
        }
        Request::CastingPublish { runs } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            return Database::open(&s.directory.join("studio.sqlite3"))?.casting_publish(&runs);
        }
        Request::CastingDeleteLot { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            return Database::open(&s.directory.join("studio.sqlite3"))?.casting_delete_batch(&run);
        }
        Request::GalleryCleanup => {
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return db.cleanup_exact_duplicates();
        }
        Request::CastingCapabilities => {
            return studio_core::engine_runtime::casting_capabilities(
                &studio_core::engine_runtime::engine_dir(),
            )
        }
        Request::CastingHistory { sync } => {
            let _ = sync; // Compatibility: the background monitor owns synchronization.
                          // UI snapshots never wait for orchestration or perform network work.
                          // The background monitor owns synchronization and automatic advances.
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let mut rows = db.casting_history()?;
            // HONEST progress: only what the app ALREADY knows - the state of each pass
            // (run manifest, `phases` field), the current phase (first unsuccessful
            // pass), and the REAL sampler progress from the WS monitor (live.rs LiveJob:
            // current node, value/max reported by ComfyUI, joined by comfy_prompt_id).
            // No computed percentage; nothing to join = null.
            let mut jobs: Vec<String> = Vec::new();
            for row in &rows {
                for phase in 1..=6 {
                    if let Some(job) = row["phases"][phase.to_string()]["job"].as_str() {
                        if !jobs.iter().any(|j| j == job) {
                            jobs.push(job.to_owned());
                        }
                    }
                }
            }
            let prompts = db.prompt_ids_for(&jobs)?;
            let mut live: std::collections::BTreeMap<String, Value> =
                std::collections::BTreeMap::new();
            if let Some(monitor) = s.monitor.lock().map_err(|_| "Monitor lock")?.as_ref() {
                if let Ok(snapshot_state) = monitor.snapshot() {
                    for (prompt, job) in snapshot_state.jobs {
                        live.insert(
                            prompt,
                            json!({"node": job.node, "value": job.value, "max": job.max}),
                        );
                    }
                }
            }
            for row in &mut rows {
                let mut current: Option<u32> = None;
                for phase in 1..=6u32 {
                    if row["phases"][phase.to_string()]["state"].as_str() == Some("succeeded") {
                        continue;
                    }
                    current = Some(phase);
                    break;
                }
                row["phaseCourante"] = match current {
                    Some(phase) => json!(phase),
                    None => Value::Null,
                };
                row["progression"] = Value::Null;
                row["promptId"] = Value::Null;
                if let Some(phase) = current {
                    if let Some(job) = row["phases"][phase.to_string()]["job"].as_str() {
                        if let Some(prompt) = prompts.get(job) {
                            row["promptId"] = json!(prompt);
                            if let Some(progression) = live.get(prompt) {
                                row["progression"] =
                                    json!({"phase": phase, "sampler": progression});
                            }
                        }
                    }
                }
            }
            return Ok(json!(rows));
        }
        Request::CastingDecide { run, keep } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            db.casting_decide(
                &run,
                keep,
                &Comfy::loopback(s.comfy_port)?,
                &s.directory.join("results"),
            )?;
            return Ok(json!(db.casting_history()?));
        }
        Request::CastingStop { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let client = Comfy::loopback(s.comfy_port)?;
            let results = s.directory.join("results");
            let runs = db.casting_batch_runs(&run)?;
            // Persist the whole local stop before any network request can fail.
            for id in &runs {
                db.casting_stop_offline(id)?;
            }
            let online = client.get("/queue").is_ok();
            if online {
                // The local stop is already persisted; a failed sync only delays reconciliation.
                if let Err(e) = db.sync_jobs(&client, &results) {
                    eprintln!("Casting stop: job sync failed: {e}");
                }
            }
            let mut controls = Vec::new();
            for id in runs {
                controls.push(if online {
                    db.casting_stop_synced(&id, &client)?
                } else {
                    db.casting_stop_offline(&id)?
                });
            }
            return Ok(json!({"runs":controls,"online":online}));
        }
        Request::CastingPause { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let mut controls = Vec::new();
            for id in db.casting_batch_runs(&run)? {
                controls.push(db.casting_pause(&id)?);
            }
            return Ok(json!({"runs":controls}));
        }
        Request::CastingNext { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let released = db.casting_next(&run)?;
            return Ok(json!({"run": released, "rows": db.casting_history()?}));
        }
        Request::CastingResume { run } => {
            let _operation = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            return Ok(db.casting_resume_batch(&run)?);
        }
        Request::EngineScript {
            script,
            id_perso,
            timeout_secs,
        } => {
            // Real script of the fish generez menu: run without a shell, log returned
            // as is. A failing or timed-out script is a reported failure.
            let timeout = std::time::Duration::from_secs(
                timeout_secs
                    .unwrap_or(studio_core::engine_runtime::SCRIPT_TIMEOUT_SECS)
                    .clamp(10, 3600),
            );
            let outcome = studio_core::engine_runtime::run_engine_script(
                &studio_core::engine_runtime::engine_dir(),
                &script,
                id_perso.as_deref(),
                timeout,
            )?;
            return Ok(json!({
                "script": outcome.script,
                "argv": outcome.argv,
                "cwd": outcome.cwd.display().to_string(),
                "success": outcome.success,
                "code": outcome.code,
                "timeout": outcome.timeout,
                "stdout": outcome.stdout,
                "stderr": outcome.stderr,
            }));
        }
        Request::ActionDrafts => return action_drafts_request(s, request),
        Request::ActionDraftSave { .. } => return action_drafts_request(s, request),
        Request::ActionDraftDelete { .. } => return action_drafts_request(s, request),
        Request::ActionDraftPromote { .. } => return action_drafts_request(s, request),
        Request::ActionDraftsRefresh => return action_drafts_request(s, request),
        Request::CastingLancer {
            graine,
            nombre,
            famille,
            perso,
            pose_fixe,
            persos,
            actions,
            exclude,
            genre,
            subfamily,
            source_path,
            step,
            drafts,
            all_actions,
        } => {
            // One casting at a time (the engine always rewrites output/casting/) and
            // one Saya operation in flight: same lock as the 6-pass runs.
            // U2 rotations: perso -> Mode A (--poses <ID> --casting N), pose_fixe ->
            // Mode B (--casting N --pose-fixe <action>). V1 targeting: persos ->
            // (--casting N --persos ID1,ID2,...). V2 targeting: actions ->
            // (--casting N --actions a,b,c), otherwise casting U1.
            let _operation = s
                .casting_operations
                .try_lock()
                .map_err(|_| "Une préparation de casting est déjà en cours")?;
            let progress = CastingPreparationGuard::start(&s.casting_preparation, nombre as usize)?;
            if !std::path::Path::new(&source_path).is_absolute() {
                return Err("Chemin de workflow Saya absolu requis".into());
            }
            let seed_u64: u64 = graine
                .trim()
                .parse()
                .map_err(|_| format!("Graine du casting invalide : {graine}"))?;
            let mode = perso
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| ("perso", p.to_owned()));
            let mode = match mode {
                Some(mode) => Some(mode),
                None => pose_fixe
                    .as_deref()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(|p| ("pose_fixe", p.to_owned())),
            };
            // V1 targeting: ids are normalized (trimmed, non-empty, deduplicated keeping
            // order); the count follows the selection (the engine rejects a --casting
            // that diverges from --persos).
            let targets: Vec<String> = persos
                .unwrap_or_default()
                .into_iter()
                .map(|id| id.trim().to_owned())
                .filter(|id| !id.is_empty())
                .collect();
            let mut deduped = Vec::with_capacity(targets.len());
            for id in &targets {
                if !deduped.contains(id) {
                    deduped.push(id.clone());
                }
            }
            // V2 targeting: same normalization on targeted actions. The two targetings
            // are mutually exclusive (the engine would ignore one of them): clean refusal
            // BEFORE any draw, never an ambiguous casting.
            let targeted_actions: Vec<String> = actions
                .unwrap_or_default()
                .into_iter()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .collect();
            let mut seen_actions = Vec::with_capacity(targeted_actions.len());
            for name in &targeted_actions {
                if !seen_actions.contains(name) {
                    seen_actions.push(name.clone());
                }
            }
            if !deduped.is_empty() && !seen_actions.is_empty() {
                return Err(
                    "Ciblages exclusifs : des persos ciblés OU des actions ciblées, pas les deux"
                        .into(),
                );
            }
            if all_actions
                && (!matches!(mode, Some(("perso", _)))
                    || drafts
                    || !deduped.is_empty()
                    || pose_fixe.as_deref().is_some_and(|s| !s.trim().is_empty()))
            {
                return Err("Toutes les actions : choisis un perso, sans pose fixe ni persos ciblés".into());
            }
            if (perso.as_deref().is_some_and(|s| !s.trim().is_empty())
                && pose_fixe.as_deref().is_some_and(|s| !s.trim().is_empty()))
                || (mode.is_some() && !all_actions && (!deduped.is_empty() || !seen_actions.is_empty()))
            {
                return Err("Choisir un seul mode de casting : ciblage ou rotation".into());
            }
            // Action exclusion: reaches the REAL engine draw (--exclude a,b,c, engine
            // slice W1 - A/B convergence). ENGINE semantics: the exclusion wins over
            // --actions and combines with --pose-fixe; it is refused with --persos
            // targeting and --poses rotations.
            let exclusions: Vec<String> = exclude
                .unwrap_or_default()
                .into_iter()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .collect();
            if !exclusions.is_empty()
                && (perso.as_deref().is_some_and(|p| !p.trim().is_empty())
                    || !deduped.is_empty())
            {
                return Err(
                    "L'exclusion d'actions ne s'applique pas aux rotations de perso ni au ciblage de persos : tirage libre, pose fixe ou actions ciblées uniquement"
                        .into(),
                );
            }
            let mut db = Database::open(&s.directory.join("studio.sqlite3"))?;
            let selected_subfamily = subfamily.as_deref().filter(|v| !v.is_empty());
            if selected_subfamily.is_some() && (!deduped.is_empty() || perso.is_some()) {
                return Err(
                    "La sous-famille des personnages favoris reste celle de leur fiche".into(),
                );
            }
            if drafts && (selected_subfamily.is_some() || mode.is_some() || !deduped.is_empty()) {
                return Err("Test de brouillons : ni sous-famille, ni rotation, ni persos ciblés".into());
            }
            let outcome = if let Some(subfamily) = selected_subfamily {
                studio_core::engine_runtime::casting_subfamily(
                    seed_u64,
                    nombre,
                    famille.as_deref().ok_or("Famille requise")?,
                    subfamily,
                    &seen_actions,
                    &exclusions,
                    pose_fixe.as_deref(),
                )?
            } else {
                match mode.as_ref() {
                    Some(("perso", id)) if all_actions => {
                        studio_core::engine_runtime::casting_character_actions(seed_u64, id, &seen_actions)?
                    }
                    Some(("perso", id)) => {
                        studio_core::engine_runtime::casting_character(seed_u64, id, nombre)?
                    }
                    Some(("pose_fixe", action)) => studio_core::engine_runtime::casting_pose(
                        seed_u64,
                        nombre,
                        famille.as_deref(),
                        action,
                        &exclusions,
                    )?,
                    _ if !deduped.is_empty() => {
                        studio_core::engine_runtime::casting_characters(seed_u64, &deduped)?
                    }
                    _ if drafts => {
                        // Draft test: only pending drafts, drawn from the engine test copy.
                        let root = s.directory.join("action-drafts");
                        let pending = studio_core::action_drafts::pending_ids(&root)?;
                        if seen_actions.is_empty() {
                            return Err("Coche au moins un brouillon d'action à tester".into());
                        }
                        if let Some(other) = seen_actions.iter().find(|a| !pending.contains(a)) {
                            return Err(format!("« {other} » n'est pas un brouillon en attente").into());
                        }
                        studio_core::actions_bank::casting_actions_in_with_timeout(
                            &studio_core::action_drafts::sandbox_engine(&root),
                            seed_u64,
                            nombre,
                            famille.as_deref(),
                            &seen_actions,
                            &exclusions,
                            std::time::Duration::from_secs(
                                studio_core::engine_runtime::DRY_RUN_TIMEOUT_SECS,
                            ),
                        )?
                    }
                    _ if !seen_actions.is_empty() => studio_core::actions_bank::casting_actions(
                        seed_u64,
                        nombre,
                        famille.as_deref(),
                        &seen_actions,
                        &exclusions,
                    )?,
                    _ if !exclusions.is_empty() => studio_core::engine_runtime::casting_exclude(
                        seed_u64,
                        nombre,
                        famille.as_deref(),
                        &exclusions,
                    )?,
                    _ => studio_core::engine_runtime::casting(
                        seed_u64,
                        nombre,
                        famille.as_deref(),
                    )?,
                }
            };
            let mut shots = studio_core::casting::read_catalogue(&outcome.catalogue)?;
            if let Some(subfamily) = selected_subfamily {
                for shot in &mut shots {
                    shot["subfamily"] = json!(subfamily);
                }
            }
            progress.update("characters", 0, shots.len())?;
            if let Some(mode) = genre.as_deref().filter(|s| !s.is_empty()) {
                if !deduped.is_empty() || perso.is_some() {
                    return Err("Le genre des personnages favoris reste celui de leur fiche".into());
                }
                shots = studio_core::engine_runtime::casting_mode(
                    &studio_core::engine_runtime::engine_dir(),
                    mode,
                    &shots,
                )?;
                if shots.is_empty() {
                    return Err("Aucun personnage compatible avec ce mode".into());
                }
                for shot in &shots {
                    studio_core::casting::validate_shot(shot)?;
                }
            }
            let source = std::fs::read_to_string(&source_path)?;
            let client = Comfy::loopback(s.comfy_port)?;
            progress.update("catalogue", 0, shots.len())?;
            let info = client.get("/object_info")?;
            let results = s.directory.join("results");
            let _saya = s
                .saya_operations
                .lock()
                .map_err(|_| "Saya operation lock")?;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = s.directory.join("casting").join(format!("u1-{timestamp}"));
            std::fs::create_dir_all(&dir)?;
            let catalogue = dir.join("catalogue.json");
            std::fs::write(&catalogue, serde_json::to_vec_pretty(&shots)?)?;
            let mut launched = Vec::new();
            progress.update("prepare", 0, shots.len())?;
            for (index, shot) in shots.iter().enumerate() {
                // The original source is never written: each OC gets ITS OWN filled copy.
                let mut filled: Value =
                    serde_json::from_str(&studio_core::casting::fill_source(&source, shot)?)?;
                filled["extra"]["saya_casting"] = json!({"graine":seed_u64.to_string(),"catalogue":catalogue,"shot":shot,"decision":"","deferred":true,"index":index});
                let filled_text = filled.to_string();
                let file_path = dir.join(format!("passe1-{:04}.json", index + 1));
                std::fs::write(&file_path, &filled_text)?;
                let seed = studio_core::casting::shot_seed(shot)?;
                let run = db.saya_start_source_with_info(
                    &filled_text,
                    &file_path.to_string_lossy(),
                    Some(&seed),
                    &format!("casting-tests/{timestamp}/{}", index + 1),
                    &client,
                    &results,
                    &info,
                )?;
                progress.update("prepare", index + 1, shots.len())?;
                launched.push(json!({
                    "stem": shot["stem"], "name": shot["name"], "famille": shot["famille"],
                    "model": shot["model"], "seed": seed, "run": run["id"],
                    "sourcePath": file_path, "shot": shot,
                }));
            }
            let ids: Vec<String> = launched
                .iter()
                .filter_map(|r| r["run"].as_str().map(str::to_owned))
                .collect();
            progress.update("start", shots.len(), shots.len())?;
            if step {
                db.casting_batch_step(&ids)?;
            }
            db.casting_batch_prepared(&ids)?;
            // The background loop submits the first trial; return controls immediately.
            return Ok(json!({
                "graine": seed_u64.to_string(), "famille": famille,
                "perso": perso, "pose_fixe": pose_fixe, "persos": deduped,
                "actions": seen_actions, "exclude": exclusions,
                "catalogue": catalogue,
                "moteurStdout": outcome.stdout, "moteurStderr": outcome.stderr,
                "shots": launched,
            }));
        }
        // Generations browser (V5): read-only listing of image folders (engine,
        // ComfyUI copies, app results). No database, no writes: the list comes from
        // the filesystem.
        Request::ComfyOutputInfo => {
            studio_core::generation_browser::refresh_running_argv(s.comfy_port);
            let found = studio_core::generation_browser::comfy_output_resolution();
            return Ok(json!({
                "path": found.path.map(|p| p.display().to_string()),
                "source": found.source.as_str(),
            }));
        }
        Request::GenerationBrowser => {
            let browser_roots = browser_roots(s);
            return Ok(studio_core::generation_browser::list_roots(&browser_roots)?);
        }
        Request::GenerationBrowserImage { path, preview } => {
            let browser_roots = browser_roots(s);
            return Ok(json!(studio_core::generation_browser::read_image(
                std::path::Path::new(&path),
                &browser_roots,
                &s.directory.join("thumbnails"),
                preview,
            )?));
        }
        // Long file IO / child processes run without holding `s.db` (UI requests wait on it).
        Request::GrokPreview { paths } => return side_database(s)?.grok_preview(&paths),
        Request::TrashAssets { plan, confirmed } => {
            return side_database(s)?.trash_assets(&plan, confirmed);
        }
        Request::OpenAssetDirectory { id } => {
            let path =
                s.db.lock()
                    .map_err(|_| "Database lock unavailable")?
                    .asset_parent_directory(&id)?;
            open_detached(&path)?;
            return Ok(Value::Null);
        }
        Request::OpenDirectory { path } => {
            let path =
                studio_core::library_sources::checked_directory(std::path::Path::new(&path))?;
            open_detached(&path)?;
            return Ok(Value::Null);
        }
        _ => {}
    }
    let mut db = s.db.lock().map_err(|_| "Database lock unavailable")?;
    Ok(match request {
        Request::Gallery { filters } => serde_json::to_value(db.gallery(&filters)?)?,
        Request::GalleryWindow { filters, limit } => {
            serde_json::to_value(db.gallery_window(&filters, limit)?)?
        }
        Request::CharacterSheet { id } => db.character_sheet(&id)?,
        Request::Facets => db.facets()?,
        Request::GalleryNeighbors {
            filters,
            id,
            fallback,
        } => db.gallery_neighbors(&filters, &id, fallback)?,
        Request::GrokScenePreview { paths } => db.grok_scene_preview(&paths)?,
        Request::GrokSceneImport { choices, confirmed } => {
            json!(db.grok_scene_import(&choices, confirmed)?)
        }
        Request::GrokScenes => db.grok_scenes()?,
        Request::GrokScene { id } => db.grok_scene(&id)?,
        Request::GrokDirectoryFiles { paths } => {
            let mut files = Vec::new();
            for path in paths {
                let root =
                    studio_core::library_sources::checked_directory(std::path::Path::new(&path))?;
                for entry in std::fs::read_dir(root)? {
                    let path = entry?.path();
                    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
            files.sort();
            files.dedup();
            if files.len() > 64 {
                return Err("Plus de 64 JSON : choisissez les fichiers explicitement".into());
            }
            json!(files)
        }
        Request::GrokImport { choices, confirmed } => db.grok_import(&choices, confirmed)?,
        Request::PortraitCandidates { character, offset } => {
            db.portrait_candidates(&character, offset)?
        }
        Request::ChoosePortrait { character, asset } => {
            db.choose_portrait(&character, &asset)?;
            Value::Null
        }
        Request::GenerationMetadata { id } => db.generation_metadata(&id)?,
        Request::SetCatalogRemoved { ids, removed } => {
            db.set_catalog_removed(&ids, removed)?;
            Value::Null
        }
        Request::RefreshAssetPresence { id } => db.refresh_asset_presence(&id)?,
        Request::PrepareAssetTrash { ids } => json!(db.prepare_asset_trash(&ids)?),
        Request::LibrarySources => json!(db.library_sources()?),
        Request::AddLibrarySource { path } => {
            json!(db.add_library_source(std::path::Path::new(&path))?)
        }
        Request::SetLibrarySource { id, enabled } => {
            db.set_library_source(&id, enabled)?;
            Value::Null
        }
        Request::IndexErrors { run, offset } => json!(db.index_errors(run, offset)?),
        Request::References { character } => json!(db.references(&character)?),
        Request::ReferenceHistory { character, before } => {
            json!(db.reference_history(&character, before)?)
        }
        Request::ReferenceEdit {
            character,
            variant,
            asset,
            operation,
        } => {
            match operation.as_str() {
                "add" => db.add_reference(&character, variant.as_deref(), &asset)?,
                "remove" => db.remove_reference(&character, variant.as_deref(), &asset)?,
                "primary" => db.primary_reference(&character, variant.as_deref(), &asset)?,
                _ => return Err("Action de référence inconnue".into()),
            }
            Value::Null
        }
        Request::ReferenceOrder {
            character,
            variant,
            assets,
        } => {
            db.reorder_references(&character, variant.as_deref(), &assets)?;
            Value::Null
        }
        Request::Characters { include_archived } => {
            serde_json::to_value(db.all_characters(include_archived)?)?
        }
        Request::ArchiveCharacter { id, archived } => {
            db.set_character_archived(&id, archived)?;
            Value::Null
        }
        Request::CharacterHistory { id } => json!(db.character_history(&id)?),
        Request::RestoreCharacter { id, revision } => {
            db.restore_character(&id, revision)?;
            Value::Null
        }
        Request::Variants { id } => json!(db.variants(&id)?),
        Request::SaveVariant {
            character,
            id,
            name,
            blocks,
            constraints,
        } => json!(db.save_variant(&character, id, &name, blocks, constraints)?),
        Request::ExportCharacter { id } => db.export_character(&id)?,
        Request::ImportCharacter { text } => json!(db.import_character_document(&text)?),
        Request::Preferences => {
            // Proposed capture root: inside the sandbox during a test, never the
            // user's real Images folder.
            let mut prefs = db.preferences()?;
            let default = match studio_core::gallery::test_sandbox() {
                Some(sandbox) => sandbox.join("captures"),
                None => studio_core::Database::default_capture_root()?,
            };
            if let Some(map) = prefs.as_object_mut() {
                map.insert("defaultCaptureRoot".into(), json!(default));
            }
            prefs
        }
        Request::GeneratorCatalog => studio_core::generator::catalog(),
        Request::PrepareGeneratedVariant { character, input } => {
            db.prepare_generated_variant(&character, &input)?
        }
        Request::SaveGeneratedVariant {
            character,
            name,
            input,
            revision,
            confirmed,
        } => json!(db.save_generated_variant(&character, &name, &input, revision, confirmed)?),
        Request::GenerateCharacter { input } => studio_core::generator::generate(&input)?,
        Request::SuggestCharacter { constraints, seed } => {
            studio_core::characters::suggest_character(&constraints, &seed)?
        }
        Request::Proposal { id } => db.proposal(&id)?,
        Request::Proposals => json!(db.proposals()?),
        Request::DecideProposal {
            id,
            decision,
            confirmed,
            current,
        } => json!(db.decide_proposal(&id, &decision, confirmed, &current)?),
        Request::Jobs => json!(db.jobs()?),
        Request::Checkpoints { job } => json!(db.checkpoints(&job)?),
        Request::GenerationTree { root } => db.generation_tree(&root)?,
        Request::GenerationPath { id } => db.generation_path(&id)?,
        Request::GenerationBranches { id } => db.generation_branches(&id)?,
        Request::GenerationCompare { left, right } => db.generation_compare(&left, &right)?,
        Request::RecordTreeRelation {
            parent,
            child,
            relation,
            label,
            checkpoint,
        } => json!(db.record_generation_relation(
            &parent,
            &child,
            &relation,
            &label,
            checkpoint.as_deref()
        )?),
        Request::PlanTreeBranch {
            parent,
            relation,
            label,
            checkpoint,
        } => json!(db.plan_branch_from_saved_state(
            &parent,
            &relation,
            &label,
            checkpoint.as_deref()
        )?),
        Request::LinkTreeBranch { relation, child } => {
            db.link_generation_child(&relation, &child)?;
            Value::Null
        }
        Request::AbCampaigns => json!(db.ab_campaigns()?),
        Request::AbCampaign { id } => db.ab_campaign(&id)?,
        Request::SaveAbCampaign {
            name,
            workflow,
            model,
            loras,
            prompts,
            base_seed,
            steps,
            cfg,
            sampler,
            grid_limit,
            notes,
        } => json!(db.save_ab_campaign(&CampaignSpec {
            name,
            workflow_revision_id: workflow,
            model,
            loras,
            prompts,
            base_seed,
            steps,
            cfg,
            sampler,
            grid_limit,
            notes,
        })?),
        Request::AddAbParameter {
            campaign,
            slot,
            kind,
            label,
            values,
        } => json!(db.add_ab_parameter(&campaign, &slot, &kind, &label, &values)?),
        Request::PrepareAbGrid { campaign } => db.prepare_ab_grid(&campaign)?,
        Request::AbCellDiff {
            campaign,
            cell_a,
            cell_b,
        } => db.diff_ab_cells(&campaign, &cell_a, &cell_b)?,
        Request::SetAbVerdict {
            campaign,
            cell,
            favorite,
            best,
            rank,
            note,
        } => {
            db.set_ab_verdict(&campaign, &cell, favorite, best, rank, &note)?;
            Value::Null
        }
        Request::RegisterAbCellImage {
            campaign,
            cell,
            job,
            asset,
        } => db.register_ab_cell_image(&campaign, &cell, &job, &asset)?,
        Request::SaveAbWinnerPreset {
            campaign,
            cell,
            name,
        } => {
            json!(db.save_ab_winner_preset(&campaign, &cell, &name)?)
        }
        Request::CharacterJobs { character, before } => {
            json!(db.character_jobs(&character, before.as_deref())?)
        }
        Request::JobDetails { id } => db.job_details(&id)?,
        Request::Workflows => json!(db.workflows()?),
        Request::Recipes => json!(db.recipes()?),
        Request::Recipe { id } => db.recipe_detail(&id)?,
        Request::RecipeHistory { id } => json!(db.recipe_history(&id)?),
        Request::CreateRecipe {
            job,
            name,
            description,
        } => {
            json!(db.create_recipe_from_job(&job, &name, &description)?)
        }
        Request::SaveRecipeEdit {
            id,
            content,
            note,
            version,
        } => {
            json!(db.save_recipe_edit(&id, content, &note, version)?)
        }
        Request::DuplicateRecipe { id, name } => json!(db.duplicate_recipe(&id, &name)?),
        Request::CompareRecipes { left, right } => db.compare_recipes(&left, &right)?,
        Request::ExportRecipe { id } => db.export_recipe(&id)?,
        Request::ImportRecipe { text } => json!(db.import_recipe(&text)?),
        Request::ApplyRecipe { id, workflow } => db.apply_recipe(&id, &workflow)?,
        Request::RecipeApplication { id, workflow } => db.recipe_application(&id, &workflow)?,
        Request::AbPlan { campaign } => db.ab_plan(&campaign)?,
        Request::JobProvenance { id } => db.job_provenance(&id)?,
        Request::Pipelines => json!(db.pipelines()?),
        Request::SavePipeline { text } => json!(db.save_pipeline(&text)?),
        Request::StartPipelineRun {
            pipeline,
            reuse_models,
        } => json!(db.start_pipeline_run_with_options(&pipeline, reuse_models)?),
        Request::PipelineRun { run } => db.pipeline_run(&run)?,
        Request::PipelineRuns { pipeline } => json!(db.pipeline_runs(&pipeline)?),
        Request::AssetProvenance { id } => db.asset_provenance(&id)?,
        Request::SyncAbResults { campaign } => db.sync_ab_results(&campaign)?,
        Request::AbSpecFromRecipe { recipe } => db.ab_spec_from_recipe(&recipe)?,
        Request::Modules => json!(db.workflow_modules()?),
        Request::WorkflowNodes { id } => json!(db.workflow_nodes(&id)?),
        Request::CompareModuleVersions { id, a, b } => db.compare_module_versions(&id, a, b)?,
        Request::RollbackModule { id, version } => {
            db.rollback_module(&id, version)?;
            Value::Null
        }
        Request::Annotate {
            id,
            favorite,
            note,
            review,
        } => {
            db.annotate_asset(&id, favorite, &note, &review)?;
            Value::Null
        }
        Request::Detail { id } => db.asset_detail(&id)?,
        Request::SaveCharacter { id, name, identity } => {
            json!(db.save_character(id, name, identity)?)
        }
        Request::CastingValider {
            run,
            graine,
            catalogue,
            shot,
        } => json!(db.casting_validate(&run, &graine, &catalogue, &shot)?),
        // Reusable save of a casting result: card created/attached + pass-1 image
        // attached. NEVER touches the run manifest: this call cannot start passes 2-6.
        Request::CastingEnregistrerPerso { run, stem, nom } => {
            json!(db.casting_save_character(&run, &stem, &nom)?)
        }
        Request::CastingChoices => json!(db.casting_choices()?),
        Request::CastingPreviews => db.casting_previews()?,
        // Character targeting (V1): read-only engine registry + app cards.
        Request::CastingPersos => {
            json!(db.available_characters(&studio_core::engine_runtime::engine_dir())?)
        }
        // Engine action bank (V2): read-only `data/actions/**` files, in the
        // historical _ordre.json order. No database, no writes.
        Request::ActionsLister => json!(studio_core::actions_bank::list_actions(
            &studio_core::engine_runtime::engine_dir()
                .join(studio_core::actions_bank::ACTIONS_DATA)
        )?),
        Request::DeleteCharacter { id } => {
            db.delete_character(&id)?;
            Value::Null
        }
        Request::Associate {
            character,
            asset,
            add,
        } => {
            db.associate(&character, &asset, add)?;
            Value::Null
        }
        Request::SetPreference { key, value } => {
            db.set_preference(&key, value)?;
            Value::Null
        }
        Request::ImportWorkflow { name, text } => json!(db.import_workflow(&name, &text)?),
        Request::CoupleInspection { id } => db.couple_inspection(&id)?,
        Request::Scenes => json!(db.scenes()?),
        Request::Scene { id } => db.scene(&id)?,
        Request::SaveScene { id, name, manifest } => {
            json!(db.save_scene(id.as_deref(), &name, &manifest)?)
        }
        Request::DuplicateScene { id } => json!(db.duplicate_scene(&id)?),
        Request::DeleteScene { id } => {
            db.delete_scene(&id)?;
            Value::Null
        }
        Request::PreviewScene { manifest } => db.preview_scene(&manifest)?,
        Request::Workflow { id } => json!(db.workflow(&id)?),
        Request::SaveBindings { id, fields } => {
            db.save_bindings(&id, &fields)?;
            Value::Null
        }
        Request::DataPaths
        | Request::BackupEstimate { .. }
        | Request::BackupDatabase { .. }
        | Request::VerifyBackup { .. }
        | Request::BackupDependencies { .. }
        | Request::RestoreBackup { .. }
        | Request::Image { .. }
        | Request::GrokPreview { .. }
        | Request::TrashAssets { .. }
        | Request::OpenAssetDirectory { .. }
        | Request::OpenDirectory { .. }
        | Request::EditorLoad { .. }
        | Request::EditorExport { .. }
        | Request::BlurDocument { .. }
        | Request::BlurSave { .. }
        | Request::Connection
        | Request::Models
        | Request::EngineTool { .. }
        | Request::Index { .. }
        | Request::VisualIndex { .. }
        | Request::VisualStatus
        | Request::VisualCancel
        | Request::VisualSearch { .. }
        | Request::VisualPending { .. }
        | Request::ControlJob { .. }
        | Request::LiveState { .. }
        | Request::CancelIndex
        | Request::CaptureCheckpoint { .. }
        | Request::VerifyCheckpoint { .. }
        | Request::CheckpointImage { .. }
        | Request::ReviewCheckpoint { .. }
        | Request::IndexStatus
        | Request::PrepareCouple { .. }
        | Request::SaveModule { .. }
        | Request::ConnectModule { .. }
        | Request::Module { .. }
        | Request::ModuleCandidates { .. }
        | Request::ModuleInsertPreview { .. }
        | Request::PrepareAbCells { .. }
        | Request::SplitAnalysis { .. }
        | Request::SplitPreview { .. }
        | Request::SplitSave { .. }
        | Request::GraphOpenFile { .. }
        | Request::GraphNew
        | Request::CharacterCards
        | Request::NodeCatalog
        | Request::LoraLabels
        | Request::ModelLibrary { .. }
        | Request::ModelThumbnail { .. }
        | Request::ModelDetail { .. }
        | Request::ModelFavorite { .. }
        | Request::ModelFetchMetadata { .. }
        | Request::ModelOrganizePlan { .. }
        | Request::ModelOrganizeApply { .. }
        | Request::ModelOrganizeUndo { .. }
        | Request::ModelImportDownload { .. }
        | Request::CivitaiThumbnail { .. }
        | Request::ImageTemporaryOpen { .. }
        | Request::ImageTemporaryClose { .. }
        | Request::ImageTemporarySubmit { .. }
        | Request::ImageInspect { .. }
        | Request::ImageImportWorkflow { .. }
        | Request::ImagePrepare { .. }
        | Request::CivitaiJobs
        | Request::CivitaiSearch { .. }
        | Request::CivitaiDownload { .. }
        | Request::GenerationPreset { .. }
        | Request::GraphSaveUi { .. }
        | Request::GraphOpen { .. }
        | Request::MemoryStatus
        | Request::SemanticStatus
        | Request::SemanticDiagnose
        | Request::PrepareProfiles
        | Request::PrepareProfile { .. }
        | Request::SavePrepareProfile { .. }
        | Request::DeletePrepareProfile { .. }
        | Request::SaveSemanticBinding { .. }
        | Request::PrepareBindings { .. }
        | Request::PrepareUniversal { .. }
        | Request::ExportPrepareProfile { .. }
        | Request::ImportPrepareProfile { .. }
        | Request::AuditPrepareProfile { .. }
        | Request::RemapPrepareProfile { .. }
        | Request::LoraInventory { .. }
        | Request::LoraLoaders { .. }
        | Request::LoraCover { .. }
        | Request::LoraMetadataSave { .. }
        | Request::PreviewLora { .. }
        | Request::SamplerSteps { .. }
        | Request::SayaReviewDecision { .. }
        | Request::SayaStart { .. }
        | Request::SayaRuns
        | Request::LastQueuedSeed { .. }
        | Request::SayaStatus { .. }
        | Request::SayaContinue { .. }
        | Request::SayaRestart { .. }
        | Request::SayaStop { .. }
        | Request::CastingHistory { .. }
        | Request::CastingDecide { .. }
        | Request::CastingStop { .. }
        | Request::CastingPublish { .. }
        | Request::CastingDeleteLot { .. }
        | Request::GalleryCleanup
        | Request::CastingCapabilities
        | Request::CastingPreparationStatus
        | Request::CastingPause { .. }
        | Request::CastingResume { .. }
        | Request::CastingNext { .. }
        | Request::EngineScript { .. }
        | Request::CastingLancer { .. }
        | Request::ActionDrafts
        | Request::ActionDraftSave { .. }
        | Request::ActionDraftDelete { .. }
        | Request::ActionDraftPromote { .. }
        | Request::ActionDraftsRefresh
        | Request::GenerationBrowser
        | Request::ComfyOutputInfo
        | Request::GenerationBrowserImage { .. }
        | Request::SemanticIndex { .. }
        | Request::SemanticSearch { .. }
        | Request::SemanticInvalidate
        | Request::AppMemory
        | Request::ReleasePipelineMemory { .. }
        | Request::RunNextPass { .. }
        | Request::CancelPipelineRun { .. }
        | Request::GraphEdit { .. }
        | Request::GraphView { .. }
        | Request::GraphSessions
        | Request::GraphScopedWidget { .. }
        | Request::GraphEnterSubgraph { .. }
        | Request::GraphExitSubgraph { .. }
        | Request::GraphResume { .. }
        | Request::GraphDeleteSession { .. }
        | Request::GraphCopy { .. }
        | Request::GraphPaste { .. }
        | Request::GraphPrepareApi { .. }
        | Request::GraphSaveApi { .. }
        | Request::GraphExport { .. }
        | Request::GraphClose { .. }
        | Request::GraphExtractModule { .. }
        | Request::Prepare { .. }
        | Request::Submit { .. }
        | Request::CivitaiCancel { .. }
        | Request::CivitaiPause { .. }
        | Request::CivitaiResume { .. }
        | Request::SyncJobs => unreachable!(),
    })
}
#[tauri::command]
async fn pick_image_file(app: tauri::AppHandle) -> std::result::Result<Option<String>, ApiError> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choisir une image générée")
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .blocking_pick_file()
            .map(|p| {
                p.into_path().map_err(error).and_then(|p| {
                    p.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| error("Chemin non UTF-8"))
                })
            })
            .transpose()
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn pick_workflow_file(
    app: tauri::AppHandle,
) -> std::result::Result<Option<String>, ApiError> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Ouvrir un workflow ComfyUI")
            .add_filter("Workflow JSON", &["json"])
            .blocking_pick_file()
            .map(|p| {
                p.into_path().map_err(error).and_then(|p| {
                    p.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| error("Chemin non UTF-8"))
                })
            })
            .transpose()
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn pick_data_files(app: tauri::AppHandle) -> std::result::Result<Vec<String>, ApiError> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Importer des données")
            .add_filter("JSON et scripts de données", &["json", "py"])
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|file| {
                file.into_path()
                    .map_err(error)?
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| error("Chemin non UTF-8"))
            })
            .collect()
    })
    .await
    .map_err(error)?
}
#[tauri::command]
async fn pick_directories(app: tauri::AppHandle) -> std::result::Result<Vec<String>, ApiError> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Ajouter des dossiers d’images")
            .blocking_pick_folders()
            .unwrap_or_default()
            .into_iter()
            .map(|file| {
                let path = file.into_path().map_err(error)?;
                path.to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| error("Chemin non UTF-8"))
            })
            .collect()
    })
    .await
    .map_err(error)?
}

#[tauri::command]
async fn studio(
    request: Request,
    state: tauri::State<'_, Arc<AppState>>,
) -> std::result::Result<Value, ApiError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle(&state, request).map_err(error))
        .await
        .map_err(error)?
}
#[tauri::command]
async fn get_bootstrap(
    state: tauri::State<'_, Arc<AppState>>,
) -> std::result::Result<Bootstrap, ApiError> {
    // Async + blocking pool: a sync command would wait for `s.db` on the main (UI) thread.
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.db.lock().map_err(error)?.status().map_err(error)
    })
    .await
    .map_err(error)?
}

/// Read-only dry run of the draw engine: returns the path of the produced catalog.
#[tauri::command]
async fn engine_dry_run(graine: u64) -> std::result::Result<Value, ApiError> {
    tauri::async_runtime::spawn_blocking(move || {
        studio_core::engine_runtime::dry_run(graine)
            .map(|outcome| {
                json!({
                    "graine": outcome.graine,
                    "catalogue": outcome.catalogue.display().to_string(),
                    "stdout": outcome.stdout,
                    "stderr": outcome.stderr,
                })
            })
            .map_err(error)
    })
    .await
    .map_err(error)?
}

#[cfg(feature = "smoke-test")]
#[tauri::command]
fn smoke_drop_image(app: tauri::AppHandle, path: String) -> std::result::Result<(), String> {
    use tauri::Emitter;
    app.emit_to(
        tauri::EventTarget::webview("main"),
        "tauri://drag-drop",
        json!({"paths":[path],"position":{"x":400,"y":300}}),
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "smoke-test")]
#[tauri::command]
async fn smoke_capture(app: tauri::AppHandle, name: String) -> std::result::Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> std::result::Result<String, String> {
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("Invalid capture name".into());
        }
        let windows = std::process::Command::new("niri")
            .args(["msg", "--json", "windows"])
            .output()
            .map_err(|e| e.to_string())?;
        let windows: Vec<Value> =
            serde_json::from_slice(&windows.stdout).map_err(|e| e.to_string())?;
        let window = windows
            .iter()
            .find(|w| w["pid"].as_u64() == Some(u64::from(std::process::id())))
            .ok_or("Own native window not found")?;
        let focused = std::process::Command::new("niri")
            .args([
                "msg",
                "action",
                "focus-window",
                "--id",
                &window["id"].to_string(),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !focused.status.success() {
            return Err("Cannot focus own native window".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1000));

        let path = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join(format!("{name}.png"));
        let output = std::process::Command::new("niri")
            .args([
                "msg",
                "action",
                "screenshot-window",
                "--id",
                &window["id"].to_string(),
                "--path",
            ])
            .arg(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(feature = "smoke-test")]
#[tauri::command]
fn finish_smoke(app: tauri::AppHandle, report: Value) -> std::result::Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::write(dir.join("smoke-report.json"), report.to_string()).map_err(|e| e.to_string())?;
    app.exit(if report["ok"] == true { 0 } else { 1 });
    Ok(())
}

fn comfy_port() -> Result<u16> {
    #[cfg(feature = "smoke-test")]
    if let Some(port) = std::env::var_os("SAYA_STUDIO_SMOKE_COMFY_PORT") {
        return smoke_port(&port.to_string_lossy());
    }
    Ok(8188)
}
/// Model roots from the engine's folder list; the last answer is remembered for offline use.
fn model_roots(s: &AppState, kind: &str) -> Result<Vec<(String, std::path::PathBuf)>> {
    let live = Comfy::loopback(s.comfy_port).and_then(|c| c.get("/internal/folder_paths"));
    let db = s.db.lock().map_err(|_| "Database lock unavailable")?;
    let paths = match live {
        Ok(paths) => {
            db.set_preference("modelFolderPaths", paths.clone())?;
            paths
        }
        Err(e) => {
            let prefs = db.preferences()?;
            match (
                prefs["modelFolderPaths"].as_object(),
                prefs["loraRoot"].as_str(),
            ) {
                (Some(cached), _) => Value::Object(cached.clone()),
                // Legacy single LoRA folder, used only when the engine was never reachable.
                (None, Some(root)) if kind == "loras" && !root.is_empty() => {
                    json!({"loras": [root]})
                }
                _ => {
                    return Err(format!(
                        "Dossiers de modèles inconnus : moteur ComfyUI injoignable ({e})"
                    )
                    .into())
                }
            }
        }
    };
    studio_core::model_library::roots(&paths, kind)
}

#[cfg(feature = "smoke-test")]
fn smoke_port(value: &str) -> Result<u16> {
    let port: u16 = value.parse()?;
    if port == 0 || port == 8188 {
        return Err("Le faux moteur exige un port local distinct de 0 et 8188".into());
    }
    Ok(port)
}

const NATIVE_HOST_NAME: &str = "com.saya.studio";
const EXTENSION_ID: &str = "hgbfcjoeaddihdpccfhcgeohohfejidm";

fn native_host_script() -> Option<PathBuf> {
    let installed = studio_core::paths::share_dir().join("saya_native_host.py");
    std::env::var_os("SAYA_NATIVE_HOST")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| installed.is_file().then_some(installed))
        .or_else(|| {
            #[cfg(debug_assertions)]
            let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scripts/saya_native_host.py");
            #[cfg(not(debug_assertions))]
            let dev = std::path::PathBuf::new();
            dev.is_file().then_some(dev)
        })
}

/// Installs the user-level Native Messaging manifest for Brave/Chromium and the
/// Python host shipped with the app. Best effort: a failure does not block startup;
/// the extension then falls back to the token typed in its options.
fn ensure_native_host_manifests() {
    let Some(script) = native_host_script() else {
        return;
    };
    let script = script.canonicalize().unwrap_or(script);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)) {
            eprintln!("Native host script not made executable: {e}");
        }
    }
    let manifest = json!({
        "name": NATIVE_HOST_NAME,
        "description": "Saya Studio Civitai token host",
        "path": script.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    })
    .to_string();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let Some(config) = config else {
        return;
    };
    for browser in ["google-chrome", "chromium", "BraveSoftware/Brave-Browser"] {
        let directory = config.join(browser).join("NativeMessagingHosts");
        if std::fs::create_dir_all(&directory).is_err() {
            continue;
        }
        if let Err(e) = std::fs::write(
            directory.join(format!("{NATIVE_HOST_NAME}.json")),
            &manifest,
        ) {
            eprintln!(
                "Native host manifest not written in {}: {e}",
                directory.display()
            );
        }
    }
}

/// Dedicated connection to the studio database for slow work (sidecars, `gio`, file hashing),
/// so `s.db` stays free for the UI. SQLite WAL + busy timeout handle the concurrency.
fn side_database(s: &AppState) -> Result<Database> {
    Ok(Database::open(&s.directory.join("studio.sqlite3"))?)
}

/// Open a folder in the file manager without waiting for it; the child is reaped off-thread.
fn open_detached(path: &std::path::Path) -> Result<()> {
    let mut child = std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Ouverture impossible : {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Download folders of the Civitai bridge, canonicalized (missing ones are skipped): the
/// `civitaiDownloadRoot` preference, `SAYA_CIVITAI_DOWNLOAD_ROOT`, the XDG download directory
/// ($XDG_DOWNLOAD_DIR, then user-dirs.dirs, then ~/Downloads) and the app's own folder.
fn civitai_download_roots(s: &AppState) -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    let prefs = s.db.lock().ok().and_then(|db| db.preferences().ok());
    if let Some(root) = prefs
        .as_ref()
        .and_then(|p| p["civitaiDownloadRoot"].as_str())
        .filter(|r| !r.trim().is_empty())
    {
        candidates.push(root.into());
    }
    candidates.extend(std::env::var_os("SAYA_CIVITAI_DOWNLOAD_ROOT").map(Into::into));
    candidates.extend(std::env::var_os("XDG_DOWNLOAD_DIR").map(Into::into));
    if let Some(home) = &home {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
            for line in text.lines() {
                let Some(value) = line.trim().strip_prefix("XDG_DOWNLOAD_DIR=") else {
                    continue;
                };
                let value = value.trim().trim_matches('"');
                candidates.push(value.replace("$HOME", &home.to_string_lossy()).into());
            }
        }
        candidates.push(home.join("Downloads"));
    }
    candidates.push(s.directory.join("civitai-downloads"));
    let mut roots = Vec::new();
    for candidate in candidates {
        if let Ok(root) = candidate.canonicalize() {
            if candidate.is_absolute() && !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

pub fn spawn_civitai_bridge(
    directory: &std::path::Path,
    roots: &[String],
) -> Option<std::process::Child> {
    #[cfg(debug_assertions)]
    let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/civitai_bridge.py");
    #[cfg(not(debug_assertions))]
    let dev = std::path::PathBuf::new();
    let installed = studio_core::paths::share_dir().join("civitai_bridge.py");
    let script = std::env::var("SAYA_CIVITAI_BRIDGE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| dev.is_file().then_some(dev))
        .or_else(|| installed.is_file().then_some(installed.to_path_buf()))?;
    let token = directory.join("civitai-token");
    let mut command = std::process::Command::new("python3");
    command
        .arg(script)
        .arg("--port")
        .arg("47821")
        .arg("--token-file")
        .arg(token)
        .arg("--parent-pid")
        .arg(std::process::id().to_string())
        .arg("--allowed-root")
        .arg(directory.join("civitai-downloads"));
    for root in roots {
        command.arg("--allowed-root").arg(root);
    }
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            // Smoke builds run with an isolated XDG_DATA_HOME: its parent becomes the
            // mandatory sandbox. Release builds only honour an explicit SAYA_STUDIO_SANDBOX,
            // so a user-customised XDG_DATA_HOME never restricts real captures.
            #[cfg(feature = "smoke-test")]
            if studio_core::gallery::test_sandbox().is_none() {
                let real = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"));
                if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from) {
                    if Some(&xdg) != real.as_ref() {
                        std::env::set_var("SAYA_STUDIO_SANDBOX", xdg.parent().unwrap_or(&xdg));
                    }
                }
            }
            #[cfg(feature = "smoke-test")]
            if studio_core::gallery::test_sandbox().is_none() {
                return Err("Smoke test sans dossier isolé (XDG_DATA_HOME ou SAYA_STUDIO_SANDBOX) : refus de toucher les données réelles".into());
            }
            std::fs::create_dir_all(&directory)?;
            ensure_native_host_manifests();
            let db = Database::open(&directory.join("studio.sqlite3"))
                .map_err(|e| -> Box<dyn std::error::Error> { e })?;
            // Base preset installed with the package (validated characters + multi-pass workflow).
            // Lasting settings live in the user's config folder too (see user_config).
            studio_core::user_config::enable();
            match db.import_missing_settings() {
                Ok(0) => {}
                Ok(n) => eprintln!("Réglages repris du dossier de config : {n}"),
                Err(e) => eprintln!("Réglages du dossier de config illisibles : {e}"),
            }
            if let Some(dir) = db.preferences().ok().and_then(|p| p["comfyOutputDir"].as_str().map(str::to_owned)) {
                studio_core::generation_browser::set_preferred_output_dir(&dir);
            }
            match db.disable_symlinked_sources() {
                Ok(0) => {}
                Ok(n) => eprintln!("Sources d’images par lien symbolique désactivées : {n}"),
                Err(e) => eprintln!("Vérification des sources d’images impossible : {e}"),
            }
            let presets = studio_core::paths::presets_dir();
            match db.seed_preset(&presets) {
                Ok(r) => eprintln!("Preset de base : {r}"),
                Err(e) => eprintln!("Preset de base non appliqué : {e}"),
            }
            // Disposable previews (passes 2-5, ComfyUI-side copies of finished runs) are
            // deleted at every start, off the UI thread; see studio_core::lineage.
            {
                let path = directory.join("studio.sqlite3");
                let port = comfy_port().unwrap_or(8188);
                std::thread::spawn(move || {
                    // Short best-effort probe of the running ComfyUI for its output folder.
                    studio_core::generation_browser::refresh_running_argv(port);
                    let outcome = Database::open(&path).and_then(|db| {
                        db.cleanup_previews(studio_core::generation_browser::comfy_output_dir().as_deref())
                    });
                    match outcome {
                        Ok(report) => eprintln!("Preview cleanup: {report}"),
                        Err(error) => eprintln!("Preview cleanup failed: {error}"),
                    }
                    match Database::open(&path).and_then(|db| db.purge_internal_workflows(20)) {
                        Ok(0) => {}
                        Ok(n) => eprintln!("Internal workflows purged: {n}"),
                        Err(error) => eprintln!("Internal workflow purge failed: {error}"),
                    }
                });
            }
            let comfy_port = comfy_port().map_err(|e| -> Box<dyn std::error::Error> { e })?;
            let monitor =
                studio_core::live_transport::Monitor::loopback(directory.clone(), comfy_port)
                    .map_err(|e| -> Box<dyn std::error::Error> { e })?;
            let prefs = db.preferences().unwrap_or_else(|_| json!({}));
            let mut roots: Vec<String> = Vec::new();
            if let Some(root) = prefs
                .get("loraRoot")
                .and_then(Value::as_str)
                .filter(|r| !r.trim().is_empty())
            {
                roots.push(root.to_owned());
            }
            let bridge = spawn_civitai_bridge(&directory, &roots);
            let state = Arc::new(AppState {
                comfy_port,
                engine_operations: Mutex::new(()),
                saya_operations: Mutex::new(()),
                casting_operations: Mutex::new(()),
                casting_preparation: Mutex::new(None),
                temporary_images: Mutex::default(),
                db: Mutex::new(db),
                directory,
                index: Mutex::default(),
                visual: Mutex::default(),
                editors: Mutex::default(),
                monitor: Mutex::new(Some(monitor)),
                bridge: Mutex::new(bridge),
                node_catalog: Mutex::new(None),
            });
            // Continue accepted runs even when the Generation page is not mounted.
            let weak = Arc::downgrade(&state);
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                let Some(state) = weak.upgrade() else { break };
                let Ok(_operation) = state.saya_operations.try_lock() else {
                    continue;
                };
                let Ok(mut db) = Database::open(&state.directory.join("studio.sqlite3")) else {
                    continue;
                };
                let Ok(runs) = db.saya_runs() else { continue };
                let Ok(client) = Comfy::loopback(state.comfy_port) else {
                    continue;
                };
                if let Err(e) = db.casting_tick(&client, &state.directory.join("results")) {
                    eprintln!("Casting tick failed: {e}");
                }
                for run in runs
                    .iter()
                    .filter(|r| r["state"] == "running" && r["autoAdvance"] == true)
                {
                    if let Some(id) = run["id"].as_str() {
                        if let Err(e) = db.saya_status(id, &client, &state.directory.join("results")) {
                            eprintln!("Saya auto-advance failed for run {id}: {e}");
                        }
                    }
                }
            });
            app.manage(state);
            Ok(())
        });
    #[cfg(feature = "smoke-test")]
    let builder = builder
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                if let Ok(path) = std::env::var("SAYA_STUDIO_SMOKE_SCRIPT") {
                    if std::env::var("SAYA_STUDIO_SMOKE_FULL_HD").as_deref()==Ok("1") {
                        if let Some(window)=webview.app_handle().get_webview_window("main") {
                            if let Err(error)=window.set_size(tauri::LogicalSize::new(1920.0,1080.0)) {eprintln!("Smoke resize failed: {error}");}
                        }
                    }
                    eprintln!("Smoke: page loaded, injecting {}", path);
                    match std::fs::read_to_string(&path) {
                        Ok(script) => {
                            // Placeholders let the smoke scripts stay free of machine-specific paths.
                            let repo_root = std::path::Path::new(&path)
                                .parent()
                                .and_then(|p| p.parent())
                                .map(|p| p.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let comfy_dir = std::env::var("SAYA_COMFYUI_DIR").unwrap_or_else(|_| {
                                format!("{}/AI/ComfyUI", std::env::var("HOME").unwrap_or_default())
                            });
                            let script = script
                                .replace("__SMOKE_REPO_ROOT__", &repo_root)
                                .replace("__SMOKE_COMFYUI_DIR__", &comfy_dir);
                            if let Err(error) = webview.eval(script) {
                                eprintln!("Smoke injection failed: {error}");
                            }
                        }
                        Err(error) => eprintln!("Smoke script read failed: {error}"),
                    }
                    let probe=webview.clone();
                    std::thread::spawn(move || {
                        // Periodic probe: a missing answer pinpoints a blocked WebView thread.
                        for _ in 0..40 {
                            std::thread::sleep(std::time::Duration::from_secs(15));
                            let sent = probe.eval_with_callback("JSON.stringify({step:window.__SMOKE_STEP,at:Date.now(),heading:document.querySelector('h1')?.textContent,ipc:typeof window.__TAURI_INTERNALS__?.invoke})", |value| eprintln!("Smoke probe: {value}"));
                            if sent.is_err() { break; }
                        }
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            studio,
            get_bootstrap,
            engine_dry_run,
            pick_directories,
            pick_data_files,
            pick_workflow_file,
            pick_image_file,
            finish_smoke,
            smoke_capture,
            smoke_drop_image,
            smoke_actions
        ]);
    #[cfg(not(feature = "smoke-test"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        studio,
        get_bootstrap,
        engine_dry_run,
        pick_directories,
        pick_data_files,
        pick_workflow_file,
        pick_image_file
    ]);
    builder
        .build(tauri::generate_context!())
        .expect("Saya Studio failed to start")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<Arc<AppState>>().inner().clone();
                if let Ok(mut monitor) = state.monitor.lock() {
                    monitor.take();
                };
                let bridge_guard = state.bridge.lock();
                if let Ok(mut bridge) = bridge_guard {
                    if let Some(mut child) = bridge.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn graph_paste_accepts_fragment_and_offset() {
        let request: super::Request = serde_json::from_value(serde_json::json!({"action":"graph_paste","session":"s","fragment":{"format":1,"nodes":[],"connections":[]},"offset":{"x":30.0,"y":40.0}})).unwrap();
        assert!(
            matches!(request, super::Request::GraphPaste{offset,..} if offset.x==30.0 && offset.y==40.0)
        );
    }
    #[test]
    fn graph_save_api_accepts_frontend_camel_case() {
        let request: super::Request = serde_json::from_value(serde_json::json!({"action":"graph_save_api","session":"session","name":"API","previewHash":"exact-hash","confirmed":true})).unwrap();
        assert!(
            matches!(request,super::Request::GraphSaveApi{preview_hash,confirmed:true,..} if preview_hash=="exact-hash")
        );
    }
    #[test]
    fn run_next_pass_accepts_optional_unload_policy() {
        let request: super::Request = serde_json::from_value(serde_json::json!({"action":"run_next_pass","run":"r","confirmed":true,"unload_after_pass":true})).unwrap();
        assert!(matches!(
            request,
            super::Request::RunNextPass {
                unload_after_pass: true,
                ..
            }
        ));
        let request: super::Request = serde_json::from_value(
            serde_json::json!({"action":"run_next_pass","run":"r","confirmed":true}),
        )
        .unwrap();
        assert!(matches!(
            request,
            super::Request::RunNextPass {
                unload_after_pass: false,
                ..
            }
        ));
    }

    #[test]
    fn split_confirmation_accepts_frontend_camel_case() {
        let request: super::Request = serde_json::from_value(serde_json::json!({"action":"split_save","workflow":"source","cuts":[{"node":"6","slot":0}],"output":"7","name":"Passes","previewHash":"exact-hash","confirmed":true})).unwrap();
        assert!(
            matches!(request,super::Request::SplitSave{preview_hash,confirmed:true,..} if preview_hash=="exact-hash")
        );
    }
    use super::*;
    #[test]
    fn ipc_visual_search_contract_roundtrip() {
        let directory = std::env::temp_dir().join(format!(
            "studio-visual-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let library = directory.join("library");
        std::fs::create_dir_all(&library).unwrap();
        for (name, shift) in [("original.png", 0u8), ("other.png", 200u8)] {
            let mut image = image::RgbImage::new(320, 240);
            for y in 0..240u32 {
                for x in 0..320u32 {
                    let r = ((x * 255) / 320) as u8;
                    image.put_pixel(
                        x,
                        y,
                        image::Rgb([r.saturating_add(shift), (y % 255) as u8, 90]),
                    );
                }
            }
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgb8(image)
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            std::fs::write(library.join(name), bytes).unwrap();
        }
        let state = AppState {
            comfy_port: 8188,
            engine_operations: Mutex::new(()),
            saya_operations: Mutex::new(()),
            casting_operations: Mutex::new(()),
            casting_preparation: Mutex::new(None),
            temporary_images: Mutex::default(),
            db: Mutex::new(Database::open(&directory.join("studio.sqlite3")).unwrap()),
            directory: directory.clone(),
            index: Mutex::default(),
            visual: Mutex::default(),
            editors: Mutex::default(),
            monitor: Mutex::new(None),
            bridge: Mutex::new(None),
            node_catalog: Mutex::new(None),
        };
        let request = |v| handle(&state, serde_json::from_value(v).unwrap()).unwrap();
        request(json!({"action":"index","root":library.to_string_lossy()}));
        // Gallery indexing vectorizes on add: nothing pending.
        let status = request(json!({"action":"visual_status"}));
        assert_eq!(status["running"], false);
        assert_eq!(status["pending"], 0);
        let page = request(json!({"action":"gallery","filters":{}}));
        let query = page["items"][0]["id"].as_str().unwrap().to_string();
        let result = request(json!({"action":"visual_search","asset":query}));
        assert_eq!(result["query"]["vectorized"], true);
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["score"].as_f64().unwrap() > 0.0);
        // f32 serialized as f64: compare with tolerance, not exact equality.
        assert!((result["weights"]["structure"].as_f64().unwrap() - 0.35).abs() < 1e-6);
        assert!((result["weights"]["spatial"].as_f64().unwrap() - 0.15).abs() < 1e-6);
        // Backfill on an already vectorized database stays a finished no-op.
        let report = request(json!({"action":"visual_index"}));
        assert_eq!(report["vectorized"], 0);
        assert_eq!(report["remaining"], 0);
        // The query image without a vector is analyzed on the fly, without writing.
        let invalidated = state.db.lock().unwrap().visual_invalidate().unwrap();
        assert_eq!(invalidated, 2);
        let result = request(json!({"action":"visual_search","asset":query}));
        assert_eq!(result["query"]["vectorized"], false);
        assert_eq!(request(json!({"action":"visual_status"}))["pending"], 2);
        // Unknown file refused without panic.
        assert!(handle(
            &state,
            serde_json::from_value(json!({"action":"visual_search","asset":"nope"})).unwrap()
        )
        .is_err());
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(feature = "smoke-test")]
    #[test]
    fn isolated_engine_port_never_targets_the_real_server() {
        for value in ["0", "8188", "65536", "http://remote:8188", ""] {
            assert!(smoke_port(value).is_err());
        }
        assert_eq!(smoke_port("49152").unwrap(), 49152);
    }
    #[test]
    fn ipc_character_and_gallery_contract_roundtrip() {
        let directory = std::env::temp_dir().join(format!(
            "studio-ipc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let state = AppState {
            comfy_port: 8188,
            engine_operations: Mutex::new(()),
            saya_operations: Mutex::new(()),
            casting_operations: Mutex::new(()),
            casting_preparation: Mutex::new(None),
            temporary_images: Mutex::default(),
            db: Mutex::new(Database::open(&directory.join("studio.sqlite3")).unwrap()),
            directory: directory.clone(),
            index: Mutex::default(),
            visual: Mutex::default(),
            editors: Mutex::default(),
            monitor: Mutex::new(None),
            bridge: Mutex::new(None),
            node_catalog: Mutex::new(None),
        };
        let request = |v| handle(&state, serde_json::from_value(v).unwrap()).unwrap();
        request(json!({"action":"set_preference","key":"selectedTab","value":"characters"}));
        assert_eq!(
            request(json!({"action":"preferences"}))["selectedTab"],
            "characters"
        );
        let id = request(
            json!({"action":"save_character","id":null,"name":"IPC","identity":{"identity_prompt":"exact,  tags"}}),
        );
        assert_eq!(
            request(json!({"action":"characters"}))
                .as_array()
                .unwrap()
                .len(),
            1
        );
        request(
            json!({"action":"save_variant","character":id,"id":null,"name":"Default","blocks":{"P1":"exact,  tags"},"constraints":{}}),
        );
        assert_eq!(
            request(json!({"action":"variants","id":id}))[0]["promptBlocks"]["P1"],
            "exact,  tags"
        );
        let cards = request(json!({"action":"character_cards"}));
        assert_eq!(cards.as_array().unwrap().len(), 1);
        assert_eq!(cards[0]["imageCount"], 0);
        assert_eq!(cards[0]["visibleImageCount"], 0);
        let sheet = request(json!({"action":"character_sheet","id":id}));
        assert_eq!(sheet["identityPrompt"], "exact,  tags");
        assert_eq!(sheet["variants"][0]["promptBlocks"]["P1"], "exact,  tags");
        let filters = json!({"sort":"name_asc"});
        let page = request(json!({"action":"gallery_window","filters":filters,"limit":120}));
        assert_eq!(page["total"], 0);
        assert_eq!(page["items"].as_array().unwrap().len(), 0);
        assert!(handle(
            &state,
            serde_json::from_value(json!({"action":"gallery_window","filters":filters,"limit":0}))
                .unwrap()
        )
        .is_err());
        let document = request(json!({"action":"export_character","id":id}));
        let copy = request(json!({"action":"import_character","text":document.to_string()}));
        let copy2 = request(json!({"action":"import_character","text":document.to_string()}));
        request(json!({"action":"archive_character","id":id,"archived":true}));
        assert_eq!(
            request(json!({"action":"characters"}))
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            request(json!({"action":"characters","include_archived":true}))
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            request(json!({"action":"gallery","filters":{"loras":["missing"],"favorite":true}}))
                ["total"],
            0
        );
        assert_eq!(request(json!({"action":"index_status"}))["running"], false);
        let graph = json!({"p1":{"class_type":"Text","inputs":{"text":"one"}},"p2":{"class_type":"Text","inputs":{"text":"two"}},"action":{"class_type":"Text","inputs":{"text":"shared"}},"bg":{"class_type":"Text","inputs":{"text":"scene"}}});
        let workflow = request(
            json!({"action":"import_workflow","name":"Couple IPC","text":graph.to_string()}),
        );
        let fields:Vec<_>=[("p1","P1"),("p2","P2"),("action","ACTION"),("bg","BG")].into_iter().map(|(node,section)|json!({"nodeId":node,"inputName":"text","section":section,"value":"unused"})).collect();
        request(json!({"action":"save_bindings","id":workflow,"fields":fields}));
        assert_eq!(
            request(json!({"action":"couple_inspection","id":workflow}))["roles"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        let _:Request=serde_json::from_value(json!({"action":"prepare_couple","id":workflow,"input":{"p1":"one","p2":"two","action":"shared","bg":"scene","mask":null}})).unwrap();
        // Scene composer over the same saved mappings: preview, save, list, delete.
        // The two imports stay active; the archived original would be refused.
        let manifest = json!({
            "workflowId":workflow,"composition":"couple",
            "p1":{"characterId":copy,"variantId":null},
            "p2":{"characterId":copy2,"variantId":null},
            "action":{"source":"custom","text":"shared action"},
            "bg":{"source":"workflow"},"mask":null
        });
        let preview = request(json!({"action":"preview_scene","manifest":manifest}));
        assert_eq!(preview["prompts"]["P1"]["text"], "exact,  tags");
        assert_eq!(preview["prompts"]["P2"]["text"], "exact,  tags");
        assert_eq!(preview["prompts"]["BG"]["text"], "scene");
        assert_eq!(preview["prompts"]["BG"]["changed"], false);
        let scene = request(
            json!({"action":"save_scene","id":null,"name":"IPC scene","manifest":manifest}),
        );
        assert_eq!(
            request(json!({"action":"scenes"}))
                .as_array()
                .unwrap()
                .len(),
            1
        );
        request(json!({"action":"delete_scene","id":scene}));
        assert_eq!(
            request(json!({"action":"scenes"}))
                .as_array()
                .unwrap()
                .len(),
            0
        );
        // Modules contracts: actions accepted by the IPC layer.
        assert_eq!(
            request(json!({"action":"modules"}))
                .as_array()
                .unwrap()
                .len(),
            0
        );
        // Recipe IPC: capture from a real prepared snapshot, version, diff, exchange, apply.
        request(json!({"action":"archive_character","id":id,"archived":false}));
        let job = {
            let db = state.db.lock().unwrap();
            let edits = vec![
                Edit {
                    section: Some("P1".into()),
                    node_id: "p1".into(),
                    input_name: "text".into(),
                    value: "un,  exact\n(poids:1.2)".into(),
                },
                Edit {
                    section: Some("P2".into()),
                    node_id: "p2".into(),
                    input_name: "text".into(),
                    value: "deux, exact".into(),
                },
                Edit {
                    section: Some("ACTION".into()),
                    node_id: "action".into(),
                    input_name: "text".into(),
                    value: "partagé exact".into(),
                },
                Edit {
                    section: Some("BG".into()),
                    node_id: "bg".into(),
                    input_name: "text".into(),
                    value: "décor exact".into(),
                },
            ];
            let schema =
                json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
            db.prepare_job_with_characters(
                workflow.as_str().unwrap(),
                &edits,
                &schema,
                &[studio_core::workflow::JobCharacter {
                    role: "P1".into(),
                    character_id: id.as_str().unwrap().to_owned(),
                    variant_id: None,
                }],
            )
            .unwrap()
            .job_id
        };
        let created = request(
            json!({"action":"create_recipe","job":job,"name":"Recette IPC","description":"description"}),
        );
        assert_eq!(created["version"], 1);
        let recipe_id = created["id"].as_str().unwrap().to_owned();
        let detail = request(json!({"action":"recipe","id":recipe_id}));
        assert_eq!(
            detail["content"]["prompts"]["P1"]["text"],
            "un,  exact\n(poids:1.2)"
        );
        assert_eq!(detail["content"]["characters"][0]["characterId"], id);
        let mut edited = detail["content"].clone();
        edited["prompts"]["P2"]["text"] = json!("deux modifié");
        let saved = request(
            json!({"action":"save_recipe_edit","id":recipe_id,"content":edited,"note":"v2","version":1}),
        );
        assert_eq!(saved["version"], 2);
        let history = request(json!({"action":"recipe_history","id":recipe_id}));
        assert_eq!(history.as_array().unwrap().len(), 2);
        let comparison =
            request(json!({"action":"compare_recipes","left":recipe_id,"right":recipe_id}));
        assert_eq!(comparison["identical"], true);
        let exported = request(json!({"action":"export_recipe","id":recipe_id}));
        let imported = request(json!({"action":"import_recipe","text":exported.to_string()}));
        assert_ne!(imported["id"], recipe_id);
        assert_eq!(imported["contentSha256"], saved["contentSha256"]);
        let application =
            request(json!({"action":"apply_recipe","id":recipe_id,"workflow":workflow}));
        let edits = application["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 4, "{edits:?}");
        assert!(edits.iter().any(|e| e["value"] == "deux modifié"));
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn ipc_generation_tree_actions_roundtrip() {
        let directory = std::env::temp_dir().join(format!(
            "studio-ipc-tree-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let state = AppState {
            comfy_port: 8188,
            engine_operations: Mutex::new(()),
            saya_operations: Mutex::new(()),
            casting_operations: Mutex::new(()),
            casting_preparation: Mutex::new(None),
            temporary_images: Mutex::default(),
            db: Mutex::new(Database::open(&directory.join("studio.sqlite3")).unwrap()),
            directory: directory.clone(),
            index: Mutex::default(),
            visual: Mutex::default(),
            editors: Mutex::default(),
            monitor: Mutex::new(None),
            bridge: Mutex::new(None),
            node_catalog: Mutex::new(None),
        };
        let jobs: Vec<String> = {
            let mut db = state.db.lock().unwrap();
            let workflow = db
                .import_workflow(
                    "IPC tree",
                    &json!({"1":{"class_type":"Text","inputs":{"text":"same"}}}).to_string(),
                )
                .unwrap();
            let edit = studio_core::workflow::Edit {
                node_id: "1".into(),
                input_name: "text".into(),
                value: "same".into(),
                section: Some("P1".into()),
            };
            db.save_bindings(&workflow, std::slice::from_ref(&edit))
                .unwrap();
            let info = json!({"Text":{"input":{"required":{"text":["STRING",{}]}},"output":[]}});
            ["root", "a", "b"]
                .iter()
                .map(|_| {
                    db.prepare_job(&workflow, std::slice::from_ref(&edit), &info)
                        .unwrap()
                        .job_id
                })
                .collect()
        };
        let [root, child, other] = jobs.as_slice() else {
            unreachable!()
        };
        let request = |v| handle(&state, serde_json::from_value(v).unwrap()).unwrap();
        request(
            json!({"action":"record_tree_relation","parent":root,"child":child,"relation":"variant","label":"décor"}),
        );
        let planned = request(
            json!({"action":"plan_tree_branch","parent":root,"relation":"upscale","label":"plus tard"}),
        );
        let planned = planned.as_str().unwrap();
        assert!(
            request(json!({"action":"generation_tree","root":root}))["root"]["children"]
                .as_array()
                .unwrap()
                .len()
                == 2
        );
        assert!(
            request(json!({"action":"generation_branches","id":root}))["branches"]
                .as_array()
                .unwrap()
                .len()
                == 2
        );
        request(json!({"action":"link_tree_branch","relation":planned,"child":other}));
        let path = request(json!({"action":"generation_path","id":other}));
        assert_eq!(path["path"].as_array().unwrap().len(), 2);
        // Cycles stay refused through the IPC boundary as well.
        assert!(handle(
            &state,
            serde_json::from_value(json!({"action":"record_tree_relation","parent":other,"child":root,"relation":"correction","label":""})).unwrap()
        )
        .is_err());
        let compare = request(json!({"action":"generation_compare","left":child,"right":other}));
        assert_eq!(compare["seeds"]["equal"], true);
        assert_eq!(compare["commonAncestor"], json!(root));
        for action in [
            json!({"action":"module","id":"m"}),
            json!({"action":"module","id":"m","version":2}),
            json!({"action":"save_module","workflow":"wf","module":null,"name":"n","description":"","selection":["1","2"]}),
            json!({"action":"compare_module_versions","id":"m","a":1,"b":2}),
            json!({"action":"rollback_module","id":"m","version":1}),
            json!({"action":"connect_module","id":"m","version":1,"instance":"m1","pipeline":{},
                "connection":{"inputs":[{"port":"5.model","node":"a","outputIndex":0}],
                "outputs":[{"port":"7.0","node":"b","input":"images"}],"overrides":[]}}),
        ] {
            let parsed: Request = serde_json::from_value(action).unwrap();
            assert!(matches!(
                parsed,
                Request::Module { .. }
                    | Request::SaveModule { .. }
                    | Request::CompareModuleVersions { .. }
                    | Request::RollbackModule { .. }
                    | Request::ConnectModule { .. }
            ));
        }
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ipc_editor_loads_original_and_exports_only_to_new_files() {
        let directory = std::env::temp_dir().join(format!(
            "studio-editor-ipc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let images = directory.join("images");
        std::fs::create_dir_all(&images).unwrap();
        image::RgbImage::from_pixel(4, 3, image::Rgb([12, 34, 56]))
            .save(images.join("photo.png"))
            .unwrap();
        let original = std::fs::read(images.join("photo.png")).unwrap();
        let mut db = Database::open(&directory.join("studio.sqlite3")).unwrap();
        db.index_library(&images).unwrap();
        let asset = db.gallery(&Default::default()).unwrap().items[0].id.clone();
        let state = AppState {
            comfy_port: 8188,
            engine_operations: Mutex::new(()),
            saya_operations: Mutex::new(()),
            casting_operations: Mutex::new(()),
            casting_preparation: Mutex::new(None),
            temporary_images: Mutex::default(),
            db: Mutex::new(db),
            directory: directory.clone(),
            index: Mutex::default(),
            visual: Mutex::default(),
            editors: Mutex::default(),
            monitor: Mutex::new(None),
            bridge: Mutex::new(None),
            node_catalog: Mutex::new(None),
        };
        let request = |v| handle(&state, serde_json::from_value(v).unwrap());
        let loaded = request(json!({"action":"editor_load","id":asset})).unwrap();
        assert_eq!(loaded["width"], 4);
        assert_eq!(loaded["height"], 3);
        assert!(loaded["data"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(request(json!({"action":"editor_load","id":"inconnu"})).is_err());
        // Export to a new file: written, valid content, original intact.
        let canvas = studio_core::editor::load_original(&images.join("photo.png")).unwrap()["data"]
            .as_str()
            .unwrap()
            .strip_prefix("data:image/png;base64,")
            .unwrap()
            .to_string();
        let destination = images.join("photo retouche.png");
        let saved = request(
            json!({"action":"editor_export","id":asset,"destination":destination.display().to_string(),"format":"png","image":canvas}),
        )
        .unwrap();
        assert_eq!(saved["destination"], destination.display().to_string());
        assert_eq!(std::fs::read(&destination).unwrap(), original);
        // Refusals: existing destination and destination identical to the source.
        let again = request(
            json!({"action":"editor_export","id":asset,"destination":destination.display().to_string(),"format":"png","image":canvas}),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(again.contains("existe déjà"), "message inattendu : {again}");
        let same = request(
            json!({"action":"editor_export","id":asset,"destination":images.join("photo.png").display().to_string(),"format":"png","image":canvas}),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(same.contains("refus"), "message inattendu : {same}");
        assert_eq!(std::fs::read(images.join("photo.png")).unwrap(), original);
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
mod casting_preparation_tests {
    use super::*;
    #[test]
    fn preparation_progress_is_independent_and_cleared_on_early_return() {
        let state = Mutex::new(None);
        {
            let progress = CastingPreparationGuard::start(&state, 200).unwrap();
            assert_eq!(state.lock().unwrap().as_ref().unwrap().stage, "draw");
            progress.update("prepare", 37, 200).unwrap();
            let value = state.lock().unwrap();
            assert_eq!(value.as_ref().unwrap().completed, 37);
            assert_eq!(value.as_ref().unwrap().total, 200);
        }
        assert!(state.lock().unwrap().is_none());
    }
    #[test]
    fn casting_history_remains_readable_while_orchestration_is_locked() {
        let directory = std::env::temp_dir().join(format!(
            "casting-history-lock-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let db = Database::open(&directory.join("studio.sqlite3")).unwrap();
        let state = AppState {
            comfy_port: 1,
            engine_operations: Mutex::new(()),
            saya_operations: Mutex::new(()),
            casting_operations: Mutex::new(()),
            casting_preparation: Mutex::new(None),
            temporary_images: Mutex::default(),
            db: Mutex::new(db),
            directory: directory.clone(),
            index: Mutex::default(),
            visual: Mutex::default(),
            editors: Mutex::default(),
            monitor: Mutex::new(None),
            bridge: Mutex::new(None),
            node_catalog: Mutex::new(None),
        };

        let state = Arc::new(state);
        let lock = state.saya_operations.lock().unwrap();
        let worker = Arc::clone(&state);
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            tx.send(handle(&worker, Request::CastingHistory { sync: true }))
                .unwrap();
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(2));
        drop(lock);
        thread.join().unwrap();
        assert_eq!(
            result.expect("history blocked by orchestration").unwrap(),
            json!([])
        );
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
