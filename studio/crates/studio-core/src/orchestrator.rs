//! Multi-pass orchestration over the existing runner: a GLM v1 pipeline
//! manifest is executed one pass per call, each pass being an ordinary job of
//! the job manager (prepare → confirmed submit → sync). Pass outputs are kept
//! in the pass-state store (pinned, hash-verified), uploaded back to the
//! engine for the next pass, and every completed pass is revalidated before
//! reuse. This is a logical pipeline resume, never an exact sampler resume.
use crate::comfy::Comfy;
use crate::gallery::digest;
use crate::generation_storage;
use crate::generation_store;
use crate::pass_states::{self, NewState, PassMemoryManager, Retention, StateKind};
use crate::vram_policy;
use crate::workflow_pipeline::{PassKind, PassStatus, Step, WorkflowPipeline};
use crate::{Database, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

const STORE_BUDGET: u64 = 8 * 1024 * 1024 * 1024;
const MAX_STATE: u64 = 64 * 1024 * 1024;

pub(crate) fn entry_sha(bytes: &[u8]) -> String {
    digest(bytes)
}

pub(crate) fn store_name(run: &str) -> String {
    format!("run-{run}")
}

/// Store key of one attempt: a re-execution after corruption gets a fresh
/// key, so a corrupt state is quarantined, never overwritten or reused.
pub(crate) fn attempt_key(step: &str, job: &str) -> String {
    format!("{step}-{}", &job[..8.min(job.len())])
}

/// Settings a pass manifest asks the runner to apply, resolved against the
/// workflow by the preflight. Anything not resolvable is refused, never dropped.
#[derive(Debug, Clone, Default)]
pub struct PassSettings {
    /// (node, input, exact value) writes applied to the pass graph.
    pub writes: Vec<(String, String, Value)>,
}

const SECTIONS: [&str; 6] = ["QUALITY", "P1", "P2", "ACTION", "BG", "NEG"];

/// Text → value typed like the workflow field it replaces (u64 exact).
fn typed(field: &crate::workflow::Field, text: &str) -> Result<Value> {
    Ok(match field.value_type.as_str() {
        "string" => json!(text),
        "seed" => json!(String::from(crate::contracts::Seed::parse(text)?).parse::<u64>()?),
        "integer" => text
            .parse::<u64>()
            .map(|v| json!(v))
            .or_else(|_| text.parse::<i64>().map(|v| json!(v)))
            .map_err(|_| {
                format!(
                    "{}.{} : entier attendu, reçu « {text} »",
                    field.node_id, field.input_name
                )
            })?,
        "number" => {
            let value: f64 = text.parse().map_err(|_| {
                format!(
                    "{}.{} : nombre attendu, reçu « {text} »",
                    field.node_id, field.input_name
                )
            })?;
            if !value.is_finite() {
                return Err("Nombre fini attendu".into());
            }
            json!(value)
        }
        "boolean" => json!(text.parse::<bool>().map_err(|_| format!(
            "{}.{} : true/false attendu",
            field.node_id, field.input_name
        ))?),
        other => return Err(format!("Type de champ non pris en charge : {other}").into()),
    })
}

/// A missing or corrupt stored state is `Ok(None)` (the step gets invalidated);
/// any other I/O failure (permissions, disk) is propagated instead of silently
/// triggering a re-run.
fn absent_or_corrupt<T>(result: Result<T>) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) => match error.downcast_ref::<std::io::Error>() {
            Some(io)
                if !matches!(
                    io.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::InvalidData
                        | std::io::ErrorKind::UnexpectedEof
                ) =>
            {
                Err(error)
            }
            _ => Ok(None),
        },
    }
}

impl Database {
    /// Execution guard run before any write or submission, on every call
    /// (fresh run or resume). Returns the settings each pass will apply. With
    /// `info`, every pass graph (settings applied) is validated against the
    /// live engine schema. Every unsupported option is an explicit error.
    pub fn preflight_pipeline(
        &self,
        manifest: &WorkflowPipeline,
        info: Option<&Value>,
    ) -> Result<std::collections::BTreeMap<String, PassSettings>> {
        if manifest.format != crate::workflow_pipeline::PIPELINE_FORMAT
            || !matches!(
                manifest.version,
                crate::workflow_pipeline::PIPELINE_VERSION
                    | crate::workflow_pipeline::PIPELINE_MULTILINE_VERSION
            )
        {
            return Err("Manifeste de pipeline : format ou version non pris en charge".into());
        }
        manifest.validate()?;
        for workflow in &manifest.workflows {
            let hash: Option<String> = self
                .connection
                .query_row(
                    "SELECT source_sha256 FROM workflow_revisions WHERE id=?",
                    [&workflow.id],
                    |r| r.get(0),
                )
                .optional()?;
            let hash =
                hash.ok_or_else(|| format!("Workflow du pipeline introuvable : {}", workflow.id))?;
            if workflow
                .source_sha256
                .as_ref()
                .is_some_and(|pinned| pinned != &hash)
            {
                return Err(format!(
                    "Workflow {} modifié depuis l'épinglage du pipeline",
                    workflow.id
                )
                .into());
            }
        }
        let mut plans = std::collections::BTreeMap::new();
        for step in &manifest.steps {
            let label = format!("Passe {}", step.id.as_str());
            if step.status != PassStatus::Pending {
                return Err(format!("{label} : statut de manifeste « {:?} » non pris en charge (l'état d'exécution est tenu par Saya Studio)", step.status).into());
            }
            if step.state.is_some() {
                return Err(format!(
                    "{label} : état intermédiaire déclaré dans le manifeste non pris en charge"
                )
                .into());
            }
            if step.resume.is_some() {
                return Err(format!("{label} : conditions de reprise déclarées non prises en charge (reprise gérée par l'exécution)").into());
            }
            let workflow = self.workflow(&step.workflow_id)?;
            let (text,): (String,) = self.connection.query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [&step.workflow_id],
                |r| Ok((r.get(0)?,)),
            )?;
            let mut graph: Value = serde_json::from_str(&text)?;
            // Ports and bindings: IMAGE or LATENT, each input bound once.
            for port in &step.inputs {
                if !matches!(port.port_type.as_str(), "IMAGE" | "LATENT") {
                    return Err(format!(
                        "{label} : entrée {} de type {} non prise en charge (IMAGE/LATENT uniquement)",
                        port.name,
                        port.port_type.as_str()
                    )
                    .into());
                }
                let count = step.bindings.iter().filter(|b| b.port == port.name).count();
                if count != 1 {
                    return Err(format!(
                        "{label} : l'entrée {} doit recevoir exactement une liaison ({count})",
                        port.name
                    )
                    .into());
                }
            }
            let mut has_latent_binding = false;
            for binding in &step.bindings {
                if !step.inputs.iter().any(|p| p.name == binding.port) {
                    return Err(format!(
                        "{label} : liaison vers un port non déclaré : {}",
                        binding.port
                    )
                    .into());
                }
                let source = manifest
                    .steps
                    .iter()
                    .find(|s| s.id == binding.source.step)
                    .ok_or("Source de liaison absente")?;
                let output = source
                    .outputs
                    .iter()
                    .find(|o| o.name == binding.source.port)
                    .ok_or_else(|| {
                        format!(
                            "{label} : sortie {}.{} inexistante",
                            binding.source.step.as_str(),
                            binding.source.port
                        )
                    })?;
                let port = step
                    .inputs
                    .iter()
                    .find(|p| p.name == binding.port)
                    .ok_or("Port de liaison absent")?;
                let expected = port.port_type.as_str();
                if output.port_type.as_str() != expected {
                    return Err(format!(
                        "{label} : liaison {} depuis une sortie {} vers une entrée {expected} refusée",
                        binding.port,
                        output.port_type.as_str()
                    )
                    .into());
                }
                let wanted_kind = match expected {
                    "IMAGE" => PassKind::Image,
                    "LATENT" => PassKind::Latent,
                    _ => unreachable!(),
                };
                if source.kind != wanted_kind {
                    return Err(format!(
                        "{label} : source {} de kind {:?} incompatible avec la frontière {expected}",
                        source.id.as_str(),
                        source.kind
                    )
                    .into());
                }
                let node = graph.get(&binding.node_id).ok_or_else(|| {
                    format!(
                        "{label} : node cible {} absent du workflow",
                        binding.node_id
                    )
                })?;
                match expected {
                    "IMAGE" => {
                        if node["class_type"] != "LoadImage" || binding.input != "image" {
                            return Err(format!("{label} : liaison IMAGE acceptée uniquement vers LoadImage.image (reçu {}.{})", binding.node_id, binding.input).into());
                        }
                    }
                    "LATENT" => {
                        // ComfyUI LoadLatent takes a filename choice, not a graph
                        // link. The runtime resolves the SaveLatent output file;
                        // preflight validates the node/input contract only and
                        // never invents a filename or converts the latent.
                        if node["class_type"] != "LoadLatent" || binding.input != "latent" {
                            return Err(format!("{label} : liaison LATENT acceptée uniquement vers LoadLatent.latent (reçu {}.{})", binding.node_id, binding.input).into());
                        }
                        has_latent_binding = true;
                    }
                    _ => unreachable!(),
                }
            }
            if step.outputs.is_empty() {
                return Err(format!("{label} : aucune sortie déclarée").into());
            }
            let mut seen_outputs = std::collections::BTreeSet::new();
            for output in &step.outputs {
                if !matches!(output.port_type.as_str(), "IMAGE" | "LATENT") {
                    return Err(format!(
                        "{label} : sortie {} de type {} non prise en charge",
                        output.name,
                        output.port_type.as_str()
                    )
                    .into());
                }
                if !seen_outputs.insert(output.port_type.as_str()) {
                    return Err(format!(
                        "{label} : une seule sortie par type prise en charge ({} dupliquée)",
                        output.port_type.as_str()
                    )
                    .into());
                }
            }
            let mut saves = 0;
            for output in &step.outputs {
                if output.port_type.as_str() == "IMAGE" {
                    saves += graph
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(_, n)| n["class_type"] == "SaveImage")
                        .count();
                } else {
                    saves += graph
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(_, n)| n["class_type"] == "SaveLatent")
                        .count();
                }
            }
            if saves == 0 {
                return Err(format!(
                    "{label} : aucune sortie SaveImage/SaveLatent correspondant aux ports"
                )
                .into());
            }
            // Seed and parameters: applied exactly, or refused.
            let mut settings = PassSettings::default();
            if let Some(seed) = &step.seed {
                let seeds: Vec<&crate::workflow::Field> = workflow
                    .fields
                    .iter()
                    .filter(|f| f.value_type == "seed")
                    .collect();
                let [field] = seeds.as_slice() else {
                    return Err(format!("{label} : seed déclarée mais non applicable ({} champ(s) seed dans le workflow)", seeds.len()).into());
                };
                settings.writes.push((
                    field.node_id.clone(),
                    field.input_name.clone(),
                    typed(field, &String::from(seed.clone()))?,
                ));
            }
            for (key, value) in &step.parameters {
                let field = if SECTIONS.contains(&key.as_str()) {
                    let mapped: Vec<&crate::workflow::Field> = workflow
                        .fields
                        .iter()
                        .filter(|f| f.enabled && f.section.as_deref() == Some(key.as_str()))
                        .collect();
                    match mapped.as_slice() {
                        [field] => *field,
                        [] => {
                            return Err(format!(
                                "{label} : paramètre {key} sans section mappée dans le workflow"
                            )
                            .into())
                        }
                        _ => {
                            return Err(format!(
                                "{label} : paramètre {key} ambigu (plusieurs entrées mappées)"
                            )
                            .into())
                        }
                    }
                } else if let Some((node, input)) = key.split_once('.') {
                    workflow.fields.iter().find(|f| f.node_id == node && f.input_name == input).ok_or_else(|| format!("{label} : paramètre {key} introuvable dans le workflow (entrée primitive attendue)"))?
                } else {
                    return Err(format!("{label} : paramètre « {key} » non pris en charge (section P1…NEG ou « node.entrée » explicite)").into());
                };
                if settings
                    .writes
                    .iter()
                    .any(|(n, i, _)| n == &field.node_id && i == &field.input_name)
                {
                    return Err(format!(
                        "{label} : {}.{} défini deux fois (seed/paramètres)",
                        field.node_id, field.input_name
                    )
                    .into());
                }
                settings.writes.push((
                    field.node_id.clone(),
                    field.input_name.clone(),
                    typed(field, value)?,
                ));
            }
            if let Some(info) = info {
                for (node, input, value) in &settings.writes {
                    graph[node]["inputs"][input] = value.clone();
                }
                if !has_latent_binding {
                    crate::workflow_split::validate_with_uploads(&graph, info, step)
                        .map_err(|e| format!("{label} : refusée par le schéma du moteur : {e}"))?;
                } else {
                    // LoadLatent.latent is a filename choice resolved from the
                    // saved latent at runtime; validating it here would require
                    // inventing a path. Validate only that the workflow classes
                    // declared by the manifest are present in object_info.
                    for node in graph.as_object().into_iter().flatten().map(|(_, n)| n) {
                        let class = node["class_type"].as_str().unwrap_or_default();
                        if !info.get(class).is_some() {
                            return Err(
                                format!("{label} : classe {class} absente de object_info").into()
                            );
                        }
                    }
                }
            }
            plans.insert(step.id.as_str().to_owned(), settings);
        }
        Ok(plans)
    }

    fn event(&self, run: &str, step: Option<&str>, kind: &str, detail: Value) -> Result<()> {
        self.connection.execute(
            "INSERT INTO pipeline_run_events(run_id,step_id,kind,detail_json) VALUES(?1,?2,?3,?4)",
            params![run, step, kind, detail.to_string()],
        )?;
        Ok(())
    }

    /// Stores a validated pipeline definition; every referenced workflow must
    /// exist and match its pinned source hash when one is given.
    pub fn save_pipeline(&self, text: &str) -> Result<String> {
        let pipeline = WorkflowPipeline::parse(text)?;
        // Static guard: everything checkable without the engine is refused now.
        self.preflight_pipeline(&pipeline, None)?;
        let canonical = pipeline.to_json()?;
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO pipeline_definitions(id,name,manifest_json,manifest_sha256) VALUES(?1,?2,?3,?4)",
            params![id, pipeline.name, canonical, digest(canonical.as_bytes())],
        )?;
        Ok(id)
    }

    pub fn pipelines(&self) -> Result<Vec<Value>> {
        let rows = self
            .connection
            .prepare("SELECT id,name,manifest_sha256,created_at FROM pipeline_definitions ORDER BY created_at DESC")?
            .query_map([], |r| {
                Ok(json!({"id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "manifestSha256": r.get::<_, String>(2)?, "createdAt": r.get::<_, String>(3)?}))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn pipeline_runs(&self, pipeline: &str) -> Result<Vec<Value>> {
        let rows = self
            .connection
            .prepare("SELECT id,state,created_at,updated_at FROM pipeline_runs WHERE pipeline_id=? ORDER BY created_at DESC")?
            .query_map([pipeline], |r| {
                Ok(json!({"run": r.get::<_, String>(0)?, "state": r.get::<_, String>(1)?, "createdAt": r.get::<_, String>(2)?, "updatedAt": r.get::<_, String>(3)?}))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    fn load_pipeline(&self, id: &str) -> Result<(WorkflowPipeline, String)> {
        let (text, sha): (String, String) = self
            .connection
            .query_row(
                "SELECT manifest_json,manifest_sha256 FROM pipeline_definitions WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or("Pipeline inconnu")?;
        if digest(text.as_bytes()) != sha {
            return Err("Manifeste de pipeline altéré : exécution refusée".into());
        }
        Ok((WorkflowPipeline::parse(&text)?, sha))
    }

    fn existing_generation_dir(&self, run: &str) -> Result<Option<std::path::PathBuf>> {
        let base = self
            .pass_state_root()?
            .parent()
            .ok_or("Racine d'états sans parent")?
            .join("generations");
        let existing = base.join("pipeline").join(run);
        Ok(existing.join("manifest.json").is_file().then_some(existing))
    }

    fn vram_policy(&self, step: &Step, client: &Comfy) -> Result<Value> {
        let document: String = self
            .connection
            .query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [&step.workflow_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "{}".into());
        let api: Value = serde_json::from_str(&document).unwrap_or(Value::Null);
        let free = client
            .get("/system_stats")
            .ok()
            .and_then(|s| {
                s["devices"]
                    .as_array()
                    .and_then(|d| d.first())
                    .and_then(|d| d["vram_free"].as_u64())
            })
            .map(|bytes| bytes / 1_048_576);
        vram_policy::plan(&api, free)
    }

    fn generation_dir(&self, run: &str) -> Result<std::path::PathBuf> {
        if let Some(existing) = self.existing_generation_dir(run)? {
            return Ok(existing);
        }
        let base = self
            .pass_state_root()?
            .parent()
            .ok_or("Racine d'états sans parent")?
            .join("generations");
        Ok(generation_store::create(&base, "pipeline", run)?.0)
    }

    fn generation_meta(&self, pipeline: &str, manifest_sha: &str) -> Value {
        json!({"pipelineId":pipeline,"manifestSha256":manifest_sha,"format":"saya-generation"})
    }

    pub fn start_pipeline_run(&self, pipeline: &str) -> Result<String> {
        self.start_pipeline_run_with_options(pipeline, false)
    }

    pub fn start_pipeline_run_with_options(
        &self,
        pipeline: &str,
        reuse_models: bool,
    ) -> Result<String> {
        let (manifest, sha) = self.load_pipeline(pipeline)?;
        let order = manifest.execution_order()?;
        let run = uuid::Uuid::new_v4().to_string();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO pipeline_runs(id,pipeline_id,manifest_sha256) VALUES(?1,?2,?3)",
            params![run, pipeline, sha],
        )?;
        for (position, step) in order.iter().enumerate() {
            tx.execute(
                "INSERT INTO pipeline_run_steps(run_id,step_id,position) VALUES(?1,?2,?3)",
                params![run, step, position as i64],
            )?;
        }
        tx.commit()?;
        let gen_dir = self.generation_dir(&run).ok();
        if let Some(dir) = &gen_dir {
            generation_store::update_metadata(dir, &self.generation_meta(pipeline, &sha))?;
        }
        self.event(
            &run,
            None,
            "run_created",
            json!({"steps": order, "generationDir": gen_dir.as_ref().map(|d| d.to_string_lossy()), "reuseModels": reuse_models}),
        )?;
        Ok(run)
    }

    fn step_row(
        &self,
        run: &str,
        step: &str,
    ) -> Result<(String, Option<String>, String, Option<String>)> {
        Ok(self.connection.query_row(
            "SELECT state,job_id,input_fingerprint,output_sha256 FROM pipeline_run_steps WHERE run_id=?1 AND step_id=?2",
            params![run, step],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?)
    }

    /// Fingerprint of everything a pass consumes: its workflow revision and
    /// bindings, and the exact upstream outputs it reads.
    fn fingerprint(&self, run: &str, step: &Step) -> Result<Option<String>> {
        let workflow: Option<String> = self
            .connection
            .query_row(
                "SELECT source_sha256 FROM workflow_revisions WHERE id=?",
                [&step.workflow_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(workflow) = workflow else {
            return Ok(None);
        };
        let mut upstream = Vec::new();
        for binding in &step.bindings {
            let (_, _, _, sha) = self.step_row(run, binding.source.step.as_str())?;
            let Some(sha) = sha else { return Ok(None) };
            upstream.push(json!({"step": binding.source.step.as_str(), "sha": sha}));
        }
        let material = json!({"workflow": workflow, "bindings": step.bindings, "parameters": step.parameters, "seed": step.seed, "upstream": upstream});
        Ok(Some(digest(material.to_string().as_bytes())))
    }

    /// Revalidates completed passes in execution order. A pass whose inputs
    /// changed, or whose stored state is missing/corrupt, is invalidated with
    /// every downstream pass; nothing is silently reused.
    fn revalidate(&self, run: &str, manifest: &WorkflowPipeline, root: &Path) -> Result<()> {
        let order = manifest.execution_order()?;
        let mut invalid: Vec<String> = Vec::new();
        for id in order {
            let step = manifest
                .steps
                .iter()
                .find(|s| s.id.as_str() == id)
                .ok_or("Étape absente")?;
            let (state, job, stored_fingerprint, sha) = self.step_row(run, &id)?;
            let depends_on_invalid = step
                .bindings
                .iter()
                .any(|b| invalid.iter().any(|i| i == b.source.step.as_str()));
            if state != "completed" {
                if depends_on_invalid {
                    invalid.push(id);
                }
                continue;
            }
            let reason = if depends_on_invalid {
                Some("Dépendance amont invalidée".to_owned())
            } else if self.fingerprint(run, step)?.as_deref() != Some(stored_fingerprint.as_str()) {
                Some(
                    "Entrées modifiées depuis l'exécution (workflow, liaisons ou amont)".to_owned(),
                )
            } else {
                let key = attempt_key(&id, job.as_deref().unwrap_or_default());
                let gen = self.existing_generation_dir(run)?;
                match sha.as_deref() {
                    Some(sha) => {
                        let by_state = absent_or_corrupt(pass_states::verify_state(
                            root,
                            &store_name(run),
                            &key,
                            sha,
                        ))?;
                        let by_store = match gen.as_ref() {
                            Some(g) => absent_or_corrupt(generation_store::load_pass(g, &id, sha))?,
                            None => None,
                        };
                        let invalid = if gen.is_some() {
                            by_store.is_none() || by_state.is_none()
                        } else {
                            by_state.is_none()
                        };
                        if invalid {
                            Some("État enregistré refusé : magasin Saya ou état de passe absent/corrompu".to_owned())
                        } else {
                            None
                        }
                    }
                    None => Some("État de sortie absent".to_owned()),
                }
            };
            if let Some(reason) = reason {
                self.connection.execute(
                    "UPDATE pipeline_run_steps SET state='invalidated',message=?3,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=?1 AND step_id=?2",
                    params![run, id, reason],
                )?;
                self.event(run, Some(&id), "invalidated", json!({"reason": reason}))?;
                invalid.push(id);
            }
        }
        Ok(())
    }

    pub fn pipeline_run(&self, run: &str) -> Result<Value> {
        let (pipeline, state, created): (String, String, String) = self
            .connection
            .query_row(
                "SELECT pipeline_id,state,created_at FROM pipeline_runs WHERE id=?",
                [run],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or("Exécution inconnue")?;
        let steps: Vec<Value> = self
            .connection
            .prepare("SELECT step_id,position,state,job_id,output_kind,output_sha256,output_bytes,message,updated_at FROM pipeline_run_steps WHERE run_id=? ORDER BY position")?
            .query_map([run], |r| {
                Ok(json!({"step": r.get::<_, String>(0)?, "position": r.get::<_, i64>(1)?, "state": r.get::<_, String>(2)?,
                    "job": r.get::<_, Option<String>>(3)?, "outputKind": r.get::<_, Option<String>>(4)?, "outputSha256": r.get::<_, Option<String>>(5)?,
                    "outputBytes": r.get::<_, Option<i64>>(6)?, "message": r.get::<_, String>(7)?, "updatedAt": r.get::<_, String>(8)?}))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let events: Vec<Value> = self
            .connection
            .prepare("SELECT step_id,kind,detail_json,at FROM pipeline_run_events WHERE run_id=? ORDER BY id")?
            .query_map([run], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
            })?
            .map(|row| row.map_err(Into::into).and_then(|(step, kind, detail, at)| -> Result<Value> {
                Ok(json!({"step": step, "kind": kind, "detail": serde_json::from_str::<Value>(&detail)?, "at": at}))
            }))
            .collect::<Result<_>>()?;
        Ok(
            json!({"run": run, "pipeline": pipeline, "state": state, "createdAt": created, "steps": steps, "events": events}),
        )
    }

    /// Explicit release only after all completed outputs have been verified.
    pub fn release_pipeline_memory(
        &self,
        run: &str,
        manager: &crate::resources::ComfyMemoryManager,
        confirmed: bool,
    ) -> Result<Value> {
        if !confirmed {
            return Err("Confirmation explicite requise pour décharger les modèles".into());
        }
        let detail = self.pipeline_run(run)?;
        let steps = detail["steps"].as_array().ok_or("Étapes absentes")?;
        if !steps.iter().any(|s| s["state"] == "completed")
            || steps.iter().any(|s| s["state"] == "submitted")
        {
            return Err("Déchargement réservé à une frontière de passe enregistrée, sans soumission en cours".into());
        }
        let root = self.pass_state_root()?;
        for step in steps.iter().filter(|s| s["state"] == "completed") {
            let id = step["step"].as_str().ok_or("Étape absente")?;
            let job = step["job"].as_str().ok_or("Job de passe absent")?;
            let sha = step["outputSha256"].as_str().ok_or("Empreinte absente")?;
            pass_states::verify_state(&root, &store_name(run), &attempt_key(id, job), sha)?;
        }
        let (_, report) = manager.release_measured()?;
        self.event(run, None, "memory_release_observed", report.clone())?;
        Ok(report)
    }

    /// Explicit cancellation. Without an active pass the run is simply
    /// stopped (no engine call). With a submitted pass, the existing job
    /// control is used (queue removal, or targeted interrupt when running);
    /// the pass is marked failed only when the engine confirms it, otherwise
    /// it stays tracked and its real outcome is observed on the next sync.
    pub fn cancel_pipeline_run(&self, run: &str, client: &Comfy, confirmed: bool) -> Result<Value> {
        if !confirmed {
            return Err("Confirmation explicite requise pour annuler l'exécution".into());
        }
        let active: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT step_id,job_id FROM pipeline_run_steps WHERE run_id=?1 AND state='submitted' AND job_id IS NOT NULL ORDER BY position LIMIT 1",
                [run],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((step, job)) = active else {
            self.set_run_state(run, "paused")?;
            self.event(run, None, "stopped", json!({"engine": false}))?;
            return Ok(json!({"run": run, "stopped": true, "engineCalled": false}));
        };
        let prompt: Option<String> = self.connection.query_row(
            "SELECT comfy_prompt_id FROM generation_jobs WHERE id=?",
            [&job],
            |r| r.get(0),
        )?;
        let queue = client.get("/queue")?;
        let running = prompt.as_deref().is_some_and(|p| {
            crate::comfy::queue_contains(&queue, "queue_running", p).unwrap_or(false)
        });
        let action = if running { "interrupt" } else { "cancel_queue" };
        self.event(
            run,
            Some(&step),
            "cancel_requested",
            json!({"job": job, "action": action}),
        )?;
        let outcome = self.control_job(&job, action, true, client)?;
        self.set_run_state(run, "paused")?;
        if outcome["confirmed"] == json!(true) {
            self.connection.execute(
                "UPDATE pipeline_run_steps SET state='failed',message='Annulée par l''utilisateur (confirmée par le moteur)',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=?1 AND step_id=?2 AND job_id=?3",
                params![run, step, job],
            )?;
            self.event(run, Some(&step), "cancelled", json!({"job": job}))?;
        }
        Ok(json!({"run": run, "step": step, "job": job, "action": action, "engine": outcome}))
    }

    fn set_run_state(&self, run: &str, state: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE pipeline_runs SET state=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![run, state],
        )?;
        Ok(())
    }

    /// Executes (or resumes) exactly one pass, then returns: stopping between
    /// passes is simply not calling again. `confirmed` is required for every
    /// submission because each pass is a real engine generation.
    pub fn run_next_pass(
        &mut self,
        run: &str,
        client: &Comfy,
        results: &Path,
        confirmed: bool,
        timeout: Duration,
    ) -> Result<Value> {
        self.run_next_pass_with_memory(run, client, results, confirmed, timeout, None, false)
    }

    /// Same as [`Self::run_next_pass`], with an explicit memory policy. When
    /// `unload_after_pass` is set, `prepare_pass` runs before submitting a new
    /// pass and `release_after_pass` runs only after the pass state is durably
    /// saved and verified. A resume of an already-submitted pass never prepares
    /// or releases implicitly.
    #[allow(clippy::too_many_arguments)]
    pub fn run_next_pass_with_memory(
        &mut self,
        run: &str,
        client: &Comfy,
        results: &Path,
        confirmed: bool,
        timeout: Duration,
        memory: Option<&dyn PassMemoryManager>,
        unload_after_pass: bool,
    ) -> Result<Value> {
        if unload_after_pass && memory.is_none() {
            return Err("Politique mémoire demandée sans gestionnaire de mémoire".into());
        }
        let pipeline: String = self
            .connection
            .query_row(
                "SELECT pipeline_id FROM pipeline_runs WHERE id=?",
                [run],
                |r| r.get(0),
            )
            .optional()?
            .ok_or("Exécution inconnue")?;
        let (manifest, _) = self.load_pipeline(&pipeline)?;
        // Guard before any write or submission, also on resume.
        let info = client.get("/object_info")?;
        let plans = self.preflight_pipeline(&manifest, Some(&info))?;
        let root = self.pass_state_root()?;
        if !root.join("manifest.json").exists() {
            pass_states::init_store(&root, STORE_BUDGET)?;
        }
        self.revalidate(run, &manifest, &root)?;
        let order = manifest.execution_order()?;
        let mut next = None;
        for id in &order {
            let (state, job, _, _) = self.step_row(run, id)?;
            if state != "completed" {
                next = Some((id.clone(), state, job));
                break;
            }
        }
        let Some((id, state, job)) = next else {
            self.set_run_state(run, "completed")?;
            return Ok(json!({"run": run, "done": true}));
        };
        let step = manifest
            .steps
            .iter()
            .find(|s| s.id.as_str() == id)
            .ok_or("Étape absente")?
            .clone();
        let vram = self.vram_policy(&step, client)?;
        let mut memory_prepare: Option<Value> = None;
        let mut memory_release: Option<Value> = None;
        // A pass already submitted before a restart is resumed, never resubmitted.
        let job = if state == "submitted" {
            let job = job.ok_or("Passe soumise sans job")?;
            self.event(run, Some(&id), "resumed_wait", json!({"job": job}))?;
            job
        } else {
            if !confirmed {
                return Err(format!(
                    "Confirmation explicite requise pour exécuter la passe {id} sur le moteur"
                )
                .into());
            }
            let reuse_models: bool = self.connection.query_row(
                "SELECT detail_json FROM pipeline_run_events WHERE run_id=? AND kind='run_created' ORDER BY rowid LIMIT 1",
                [run], |r| r.get::<_,String>(0),
            ).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["reuseModels"].as_bool()).unwrap_or(false);
            let first_step =
                manifest.execution_order()?.first().map(String::as_str) == Some(id.as_str());
            if vram["shouldUnloadBeforePass"] == true
                && !unload_after_pass
                && !(reuse_models && first_step)
            {
                if let Some(manager) = memory {
                    match manager.prepare_pass(&pipeline, &id) {
                        Ok(outcome) => {
                            memory_prepare = Some(serde_json::to_value(&outcome)?);
                            self.event(
                                run,
                                Some(&id),
                                "memory_prepare",
                                json!({"reason":vram,"outcome":memory_prepare}),
                            )?;
                        }
                        Err(error) => {
                            self.event(
                                run,
                                Some(&id),
                                "memory_prepare_failed",
                                json!({"reason":vram,"error":error.to_string()}),
                            )?;
                            return Err(error);
                        }
                    }
                }
            }
            if unload_after_pass {
                if let Some(manager) = memory {
                    match manager.prepare_pass(&pipeline, &id) {
                        Ok(outcome) => {
                            memory_prepare = Some(serde_json::to_value(&outcome)?);
                            self.event(
                                run,
                                Some(&id),
                                "memory_prepare",
                                memory_prepare.clone().unwrap_or(Value::Null),
                            )?;
                        }
                        Err(error) => {
                            self.event(
                                run,
                                Some(&id),
                                "memory_prepare_failed",
                                json!({"error": error.to_string()}),
                            )?;
                            return Err(error);
                        }
                    }
                }
            }
            let fingerprint = self
                .fingerprint(run, &step)?
                .ok_or("Entrées de la passe indisponibles")?;
            let (text,): (String,) = self.connection.query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [&step.workflow_id],
                |r| Ok((r.get(0)?,)),
            )?;
            let mut graph: Value = serde_json::from_str(&text)?;
            for binding in &step.bindings {
                let source = manifest
                    .steps
                    .iter()
                    .find(|s| s.id == binding.source.step)
                    .ok_or("Source de liaison absente")?;
                let node = graph
                    .get(&binding.node_id)
                    .ok_or_else(|| format!("Node cible {} absent du workflow", binding.node_id))?;
                let class = node["class_type"].as_str().unwrap_or_default();
                let (_, source_job, _, sha) = self.step_row(run, binding.source.step.as_str())?;
                let sha = sha.ok_or("Sortie amont absente")?;
                let key = attempt_key(
                    binding.source.step.as_str(),
                    source_job.as_deref().unwrap_or_default(),
                );
                let bytes = self
                    .existing_generation_dir(run)?
                    .as_ref()
                    .and_then(|dir| {
                        generation_store::load_pass(dir, binding.source.step.as_str(), &sha).ok()
                    })
                    .map(Ok)
                    .unwrap_or_else(|| {
                        pass_states::load_state(&root, &store_name(run), &key, &sha).map(|(_, b)| b)
                    })?;
                match class {
                    "LoadImage" if binding.input == "image" && source.kind == PassKind::Image => {
                        let name = client.upload_image(&format!("saya_{}.png", &sha[..16]), &bytes)?;
                        graph[&binding.node_id]["inputs"][&binding.input] = json!(name);
                    }
                    "LoadLatent" if binding.input == "latent" && source.kind == PassKind::Latent => {
                        let name = client.upload_latent(&format!("saya_{}.latent", &sha[..16]), &bytes)?;
                        graph[&binding.node_id]["inputs"][&binding.input] = json!(name);
                    }
                    _ => {
                        return Err(format!(
                            "Liaison incompatible : source {} ({:?}) vers {}.{} ; attendu LoadImage.image ou LoadLatent.latent",
                            source.id.as_str(), source.kind, binding.node_id, binding.input
                        )
                        .into())
                    }
                }
            }
            for (node, input, value) in &plans.get(&id).ok_or("Plan de passe absent")?.writes {
                graph[node]["inputs"][input] = value.clone();
            }
            let info = client.get("/object_info")?;
            let prepared = self.prepare_job_from_graph(
                &step.workflow_id,
                &graph,
                &info,
                json!({"run": run, "step": id, "pipeline": pipeline}),
            )?;
            // Persist ownership before HTTP: a lost acknowledgement must never
            // leave a pending pass that creates another job on restart.
            let claimed = self.connection.execute(
                "UPDATE pipeline_run_steps SET state='submitted',job_id=?3,input_fingerprint=?4,message='',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=?1 AND step_id=?2 AND state=?5 AND job_id IS ?6",
                params![run, id, prepared.job_id, fingerprint, state, job],
            )?;
            if claimed != 1 {
                return Err("Passe déjà prise en charge ; actualisez son suivi".into());
            }
            self.event(
                run,
                Some(&id),
                "submission_requested",
                json!({"job": prepared.job_id}),
            )?;
            if let Err(error) = self.submit_job(
                &prepared.job_id,
                &prepared.snapshot_hash,
                true,
                true,
                client,
            ) {
                self.set_run_state(run, "paused")?;
                self.event(
                    run,
                    Some(&id),
                    "submission_uncertain",
                    json!({"job": prepared.job_id, "message": error.to_string()}),
                )?;
                return Err(error);
            }
            self.set_run_state(run, "running")?;
            self.event(run, Some(&id), "submitted", json!({"job": prepared.job_id}))?;
            prepared.job_id
        };
        let started = Instant::now();
        loop {
            self.sync_jobs(client, results)?;
            let job_state: String = self.connection.query_row(
                "SELECT state FROM generation_jobs WHERE id=?",
                [&job],
                |r| r.get(0),
            )?;
            match job_state.as_str() {
                "succeeded" => break,
                "failed" | "cancelled" | "interrupted" => {
                    self.connection.execute(
                        "UPDATE pipeline_run_steps SET state='failed',message=?3,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=?1 AND step_id=?2",
                        params![run, id, format!("Job {job_state}")],
                    )?;
                    self.set_run_state(run, "failed")?;
                    self.event(
                        run,
                        Some(&id),
                        "failed",
                        json!({"job": job, "state": job_state}),
                    )?;
                    return Ok(json!({"run": run, "step": id, "state": "failed"}));
                }
                _ if started.elapsed() > timeout => {
                    self.set_run_state(run, "paused")?;
                    return Ok(
                        json!({"run": run, "step": id, "state": "submitted", "waiting": true}),
                    );
                }
                _ => std::thread::sleep(Duration::from_millis(500)),
            }
        }
        // A LATENT pass keeps the verified SaveLatent container; an IMAGE pass
        // keeps the first saved PNG. Neither is converted.
        let (bytes, kind) = if step
            .outputs
            .iter()
            .any(|o| o.port_type.as_str() == "LATENT")
        {
            let outputs: String = self.connection.query_row(
                "SELECT outputs_json FROM generation_jobs WHERE id=?",
                [&job],
                |r| r.get(0),
            )?;
            let outputs: Value = serde_json::from_str(&outputs)?;
            let path = outputs
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["kind"] == "latent")
                .and_then(|o| o["path"].as_str())
                .ok_or("Passe LATENT terminée sans fichier SaveLatent")?;
            (std::fs::read(path)?, StateKind::Latent)
        } else {
            let (root_path, relative): (String, String) = self
                .connection
                .query_row(
                    "SELECT l.root_path,a.relative_path FROM job_outputs o JOIN assets a ON a.id=o.asset_id JOIN libraries l ON l.id=a.library_id WHERE o.job_id=? AND o.output_kind='saved' ORDER BY o.rowid LIMIT 1",
                    [&job],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or("Passe terminée sans image enregistrée (SaveImage requis)")?;
            (
                std::fs::read(Path::new(&root_path).join(&relative))?,
                StateKind::Image,
            )
        };
        let entry = pass_states::save_state(
            &root,
            &NewState {
                pipeline: &store_name(run),
                pass: &attempt_key(&id, &job),
                producer_step: &format!("job:{job}"),
                kind,
                retention: Retention::Pinned,
                bytes: &bytes,
                max_state_bytes: MAX_STATE,
            },
        )?;
        pass_states::verify_state(
            &root,
            &store_name(run),
            &attempt_key(&id, &job),
            &entry.sha256,
        )?;
        let kind_name = match kind {
            StateKind::Latent => "latent",
            StateKind::Image => "image",
        };
        let gen_dir = self.generation_dir(run)?;
        let snapshot: String = self.connection.query_row(
            "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
            [&job],
            |r| r.get(0),
        )?;
        let snapshot_value: Value = serde_json::from_str(&snapshot).unwrap_or(Value::Null);
        let request_sha = digest(snapshot.as_bytes());
        let meta = json!({"step":id,"job":job,"kind":kind_name,"workflowId":step.workflow_id,
            "seed":step.seed,"parameters":step.parameters,"requestSha256":request_sha,
            "snapshot":snapshot_value.clone(),
            "provenance":{"pipeline":pipeline,"run":run}});
        let recorded =
            generation_store::record_pass(&gen_dir, &id, kind_name, &bytes, meta.clone());
        if recorded.is_err() {
            // Idempotence after a crash: same pass+sha is accepted; any other
            // content is a conflict and must not be overwritten.
            let manifest: Value =
                serde_json::from_slice(&std::fs::read(gen_dir.join("manifest.json"))?)?;
            let same = manifest["passes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| p["pass"] == id && p["sha256"] == entry_sha(&bytes));
            if !same {
                return Err("Magasin de génération en conflit : passe ou contenu différent".into());
            }
        }
        self.connection.execute(
            "UPDATE pipeline_run_steps SET state='completed',output_kind=?3,output_sha256=?4,output_bytes=?5,message='',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE run_id=?1 AND step_id=?2",
            params![run, id, kind_name, entry.sha256, entry.bytes as i64],
        )?;
        self.event(
            run,
            Some(&id),
            "completed",
            json!({"job": job, "sha256": entry.sha256}),
        )?;
        if unload_after_pass {
            if let Some(manager) = memory {
                match manager.release_after_pass(&pipeline, &id) {
                    Ok(outcome) => {
                        memory_release = Some(serde_json::to_value(&outcome)?);
                        self.event(
                            run,
                            Some(&id),
                            "memory_release",
                            memory_release.clone().unwrap_or(Value::Null),
                        )?;
                    }
                    Err(error) => {
                        memory_release = Some(json!({
                            "backend": manager.backend(),
                            "released": false,
                            "details": error.to_string(),
                        }));
                        self.event(
                            run,
                            Some(&id),
                            "memory_release_failed",
                            json!({"error": error.to_string()}),
                        )?;
                    }
                }
            }
        }
        let remaining: i64 = self.connection.query_row(
            "SELECT count(*) FROM pipeline_run_steps WHERE run_id=? AND state!='completed'",
            [run],
            |r| r.get(0),
        )?;
        self.set_run_state(
            run,
            if remaining == 0 {
                "completed"
            } else {
                "running"
            },
        )?;
        let mut published = Value::Null;
        if remaining == 0 && kind_name == "image" {
            let prefs = self.preferences()?;
            if let Some(root) = prefs["generationRoot"]
                .as_str()
                .filter(|p| !p.trim().is_empty())
            {
                let (manifest, _) = self.load_pipeline(&pipeline)?;
                let created: String = self.connection.query_row(
                    "SELECT created_at FROM pipeline_runs WHERE id=?",
                    [run],
                    |r| r.get(0),
                )?;
                let stamp: String = created
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == 'T' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let path = generation_storage::publish_final(
                    Path::new(root),
                    &manifest.name,
                    &stamp,
                    &bytes,
                )?;
                published = json!(path.to_string_lossy());
            }
        }
        Ok(
            json!({"run": run, "step": id, "state": "completed", "sha256": entry.sha256, "done": remaining == 0,
                "memory": {"prepare": memory_prepare, "release": memory_release}, "vramPolicy": vram, "published": published}),
        )
    }
}
