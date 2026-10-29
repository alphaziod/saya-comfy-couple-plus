//! Pre-architecture of multi-pass pipelines assembling ComfyUI API workflows (A → B → C).
//!
//! This module only defines types, a versioned manifest (v1) and
//! pure validations. It never merges the workflows' JSON graphs
//! (no graph concatenation), orchestrates no execution and makes
//! no network call: each step stays a distinct API workflow referenced
//! by its revision identifier. The actual orchestration, ComfyUI
//! submission and integration with custom nodes are built on top of these
//! contracts.
//!
//! Crate convention kept: u64 seeds travel as exact decimal JSON text
//! ([`Seed`]), never as f64.

use crate::contracts::Seed;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Format identifier of the pipeline manifest.
pub const PIPELINE_FORMAT: &str = "saya-workflow-pipeline";
/// Historical version kept for v1 producers.
pub const PIPELINE_VERSION: u32 = 1;
pub const PIPELINE_MULTILINE_VERSION: u32 = 2;

const MAX_STEPS: usize = 64;
const MAX_DECLARED_WORKFLOWS: usize = 64;
const MAX_PORTS_PER_STEP: usize = 64;
const MAX_PARAMETERS_PER_STEP: usize = 64;
const MAX_NAME: usize = 300;
const MAX_ID: usize = 120;
const MAX_DESCRIPTION: usize = 4000;
const MAX_TAGS: usize = 32;
const MAX_TAG: usize = 64;
const MAX_PATH: usize = 1024;
const MAX_PARAMETER_VALUE: usize = 200_000;
const MAX_NOTE: usize = 2000;
const MAX_PORT_TYPE: usize = 64;
const MAX_SOURCE_SHA256: usize = 64;

/// ComfyUI port type as declared by the engine (IMAGE, LATENT, MODEL,
/// CLIP, VAE, CONDITIONING…). The exact value travels without conversion; "*"
/// keeps its wildcard meaning as already applied by `validate_graph`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PortType(String);

impl PortType {
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        if value.is_empty() || value.len() > MAX_PORT_TYPE {
            return Err("Type de port vide ou trop long".into());
        }
        if value != "*"
            && !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(format!("Type de port invalide : {value}"));
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compatibility in the engine's sense: exact equality or the "*" wildcard.
    /// IMAGE→IMAGE and LATENT→LATENT are accepted, any other crossing is refused.
    pub fn accepts(&self, provided: &PortType) -> bool {
        self.0 == "*" || provided.0 == "*" || self.0 == provided.0
    }
}

impl TryFrom<String> for PortType {
    type Error = String;
    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<PortType> for String {
    fn from(value: PortType) -> Self {
        value.0
    }
}

impl fmt::Display for PortType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable step identifier, chosen by the pipeline author and kept
/// across manifest versions (never an identifier generated on the fly).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StepId(String);

impl StepId {
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        if value.is_empty() || value.len() > MAX_ID {
            return Err("Identifiant d'étape vide ou trop long".into());
        }
        let mut chars = value.chars();
        if !chars.next().is_some_and(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("Identifiant d'étape invalide : {value}"));
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(format!("Identifiant d'étape invalide : {value}"));
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for StepId {
    type Error = String;
    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<StepId> for String {
    fn from(value: StepId) -> Self {
        value.0
    }
}

impl fmt::Display for StepId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Typed port exposed by a step (input to feed or output produced).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortSpec {
    pub name: String,
    pub port_type: PortType,
}

/// Reference to the output of an upstream step of the same pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRef {
    pub step: StepId,
    pub port: String,
}

/// Data binding: output of an upstream step → input of a target step,
/// with the exact node and input of the target workflow's API graph.
/// No graph modification is described here, only the flow contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputBinding {
    /// Declared input port of the targeted step, fed by this binding.
    pub port: String,
    /// Target node in the step workflow's API graph.
    pub node_id: String,
    /// Input of the target node receiving the data.
    pub input: String,
    /// Upstream output providing the data.
    pub source: SourceRef,
}

/// Nature of the pass and of its intermediate boundary. IMAGE and LATENT are
/// supported; a new variant is a format evolution that this v1
/// reader will explicitly refuse rather than interpret blindly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassKind {
    Image,
    Latent,
}

impl PassKind {
    /// Expected port type for the intermediate state of this pass.
    pub fn boundary_type(self) -> PortType {
        match self {
            Self::Image => PortType("IMAGE".into()),
            Self::Latent => PortType("LATENT".into()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Latent => "latent",
        }
    }
}

/// Explicit status of a pass. No transition is deduced here: only
/// the future orchestration writes these statuses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassStatus {
    #[default]
    Pending,
    Prepared,
    Submitted,
    Completed,
    Failed,
    Skipped,
}

/// Explicit resume conditions of a pass, without execution logic:
/// the future orchestrator reads these fields and remains responsible for the
/// real checks (presence, integrity, compatibility of the intermediate state).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeConditions {
    /// The pass's intermediate state must exist and be verified.
    pub requires_state: bool,
    /// All upstream steps must be completed before a resume.
    pub requires_upstream_completed: bool,
    /// Statuses from which resuming this pass remains allowed.
    #[serde(default)]
    pub from_statuses: Vec<PassStatus>,
    /// Explicit human note, with no execution effect.
    #[serde(default)]
    pub note: Option<String>,
}

/// Declared intermediate state of a pass: relative path and type of the carried
/// data. No file read, write or check is done here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntermediateState {
    pub path: String,
    pub port_type: PortType,
}

/// A pipeline step: a source API workflow, its typed ports, its
/// bindings, its parameters, its exact seed and its pass state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub id: StepId,
    /// Workflow revision identifier, present in `workflows`.
    pub workflow_id: String,
    pub kind: PassKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub inputs: Vec<PortSpec>,
    #[serde(default)]
    pub outputs: Vec<PortSpec>,
    #[serde(default)]
    pub bindings: Vec<InputBinding>,
    /// Exact pass seed: u64 as canonical decimal text, following the
    /// crate's raw JSON convention (never f64).
    #[serde(default)]
    pub seed: Option<Seed>,
    /// Explicit named parameters of the pass (section texts, adapter
    /// settings…), interpreted only by the future orchestration.
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
    #[serde(default)]
    pub status: PassStatus,
    #[serde(default)]
    pub state: Option<IntermediateState>,
    #[serde(default)]
    pub resume: Option<ResumeConditions>,
}

/// Source API workflow declared by the pipeline. Presence in this list
/// is the reference validated at this level; actual presence in the database and
/// source hash matching belong to the future integration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowRef {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Optional fingerprint pinning the exact revision of the source document.
    #[serde(default)]
    pub source_sha256: Option<String>,
}

/// Complete versioned pipeline: source workflows, steps and bindings forming
/// an acyclic DAG of passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowPipeline {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub workflows: Vec<WorkflowRef>,
    /// Pipeline steps; the list order does not set the execution
    /// order, which is deduced from the bindings DAG.
    pub steps: Vec<Step>,
}

fn check_text(field: &str, value: &str, max: usize) -> Result<()> {
    if value.is_empty() {
        return Err(format!("{field} est requis").into());
    }
    if value.len() > max {
        return Err(format!("{field} dépasse {max} caractères").into());
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} contient un caractère de contrôle").into());
    }
    Ok(())
}

fn check_optional_text(field: &str, value: &Option<String>, max: usize) -> Result<()> {
    if let Some(value) = value {
        check_text(field, value, max)?;
    }
    Ok(())
}

fn check_relative_path(path: &str) -> Result<()> {
    check_text("Chemin d'état intermédiaire", path, MAX_PATH)?;
    if path.starts_with(['/', '\\']) {
        return Err("Chemin d'état intermédiaire absolu refusé".into());
    }
    if path
        .split(['/', '\\'])
        .any(|part| part == ".." || part.contains(':'))
    {
        return Err("Chemin d'état intermédiaire hors du dossier pipeline refusé".into());
    }
    Ok(())
}

fn check_ports(ports: &[PortSpec], step: &StepId, kind: &str) -> Result<()> {
    if ports.len() > MAX_PORTS_PER_STEP {
        return Err(format!(
            "Trop de ports de {kind} pour l'étape {step} (maximum {MAX_PORTS_PER_STEP})"
        )
        .into());
    }
    let mut seen = BTreeSet::new();
    for port in ports {
        check_text(
            &format!("Nom du port de {kind} de l'étape {step}"),
            &port.name,
            MAX_ID,
        )?;
        if !seen.insert(port.name.as_str()) {
            return Err(format!("Port de {kind} dupliqué {port:?} sur l'étape {step}").into());
        }
    }
    Ok(())
}

impl WorkflowPipeline {
    /// Reads a JSON manifest: format/version then full validation.
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 16_000_000 {
            return Err("Manifeste de pipeline supérieur à 16 Mo".into());
        }
        let pipeline: Self = serde_json::from_str(text)
            .map_err(|e| format!("Manifeste de pipeline invalide : {e}"))?;
        pipeline.validate()?;
        Ok(pipeline)
    }

    /// Stable JSON serialization of the manifest.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| format!("Pipeline non sérialisable : {e}").into())
    }

    /// Execution order of the steps deduced from the bindings DAG (Kahn,
    /// deterministic via ordered collections). Modifies nothing.
    pub fn execution_order(&self) -> Result<Vec<String>> {
        let known: BTreeSet<&str> = self.steps.iter().map(|s| s.id.as_str()).collect();
        let mut indegree: BTreeMap<&str, usize> =
            self.steps.iter().map(|s| (s.id.as_str(), 0)).collect();
        let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for step in &self.steps {
            for binding in &step.bindings {
                if !known.contains(binding.source.step.as_str()) {
                    return Err(format!(
                        "Liaison de l'étape {} vers une étape source inconnue : {}",
                        step.id, binding.source.step
                    )
                    .into());
                }
                *indegree.entry(step.id.as_str()).or_default() += 1;
                dependents
                    .entry(binding.source.step.as_str())
                    .or_default()
                    .push(step.id.as_str());
            }
        }
        let mut ready: BTreeSet<&str> = indegree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut order = Vec::with_capacity(self.steps.len());
        while let Some(id) = ready.pop_first() {
            for dependent in dependents.get(id).into_iter().flatten() {
                let degree = indegree
                    .get_mut(dependent)
                    .ok_or("Dépendance du pipeline incohérente")?;
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(dependent);
                }
            }
            order.push(id.to_owned());
        }
        if order.len() != self.steps.len() {
            let ordered: BTreeSet<&str> = order.iter().map(String::as_str).collect();
            let remaining: Vec<&str> = known.difference(&ordered).copied().collect();
            return Err(format!(
                "Cycle détecté dans le pipeline ; étapes non ordonnables : {}",
                remaining.join(", ")
            )
            .into());
        }
        Ok(order)
    }

    /// Full validation of references, types, uniqueness and acyclicity.
    pub fn validate(&self) -> Result<()> {
        if self.format != PIPELINE_FORMAT
            || !matches!(self.version, PIPELINE_VERSION | PIPELINE_MULTILINE_VERSION)
        {
            return Err(format!(
                "Format de pipeline inconnu : {}/{} ; attendu {PIPELINE_FORMAT}/1 ou /2",
                self.format, self.version
            )
            .into());
        }
        check_text("Identifiant du pipeline", &self.id, MAX_ID)?;
        check_text("Nom du pipeline", &self.name, MAX_NAME)?;
        check_optional_text(
            "Description du pipeline",
            &self.description,
            MAX_DESCRIPTION,
        )?;
        if self.tags.len() > MAX_TAGS {
            return Err(format!("Trop de tags (maximum {MAX_TAGS})").into());
        }
        for tag in &self.tags {
            check_text("Tag du pipeline", tag, MAX_TAG)?;
        }
        if self.workflows.len() > MAX_DECLARED_WORKFLOWS {
            return Err(
                format!("Trop de workflows déclarés (maximum {MAX_DECLARED_WORKFLOWS})").into(),
            );
        }
        let mut workflow_ids = BTreeSet::new();
        for workflow in &self.workflows {
            check_text("Identifiant de workflow déclaré", &workflow.id, MAX_ID)?;
            if !workflow_ids.insert(workflow.id.as_str()) {
                return Err(format!("Workflow déclaré deux fois : {}", workflow.id).into());
            }
            check_optional_text(
                &format!("Nom du workflow {}", workflow.id),
                &workflow.name,
                MAX_NAME,
            )?;
            if let Some(hash) = &workflow.source_sha256 {
                if hash.len() != MAX_SOURCE_SHA256
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(format!(
                        "Empreinte source invalide pour le workflow {} : hexadécimal minuscule de 64 caractères attendu",
                        workflow.id
                    )
                    .into());
                }
            }
        }
        if self.steps.is_empty() {
            return Err("Un pipeline exige au moins une étape".into());
        }
        if self.steps.len() > MAX_STEPS {
            return Err(format!("Trop d'étapes (maximum {MAX_STEPS})").into());
        }
        let mut step_ids = BTreeSet::new();
        for step in &self.steps {
            if !step_ids.insert(step.id.as_str()) {
                return Err(format!("Identifiant d'étape déjà utilisé : {}", step.id).into());
            }
        }
        for step in &self.steps {
            self.validate_step(step)?;
        }
        self.execution_order()?;
        Ok(())
    }

    fn validate_step(&self, step: &Step) -> Result<()> {
        check_text(
            &format!("Identifiant de workflow de l'étape {}", step.id),
            &step.workflow_id,
            MAX_ID,
        )?;
        if !self.workflows.iter().any(|w| w.id == step.workflow_id) {
            return Err(format!(
                "Étape {} : workflow {} absent de la liste déclarée",
                step.id, step.workflow_id
            )
            .into());
        }
        check_optional_text(&format!("Nom de l'étape {}", step.id), &step.name, MAX_NAME)?;
        check_ports(&step.inputs, &step.id, "entrée")?;
        check_ports(&step.outputs, &step.id, "sortie")?;
        let mut bound = BTreeSet::new();
        for binding in &step.bindings {
            let declared = step
                .inputs
                .iter()
                .find(|port| port.name == binding.port)
                .ok_or_else(|| {
                    format!(
                        "Étape {} : liaison vers un port d'entrée non déclaré : {}",
                        step.id, binding.port
                    )
                })?;
            if !bound.insert(binding.port.as_str()) {
                return Err(format!(
                    "Étape {} : port d'entrée lié deux fois : {}",
                    step.id, binding.port
                )
                .into());
            }
            check_text(
                &format!("Node cible de l'étape {}", step.id),
                &binding.node_id,
                MAX_ID,
            )?;
            check_text(
                &format!("Entrée cible de l'étape {}", step.id),
                &binding.input,
                MAX_ID,
            )?;
            let source = self
                .steps
                .iter()
                .find(|candidate| candidate.id == binding.source.step)
                .ok_or_else(|| {
                    format!(
                        "Étape {} : étape source inconnue : {}",
                        step.id, binding.source.step
                    )
                })?;
            let provided = source
                .outputs
                .iter()
                .find(|port| port.name == binding.source.port)
                .ok_or_else(|| {
                    format!(
                        "Étape {} : la source {} n'expose aucun port de sortie : {}",
                        step.id, binding.source.step, binding.source.port
                    )
                })?;
            if !declared.port_type.accepts(&provided.port_type) {
                return Err(format!(
                    "Types de ports incompatibles : {}:{} attend {}, reçu {} de {}:{}",
                    step.id,
                    binding.port,
                    declared.port_type,
                    provided.port_type,
                    binding.source.step,
                    binding.source.port
                )
                .into());
            }
        }
        for port in &step.inputs {
            if !bound.contains(port.name.as_str()) {
                return Err(format!(
                    "Étape {} : port d'entrée non alimenté : {}",
                    step.id, port.name
                )
                .into());
            }
        }
        if step.parameters.len() > MAX_PARAMETERS_PER_STEP {
            return Err(format!(
                "Trop de paramètres pour l'étape {} (maximum {MAX_PARAMETERS_PER_STEP})",
                step.id
            )
            .into());
        }
        for (key, value) in &step.parameters {
            check_text(
                &format!("Clé de paramètre de l'étape {}", step.id),
                key,
                MAX_ID,
            )?;
            if self.version == PIPELINE_MULTILINE_VERSION {
                if value.len() > MAX_PARAMETER_VALUE
                    || value
                        .chars()
                        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                {
                    return Err(format!(
                        "Valeur du paramètre {key} trop longue ou caractère de contrôle interdit"
                    )
                    .into());
                }
            } else {
                check_text(
                    &format!("Valeur du paramètre {key} de l'étape {}", step.id),
                    value,
                    MAX_PARAMETER_VALUE,
                )?;
            }
        }
        if let Some(state) = &step.state {
            check_relative_path(&state.path)?;
            if state.port_type != step.kind.boundary_type() {
                return Err(format!(
                    "État intermédiaire de l'étape {} incohérent : passe de type {}, état de type {}",
                    step.id,
                    step.kind.as_str(),
                    state.port_type
                )
                .into());
            }
        }
        if let Some(resume) = &step.resume {
            if resume.from_statuses.len() > 6 {
                return Err(format!("Trop de statuts de reprise pour l'étape {}", step.id).into());
            }
            check_optional_text(
                &format!("Note de reprise de l'étape {}", step.id),
                &resume.note,
                MAX_NOTE,
            )?;
        }
        Ok(())
    }
}
