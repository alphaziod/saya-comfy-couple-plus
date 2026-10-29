//! Scene composer: persisted manifests resolved against the runner's
//! explicitly registered mappings. The composer invents no
//! connection, modifies no graph and creates no mode: it produces
//! exactly the edits the existing Couple adapter validates, then the
//! same `prepare` inputs the V5 runner consumes. Engine submission
//! stays outside this module.

use crate::{
    couple::{CoupleInput, SplitEdit},
    workflow::{Edit, Field, JobCharacter, Workflow},
    Database, Result,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Maximum stored size of one serialized manifest (identity and scene texts are
/// themselves bounded by the runner's 200 000-char field limit).
const DOCUMENT_LIMIT: usize = 2_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Composition {
    Solo,
    Couple,
}

/// Explicit normalized character zone: x/y/width/height in 0..=1, feather in
/// 0..=1. It is composition metadata consumed by the preview; no workflow node
/// is created or rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneZone {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub feather: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub z: i32,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenePerson {
    pub character_id: String,
    #[serde(default)]
    pub variant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<SceneZone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SceneTextSource {
    /// Keep the text already present in the mapped workflow input.
    Workflow,
    /// Take the section registered in the referenced person's variant preset.
    Preset,
    /// Exact custom text carried by the manifest.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneText {
    pub source: SceneTextSource,
    /// Which scene person's variant provides the block; required for `preset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Exact custom text; an empty custom text leaves the mapped input untouched.
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grok_source: Option<GrokSceneSource>,
    pub workflow_id: String,
    pub composition: Composition,
    pub p1: ScenePerson,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2: Option<ScenePerson>,
    pub action: SceneText,
    pub bg: SceneText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<SceneText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative: Option<SceneText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<SplitEdit>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrokSceneSource {
    pub scene_id: String,
    pub variant_index: usize,
}
/// Profile derived exactly like `workflows()` and `couple_inspection` do:
/// only saved P1/P2 bindings decide, never the node count or a name.
fn prompt_profile(fields: &[Field]) -> &'static str {
    let mapped = |section: &str| {
        fields
            .iter()
            .any(|f| f.enabled && f.section.as_deref() == Some(section))
    };
    match (mapped("P1"), mapped("P2")) {
        (true, true) => "couple",
        (true, false) => "solo",
        _ => "unconfigured",
    }
}

/// The single string input mapped to a section, chosen like `couple_edits` does.
fn mapped_field<'a>(fields: &'a [Field], section: &str) -> Option<&'a Field> {
    fields
        .iter()
        .find(|f| f.enabled && f.value_type == "string" && f.section.as_deref() == Some(section))
}

/// Identity text resolved like the runner's preset loader: the variant's exact
/// `P1` block when a variant is selected, otherwise the sheet's exact prompt.
fn identity_text(snapshot: &Value, role: &str) -> Result<String> {
    let variant = &snapshot["variant"];
    let text = if variant.is_null() {
        snapshot["identity"]
            .get("identity_prompt")
            .or_else(|| snapshot["identity"].get("identityPrompt"))
            .and_then(Value::as_str)
    } else {
        variant["promptBlocks"].get("P1").and_then(Value::as_str)
    };
    text.filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("Identité texte absente pour {role} : la variante doit fournir sa section P1")
                .into()
        })
}

fn variant_block(snapshot: &Value, section: &str) -> Result<String> {
    let role = snapshot["role"].as_str().unwrap_or("?");
    let variant = &snapshot["variant"];
    if variant.is_null() {
        return Err(format!(
            "Source preset pour {section} : {role} n'a aucune variante sélectionnée"
        )
        .into());
    }
    match variant["promptBlocks"].get(section).and_then(Value::as_str) {
        Some(text) => Ok(text.to_owned()),
        None => Err(format!(
            "La variante de {role} n'enregistre pas de section {section} ; aucun bloc inventé"
        )
        .into()),
    }
}

/// Final text of one shared section, or `None` when the scene leaves the mapped
/// input untouched. Refusals carry an explicit diagnostic instead of a guess.
fn resolve_section(
    section: &'static str,
    spec: Option<&SceneText>,
    fields: &[Field],
    snapshots: &[Value],
    solo: bool,
) -> Result<Option<String>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    let text = match spec.source {
        SceneTextSource::Workflow => {
            return Ok(mapped_field(fields, section).map(|f| f.value.clone()));
        }
        SceneTextSource::Custom => {
            if spec.text.is_empty() {
                return Ok(None);
            }
            spec.text.clone()
        }
        SceneTextSource::Preset => {
            let role = spec.from.as_deref().ok_or_else(|| {
                format!("Source preset pour {section} : préciser le rôle (P1 ou P2) dont la variante fournit la section")
            })?;
            if role != "P1" && role != "P2" {
                return Err(format!("Rôle de preset inconnu pour {section} : {role}").into());
            }
            if solo && role != "P1" {
                return Err(format!(
                    "Composition SOLO : la source preset de {section} ne peut venir que de P1"
                )
                .into());
            }
            let snapshot = snapshots
                .iter()
                .find(|s| s["role"].as_str() == Some(role))
                .ok_or_else(|| format!("Rôle absent du manifeste : {role}"))?;
            variant_block(snapshot, section)?
        }
    };
    Ok(Some(text))
}

struct ResolvedScene {
    workflow: Workflow,
    profile: &'static str,
    input: CoupleInput,
    people: Vec<JobCharacter>,
    snapshots: Vec<Value>,
}

fn zones_overlap(a: &SceneZone, b: &SceneZone) -> bool {
    a.enabled
        && b.enabled
        && a.x < b.x + b.width
        && b.x < a.x + a.width
        && a.y < b.y + b.height
        && b.y < a.y + a.height
}

fn zone_overlaps(manifest: &SceneManifest) -> Vec<Value> {
    let Some(p2) = manifest.p2.as_ref().and_then(|p| p.zone.as_ref()) else {
        return Vec::new();
    };
    let Some(p1) = manifest.p1.zone.as_ref() else {
        return Vec::new();
    };
    if zones_overlap(p1, p2) {
        vec![json!({
            "a": "P1", "b": "P2",
            "aZ": p1.z, "bZ": p2.z,
            "visible": if p1.z >= p2.z { "P1" } else { "P2" },
            "area": ((p1.x + p1.width).min(p2.x + p2.width) - p1.x.max(p2.x))
                * ((p1.y + p1.height).min(p2.y + p2.height) - p1.y.max(p2.y)),
        })]
    } else {
        Vec::new()
    }
}

fn validate_zone(role: &str, zone: Option<&SceneZone>) -> Result<()> {
    let Some(zone) = zone else { return Ok(()) };
    let finite = zone.x.is_finite()
        && zone.y.is_finite()
        && zone.width.is_finite()
        && zone.height.is_finite()
        && zone.feather.is_finite();
    if !finite
        || zone.x < 0.0
        || zone.y < 0.0
        || zone.width <= 0.0
        || zone.height <= 0.0
        || zone.x + zone.width > 1.000_001
        || zone.y + zone.height > 1.000_001
        || zone.feather < 0.0
        || zone.feather > 1.0
        || !(-1000..=1000).contains(&zone.z)
    {
        return Err(format!(
            "Zone {role} invalide : coordonnées normalisées 0..1, largeur/hauteur > 0, feather 0..1"
        )
        .into());
    }
    Ok(())
}

fn validate_zones(manifest: &SceneManifest) -> Result<()> {
    validate_zone("P1", manifest.p1.zone.as_ref())?;
    validate_zone(
        "P2",
        manifest.p2.as_ref().and_then(|person| person.zone.as_ref()),
    )
}

impl Database {
    fn resolve_scene(&self, manifest: &SceneManifest) -> Result<ResolvedScene> {
        validate_zones(manifest)?;
        if let Some(source) = &manifest.grok_source {
            let record = self.grok_scene(&source.scene_id)?;
            if record["variants"]
                .as_array()
                .and_then(|v| v.get(source.variant_index))
                .is_none()
            {
                return Err("Variante Grok source introuvable".into());
            }
            for value in [&manifest.action, &manifest.bg]
                .into_iter()
                .chain(manifest.quality.iter())
                .chain(manifest.negative.iter())
            {
                if value.source == SceneTextSource::Custom && value.text.contains("{accent}") {
                    return Err(
                        "Variable Grok {accent} non résolue : renseignez le texte final".into(),
                    );
                }
            }
        }

        if manifest.workflow_id.trim().is_empty() {
            return Err("Workflow absent du manifeste".into());
        }
        let workflow = self.workflow(&manifest.workflow_id)?;
        let profile = prompt_profile(&workflow.fields);
        if profile == "unconfigured" {
            return Err(
                "Workflow sans mapping P1 enregistré : le compositeur n'utilise que des mappings vérifiés et refuse d'en inventer"
                    .into(),
            );
        }
        match (manifest.composition, profile) {
            (Composition::Solo, "couple") => {
                return Err(
                    "Workflow de profil COUPLE (P1 et P2 mappés) : la composition SOLO n'a pas de mapping vérifié ; choisissez COUPLE ou un workflow solo"
                        .into(),
                );
            }
            (Composition::Couple, "solo") => {
                return Err(
                    "Workflow de profil SOLO : P2 n'est pas mappé ; le compositeur ne crée aucun mode interne"
                        .into(),
                );
            }
            _ => {}
        }
        let couple = manifest.composition == Composition::Couple;
        if !couple && manifest.p2.is_some() {
            return Err(
                "Composition SOLO : le second personnage (P2) est refusé ; un seul identité mappée"
                    .into(),
            );
        }
        if couple && manifest.p2.is_none() {
            return Err(
                "Composition COUPLE : un second personnage (P2) est requis par le contrat existant (chaque identité mappée exige un prompt non vide)"
                    .into(),
            );
        }
        let solo = !couple;
        let mut people = vec![JobCharacter {
            role: "P1".into(),
            character_id: manifest.p1.character_id.trim().to_owned(),
            variant_id: manifest.p1.variant_id.clone(),
        }];
        if let Some(p2) = &manifest.p2 {
            people.push(JobCharacter {
                role: "P2".into(),
                character_id: p2.character_id.trim().to_owned(),
                variant_id: p2.variant_id.clone(),
            });
        }
        for person in &people {
            if person.character_id.is_empty() {
                return Err(format!("Personnage {} absent du manifeste", person.role).into());
            }
        }
        // Validates that each role has a saved mapping and freezes identities/revisions.
        let snapshots = self.snapshot_characters(&manifest.workflow_id, &people)?;
        let snapshot = |role: &str| -> Result<&Value> {
            snapshots
                .iter()
                .find(|s| s["role"].as_str() == Some(role))
                .ok_or_else(|| format!("Rôle absent du snapshot : {role}").into())
        };
        let p1_text = identity_text(snapshot("P1")?, "P1")?;
        let p2_text = couple
            .then(|| identity_text(snapshot("P2")?, "P2"))
            .transpose()?;
        let sections: Vec<(&'static str, Option<String>)> = [
            ("ACTION", Some(&manifest.action)),
            ("BG", Some(&manifest.bg)),
            ("QUALITY", manifest.quality.as_ref()),
            ("NEG", manifest.negative.as_ref()),
        ]
        .into_iter()
        .map(|(section, spec)| {
            resolve_section(section, spec, &workflow.fields, &snapshots, solo)
                .map(|text| (section, text))
        })
        .collect::<Result<_>>()?;
        // A requested section without a saved mapping is refused with a diagnostic;
        // couple_edits re-checks this contract independently.
        for (section, text) in &sections {
            if text.is_some() && mapped_field(&workflow.fields, section).is_none() {
                return Err(format!(
                    "Section {section} demandée par la scène sans mapping enregistré ; aucun changement implicite"
                )
                .into());
            }
        }
        // Final text per section; `None` when the mapped input is left untouched.
        let current = |section: &str| -> String {
            mapped_field(&workflow.fields, section)
                .map(|f| f.value.clone())
                .unwrap_or_default()
        };
        let wanted = |section: &'static str| -> String {
            sections
                .iter()
                .find(|(name, _)| *name == section)
                .and_then(|(_, text)| text.clone())
                .unwrap_or_else(|| current(section))
        };
        let input = CoupleInput {
            characters: people.clone(),
            p1: p1_text,
            p2: p2_text.unwrap_or_default(),
            action: wanted("ACTION"),
            bg: wanted("BG"),
            quality: manifest
                .quality
                .as_ref()
                .map(|_| wanted("QUALITY"))
                .filter(|_| mapped_field(&workflow.fields, "QUALITY").is_some()),
            negative: manifest
                .negative
                .as_ref()
                .map(|_| wanted("NEG"))
                .filter(|_| mapped_field(&workflow.fields, "NEG").is_some()),
            mask: manifest.mask.clone(),
        };
        Ok(ResolvedScene {
            workflow,
            profile,
            input,
            people,
            snapshots,
        })
    }

    /// Edits and character associations for the existing runner entry point.
    /// Nothing is written: `prepare` remains a separate explicit step.
    pub fn scene_edits(&self, manifest: &SceneManifest) -> Result<(Vec<Edit>, Vec<JobCharacter>)> {
        let resolved = self.resolve_scene(manifest)?;
        let edits = self.couple_edits(&manifest.workflow_id, &resolved.input)?;
        Ok((edits, resolved.people))
    }

    /// Read-only assembly of the complete prompts and every planned change.
    /// The workflow document and the job table are never touched here.
    pub fn preview_scene(&self, manifest: &SceneManifest) -> Result<Value> {
        let resolved = self.resolve_scene(manifest)?;
        let edits = self.couple_edits(&manifest.workflow_id, &resolved.input)?;
        let mut changes = Vec::new();
        for edit in &edits {
            let before = resolved.workflow.fields.iter().find(|f| {
                f.enabled && f.node_id == edit.node_id && f.input_name == edit.input_name
            });
            if let Some(field) = before {
                if field.value != edit.value {
                    changes.push(json!({
                        "section": edit.section,
                        "nodeId": edit.node_id,
                        "inputName": edit.input_name,
                        "before": field.value,
                        "after": edit.value,
                    }));
                }
            }
        }
        let mut prompts = Map::new();
        for section in ["P1", "P2", "ACTION", "BG", "QUALITY", "NEG"] {
            // Only mapped sections appear; the full assembled text comes from the
            // actual edits, so the preview cannot drift from what would be written.
            let Some(field) = mapped_field(&resolved.workflow.fields, section) else {
                continue;
            };
            let edit = edits.iter().find(|e| e.section.as_deref() == Some(section));
            prompts.insert(
                section.into(),
                json!({
                    "nodeId": field.node_id,
                    "inputName": field.input_name,
                    "before": field.value,
                    "text": edit.map(|e| e.value.clone()).unwrap_or_else(|| field.value.clone()),
                    "changed": edit.is_some_and(|e| e.value != field.value),
                }),
            );
        }
        Ok(json!({
            "workflow": {
                "id": resolved.workflow.id,
                "name": resolved.workflow.name,
                "promptProfile": resolved.profile,
            },
            "composition": manifest.composition,
            "characters": resolved.snapshots,
            "jobCharacters": resolved.people.iter().map(|p| json!({
                "role": p.role, "characterId": p.character_id, "variantId": p.variant_id,
            })).collect::<Vec<_>>(),
            "prompts": prompts,
            "edits": edits,
            "changes": changes,
            "zones": {
                "P1": manifest.p1.zone,
                "P2": manifest.p2.as_ref().and_then(|p| p.zone),
            },
            "zoneOrder": {
                "P1": manifest.p1.zone.as_ref().map(|z| z.z),
                "P2": manifest.p2.as_ref().and_then(|p| p.zone.as_ref()).map(|z| z.z),
            },
            "overlaps": zone_overlaps(manifest),
            "mask": manifest.mask,
            "warnings": [],
            "scope": "Aperçu calculé depuis les mappings enregistrés uniquement. Aucun graphe modifié, aucun job créé, aucune connexion inventée.",
        }))
    }

    pub fn scenes(&self) -> Result<Vec<Value>> {
        let mut query = self.connection.prepare(
            "SELECT id,name,document_json,created_at,updated_at FROM scene_manifests ORDER BY updated_at DESC, rowid DESC",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, name, document, created_at, updated_at) = row?;
            let manifest: Value = serde_json::from_str(&document)?;
            result.push(json!({
                "id": id,
                "name": name,
                "composition": manifest["composition"],
                "workflowId": manifest["workflowId"],
                "createdAt": created_at,
                "updatedAt": updated_at,
            }));
        }
        Ok(result)
    }

    pub fn scene(&self, id: &str) -> Result<Value> {
        let (name, document, created_at, updated_at): (String, String, String, String) = self
            .connection
            .query_row(
                "SELECT name,document_json,created_at,updated_at FROM scene_manifests WHERE id=?",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|_| "Scène introuvable")?;
        // Round-trip through the typed contract so stored documents stay honest.
        let manifest: SceneManifest = serde_json::from_str(&document)?;
        Ok(json!({
            "id": id,
            "name": name,
            "manifest": manifest,
            "createdAt": created_at,
            "updatedAt": updated_at,
        }))
    }

    pub fn save_scene(
        &self,
        id: Option<&str>,
        name: &str,
        manifest: &SceneManifest,
    ) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 300 {
            return Err("Nom de scène invalide".into());
        }
        if manifest.workflow_id.trim().is_empty() {
            return Err("Workflow absent du manifeste".into());
        }
        validate_zones(manifest)?;
        let document = serde_json::to_string(manifest)?;
        if document.len() > DOCUMENT_LIMIT {
            return Err("Manifeste de scène trop volumineux".into());
        }
        if let Some(id) = id {
            let changed = self.connection.execute(
                "UPDATE scene_manifests SET name=?2,document_json=?3,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
                params![id, name, document],
            )?;
            if changed != 1 {
                return Err("Scène introuvable".into());
            }
            return Ok(id.to_owned());
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO scene_manifests(id,name,document_json) VALUES(?1,?2,?3)",
            params![id, name, document],
        )?;
        Ok(id)
    }

    /// Explicit copy: modifications happen on a copy, never by silent overwrite.
    pub fn duplicate_scene(&self, id: &str) -> Result<String> {
        let scene = self.scene(id)?;
        let mut name = format!("{} (copie)", scene["name"].as_str().unwrap_or("Scène"));
        if name.len() > 300 {
            name.truncate(300);
        }
        let manifest: SceneManifest = serde_json::from_value(scene["manifest"].clone())?;
        let new_id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO scene_manifests(id,name,document_json) VALUES(?1,?2,?3)",
            params![new_id, name, serde_json::to_string(&manifest)?],
        )?;
        Ok(new_id)
    }

    pub fn delete_scene(&self, id: &str) -> Result<()> {
        if self
            .connection
            .execute("DELETE FROM scene_manifests WHERE id=?", [id])?
            != 1
        {
            return Err("Scène introuvable".into());
        }
        Ok(())
    }
}
