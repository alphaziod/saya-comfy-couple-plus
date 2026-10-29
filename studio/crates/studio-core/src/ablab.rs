//! A/B laboratory: persistent experiment campaigns, preparation only.
//!
//! Nothing in this module submits a generation, calls ComfyUI or starts a server.
//! A campaign freezes a reference workflow (+ name and source fingerprint), a reference
//! configuration and an explicit split between CONSTANT parameters and VARIANT parameters.
//! Preparing the grid materialises the bounded cartesian product as cells; the future
//! execution loop is an external contract (see docs/zcode-handoff/ab-lab.md).
//! Cell images are references to existing provenance rows (job/node/index), never copies.
//! Verdicts are strictly manual: favorite, at most one best, manual ordered rank, note.
//! The diff guard refuses to attribute a visual difference to a single parameter when
//! several parameters differ between two cells.
use crate::{Database, Result};
use rusqlite::params;
use serde_json::{json, Map, Value};

/// Canonical slot order; the cell configuration always contains every key in this order.
const SLOTS: &[(&str, &str)] = &[
    ("model", "model"),
    ("loras", "loras"),
    ("prompt_P1", "promptP1"),
    ("prompt_P2", "promptP2"),
    ("prompt_QUALITY", "promptQUALITY"),
    ("prompt_ACTION", "promptACTION"),
    ("prompt_BG", "promptBG"),
    ("prompt_NEG", "promptNEG"),
    ("base_seed", "baseSeed"),
    ("steps", "steps"),
    ("cfg", "cfg"),
    ("sampler", "sampler"),
];
const PROMPT_SECTIONS: &[&str] = &["P1", "P2", "QUALITY", "ACTION", "BG", "NEG"];
const MAX_TEXT: usize = 200_000;

/// One campaign creation request: the reference workflow, the reference configuration
/// and the explicit grid limit. No execution is implied anywhere in this payload.
pub struct CampaignSpec {
    pub name: String,
    pub workflow_revision_id: String,
    pub model: String,
    pub loras: Value,
    pub prompts: Value,
    pub base_seed: String,
    pub steps: i64,
    pub cfg: f64,
    pub sampler: String,
    pub grid_limit: i64,
    pub notes: String,
}

struct ReferenceRow {
    model: String,
    loras_json: String,
    prompts_json: String,
    base_seed: String,
    steps: i64,
    cfg: f64,
    sampler: String,
}

/// One VARIANT parameter feeding the cartesian product: its slot and its ordered
/// distinct values (value id, normalised JSON value).
struct VariantParameter {
    slot: String,
    values: Vec<(String, Value)>,
}

struct CampaignRecord {
    name: String,
    workflow_revision_id: String,
    workflow_name: String,
    workflow_source_sha256: String,
    model: String,
    loras_json: String,
    prompts_json: String,
    base_seed: String,
    steps: i64,
    cfg: f64,
    sampler: String,
    grid_limit: i64,
    notes: String,
    created_at: String,
}

fn slot_config_key(slot: &str) -> &'static str {
    for (name, key) in SLOTS {
        if *name == slot {
            return key;
        }
    }
    // Unreachable in practice: every entry point validates the slot against SLOTS and
    // the database CHECK stores the same enum.
    ""
}

fn validate_seed(seed: &str) -> Result<()> {
    let parsed: u64 = seed.parse().map_err(|_| "Seed invalide : u64 attendu")?;
    if parsed.to_string() != seed {
        return Err("Seed non canonique : écrire la valeur u64 exacte sans zéro de tête".into());
    }
    Ok(())
}

fn validate_loras(value: &Value) -> Result<()> {
    let list = value
        .as_array()
        .ok_or("LoRA attendus sous forme de tableau JSON")?;
    if list.len() > 64 {
        return Err("Trop de LoRA déclarés".into());
    }
    for lora in list {
        let name = lora
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Chaque LoRA exige un nom texte")?;
        if name.trim().is_empty() || name.len() > 500 {
            return Err("Nom de LoRA vide ou trop long".into());
        }
        for weight in ["weightModel", "weightClip"] {
            let w = lora
                .get(weight)
                .and_then(Value::as_f64)
                .ok_or("Chaque LoRA exige weightModel et weightClip numériques")?;
            if !w.is_finite() {
                return Err("Poids de LoRA non fini refusé".into());
            }
        }
    }
    Ok(())
}

/// Validate and normalise one parameter value; numbers are normalised so that the exact
/// per-cell diff never depends on the JSON spelling typed by the user.
fn validate_slot_value(slot: &str, value: &Value) -> Result<Value> {
    match slot {
        "model" | "sampler" => {
            let text = value.as_str().ok_or("Ce réglage attend une valeur texte")?;
            if text.trim().is_empty() || text.len() > 500 {
                return Err("Valeur texte vide ou trop longue".into());
            }
            Ok(json!(text))
        }
        "prompt_P1" | "prompt_P2" | "prompt_QUALITY" | "prompt_ACTION" | "prompt_BG"
        | "prompt_NEG" => {
            let text = value.as_str().ok_or("Un prompt attend une valeur texte")?;
            if text.len() > MAX_TEXT {
                return Err("Prompt trop long".into());
            }
            Ok(json!(text))
        }
        "base_seed" => {
            let text = value
                .as_str()
                .ok_or("La seed attend une chaîne u64 exacte")?;
            validate_seed(text)?;
            Ok(json!(text))
        }
        "steps" => {
            let steps = value
                .as_i64()
                .filter(|v| *v > 0 && *v <= 1_000_000)
                .ok_or("Steps attendus en entier positif")?;
            Ok(json!(steps))
        }
        "cfg" => {
            let cfg = value.as_f64().ok_or("CFG attendu en nombre")?;
            if !cfg.is_finite() || cfg < 0.0 {
                return Err("CFG négatif ou non fini refusé".into());
            }
            Ok(json!(cfg))
        }
        "loras" => {
            validate_loras(value)?;
            Ok(value.clone())
        }
        _ => Err("Réglage inconnu".into()),
    }
}

impl Database {
    pub fn save_ab_campaign(&self, spec: &CampaignSpec) -> Result<String> {
        let name = spec.name.trim();
        if name.is_empty() || name.len() > 300 {
            return Err("Nom de campagne invalide".into());
        }
        validate_seed(&spec.base_seed)?;
        if spec.steps <= 0 || spec.steps > 1_000_000 {
            return Err("Steps de campagne invalides".into());
        }
        if !spec.cfg.is_finite() || spec.cfg < 0.0 {
            return Err("CFG de campagne invalide".into());
        }
        if !(1..=512).contains(&spec.grid_limit) {
            return Err("Limite de grille hors bornes (1..=512)".into());
        }
        if spec.model.len() > 500 || spec.sampler.len() > 500 || spec.notes.len() > MAX_TEXT {
            return Err("Champ de campagne trop long".into());
        }
        validate_loras(&spec.loras)?;
        let prompts = spec
            .prompts
            .as_object()
            .ok_or("Prompts attendus sous forme d'objet section → texte")?;
        let mut clean = Map::new();
        for (section, text) in prompts {
            if !PROMPT_SECTIONS.contains(&section.as_str()) {
                return Err(format!("Section de prompt inconnue : {section}").into());
            }
            let text = text
                .as_str()
                .ok_or("Chaque prompt de campagne attend un texte")?;
            if text.len() > MAX_TEXT {
                return Err("Prompt de campagne trop long".into());
            }
            clean.insert(section.clone(), json!(text));
        }
        let (workflow_name, fingerprint): (String, String) = self.connection.query_row(
            "SELECT name,source_sha256 FROM workflow_revisions WHERE id=?",
            [&spec.workflow_revision_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO ab_campaigns(id,name,workflow_revision_id,workflow_name,workflow_source_sha256,model,loras_json,prompts_json,base_seed,steps,cfg,sampler,grid_limit,notes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                id,
                name,
                spec.workflow_revision_id,
                workflow_name,
                fingerprint,
                spec.model,
                spec.loras.to_string(),
                Value::Object(clean).to_string(),
                spec.base_seed,
                spec.steps,
                spec.cfg,
                spec.sampler,
                spec.grid_limit,
                spec.notes
            ],
        )?;
        Ok(id)
    }

    pub fn add_ab_parameter(
        &self,
        campaign: &str,
        slot: &str,
        kind: &str,
        label: &str,
        values: &[Value],
    ) -> Result<String> {
        if !SLOTS.iter().any(|(name, _)| *name == slot) {
            return Err("Réglage de campagne inconnu".into());
        }
        if !["constant", "variant"].contains(&kind) {
            return Err("Genre de paramètre inconnu : constant ou variant".into());
        }
        let label = label.trim();
        if label.is_empty() || label.len() > 300 {
            return Err("Libellé de paramètre invalide".into());
        }
        if values.is_empty() {
            return Err("Un paramètre exige au moins une valeur".into());
        }
        if kind == "constant" && values.len() != 1 {
            return Err("Un paramètre constant exige exactement une valeur".into());
        }
        if kind == "variant" && values.len() < 2 {
            return Err("Un paramètre variante exige au moins deux valeurs distinctes".into());
        }
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM ab_parameters WHERE campaign_id=?1 AND slot=?2)",
            params![campaign, slot],
            |r| r.get(0),
        )?;
        if exists {
            return Err("Un paramètre existe déjà pour ce réglage ; modifiez-le plutôt".into());
        }
        let mut cleaned: Vec<(String, Value)> = Vec::new();
        for value in values {
            let value = validate_slot_value(slot, value)?;
            if cleaned.iter().any(|(_, old)| old == &value) {
                return Err("Deux valeurs identiques dans un même paramètre".into());
            }
            cleaned.push((uuid::Uuid::new_v4().to_string(), value));
        }
        let position: i64 = self.connection.query_row(
            "SELECT coalesce(max(position)+1,0) FROM ab_parameters WHERE campaign_id=?",
            [campaign],
            |r| r.get(0),
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO ab_parameters(id,campaign_id,slot,kind,label,position) VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, campaign, slot, kind, label, position],
        )?;
        for (index, (value_id, value)) in cleaned.iter().enumerate() {
            tx.execute(
                "INSERT INTO ab_parameter_values(id,parameter_id,value_json,position) VALUES(?1,?2,?3,?4)",
                params![value_id, id, value.to_string(), index as i64],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    fn reference_config(&self, campaign: &str) -> Result<Map<String, Value>> {
        let row: ReferenceRow = self
            .connection
            .query_row(
                "SELECT model,loras_json,prompts_json,base_seed,steps,cfg,sampler FROM ab_campaigns WHERE id=?",
                [campaign],
                |r| {
                    Ok(ReferenceRow {
                        model: r.get(0)?,
                        loras_json: r.get(1)?,
                        prompts_json: r.get(2)?,
                        base_seed: r.get(3)?,
                        steps: r.get(4)?,
                        cfg: r.get(5)?,
                        sampler: r.get(6)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    rusqlite::Error::InvalidParameterName("Campagne inconnue".into())
                }
                other => other,
            })?;
        let prompts: Value = serde_json::from_str(&row.prompts_json)?;
        let mut config = Map::new();
        config.insert("model".into(), json!(row.model));
        config.insert("loras".into(), serde_json::from_str(&row.loras_json)?);
        for section in PROMPT_SECTIONS {
            let key = slot_config_key(&format!("prompt_{section}"));
            config.insert(
                key.into(),
                prompts.get(*section).cloned().unwrap_or(Value::Null),
            );
        }
        config.insert("baseSeed".into(), json!(row.base_seed));
        config.insert("steps".into(), json!(row.steps));
        config.insert("cfg".into(), json!(row.cfg));
        config.insert("sampler".into(), json!(row.sampler));
        Ok(config)
    }

    /// Materialise the bounded cartesian product of VARIANT parameters as cells.
    /// Constants resolve to their single value; slots without a parameter keep the
    /// campaign reference value. Nothing is executed: only rows are written.
    pub fn prepare_ab_grid(&self, campaign: &str) -> Result<Value> {
        let mut reference = self.reference_config(campaign)?;
        let existing: i64 = self.connection.query_row(
            "SELECT count(*) FROM ab_cells WHERE campaign_id=?",
            [campaign],
            |r| r.get(0),
        )?;
        if existing > 0 {
            return Err("Grille déjà préparée pour cette campagne".into());
        }
        let limit: i64 = self.connection.query_row(
            "SELECT grid_limit FROM ab_campaigns WHERE id=?",
            [campaign],
            |r| r.get(0),
        )?;
        let mut params = self.connection.prepare(
            "SELECT id,slot,kind FROM ab_parameters WHERE campaign_id=? ORDER BY position",
        )?;
        let rows = params
            .query_map([campaign], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(params);
        let mut values = self.connection.prepare(
            "SELECT id,value_json FROM ab_parameter_values WHERE parameter_id=? ORDER BY position",
        )?;
        let mut constants: Vec<(String, String)> = Vec::new();
        let mut variants: Vec<VariantParameter> = Vec::new();
        for (id, slot, kind) in rows {
            let collected = values
                .query_map([&id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let parsed = collected
                .into_iter()
                .map(|(value_id, text)| -> Result<(String, Value)> {
                    Ok((value_id, serde_json::from_str(&text)?))
                })
                .collect::<Result<Vec<_>>>()?;
            if kind == "constant" {
                if parsed.len() != 1 {
                    return Err("Paramètre constant sans exactement une valeur".into());
                }
                reference.insert(slot_config_key(&slot).to_string(), parsed[0].1.clone());
                constants.push((slot, parsed[0].0.clone()));
            } else {
                if parsed.len() < 2 {
                    return Err("Paramètre variante avec moins de deux valeurs".into());
                }
                variants.push(VariantParameter {
                    slot,
                    values: parsed,
                });
            }
        }
        drop(values);
        let mut size: u64 = 1;
        for variant in &variants {
            size = size
                .checked_mul(variant.values.len() as u64)
                .ok_or("Produit de variantes hors bornes")?;
        }
        if size > limit as u64 {
            return Err(format!(
                "Grille de {size} cellules au-delà de la limite explicite de {limit} ; réduisez les variantes ou augmentez la limite"
            )
            .into());
        }
        let tx = self.connection.unchecked_transaction()?;
        // Last variant varies fastest (odometer order), deterministic positions.
        for index in 0..size {
            let mut config = reference.clone();
            let mut choices = Map::new();
            for (slot, value_id) in &constants {
                choices.insert(slot.clone(), json!(value_id));
            }
            let mut rest = index;
            for variant in variants.iter().rev() {
                let pick = (rest % variant.values.len() as u64) as usize;
                rest /= variant.values.len() as u64;
                let (value_id, value) = &variant.values[pick];
                config.insert(slot_config_key(&variant.slot).to_string(), value.clone());
                choices.insert(variant.slot.clone(), json!(value_id));
            }
            tx.execute(
                "INSERT INTO ab_cells(id,campaign_id,position,config_json,choices_json) VALUES(?1,?2,?3,?4,?5)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    campaign,
                    index as i64,
                    Value::Object(config).to_string(),
                    Value::Object(choices).to_string()
                ],
            )?;
        }
        tx.commit()?;
        Ok(json!({"cells": size, "limit": limit}))
    }

    /// Exact field-by-field diff of two cells of the same campaign, with the guard that
    /// refuses to attribute a visual difference to a single parameter when several differ.
    pub fn diff_ab_cells(&self, campaign: &str, cell_a: &str, cell_b: &str) -> Result<Value> {
        let config = |cell: &str| -> Result<Map<String, Value>> {
            let in_campaign: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM ab_cells WHERE id=?1 AND campaign_id=?2)",
                params![cell, campaign],
                |r| r.get(0),
            )?;
            if !in_campaign {
                return Err("Cellule étrangère à cette campagne".into());
            }
            let text: String = self.connection.query_row(
                "SELECT config_json FROM ab_cells WHERE id=?",
                [cell],
                |r| r.get(0),
            )?;
            Ok(serde_json::from_str(&text)?)
        };
        let (a, b) = (config(cell_a)?, config(cell_b)?);
        let mut entries = Vec::new();
        let mut differing = Vec::new();
        for (_, key) in SLOTS {
            let (left, right) = (a.get(*key), b.get(*key));
            let differs = left != right;
            if differs {
                differing.push((*key).to_string());
            }
            entries.push(json!({"key":key,"a":left,"b":right,"differs":differs}));
        }
        let mut variant_slots: Vec<String> = Vec::new();
        let mut query = self
            .connection
            .prepare("SELECT slot FROM ab_parameters WHERE campaign_id=?1 AND kind='variant'")?;
        let rows = query
            .query_map([campaign], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        for slot in rows {
            variant_slots.push(slot_config_key(&slot).to_string());
        }
        let attribution = match differing.len() {
            0 => {
                json!({"allowed":false,"slot":null,"reason":"Les deux cellules ont exactement la même configuration ; rien à attribuer."})
            }
            1 => {
                let slot = &differing[0];
                if variant_slots.contains(slot) {
                    json!({"allowed":true,"slot":slot,"reason":"Une seule variable diffère : l'attribution à ce paramètre est admissible, la preuve visuelle reste humaine."})
                } else {
                    json!({"allowed":false,"slot":slot,"reason":"La différence ne porte pas sur un paramètre de type variante : attribution refusée."})
                }
            }
            n => json!({"allowed":false,"slot":null,"reason":format!(
                "{n} paramètres diffèrent ({}) : refus d'attribuer la différence visuelle à un paramètre unique. Isolez une seule variable ou concluez à l'absence d'attribution.",
                differing.join(", ")
            )}),
        };
        Ok(json!({
            "entries": entries,
            "differing": differing,
            "attribution": attribution,
            "cellA": cell_a,
            "cellB": cell_b
        }))
    }

    /// Manual verdict only: favorite, at most one best per campaign, explicit manual
    /// rank, note. No automatic ranking exists anywhere in this module.
    pub fn set_ab_verdict(
        &self,
        campaign: &str,
        cell: &str,
        favorite: bool,
        best: bool,
        rank: Option<i64>,
        note: &str,
    ) -> Result<()> {
        let in_campaign: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM ab_cells WHERE id=?1 AND campaign_id=?2)",
            params![cell, campaign],
            |r| r.get(0),
        )?;
        if !in_campaign {
            return Err("Cellule étrangère à cette campagne".into());
        }
        if let Some(rank) = rank {
            if rank <= 0 {
                return Err("Rang manuel attendu en entier positif".into());
            }
            let taken: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM ab_cell_verdicts WHERE campaign_id=?1 AND rank=?2 AND cell_id<>?3)",
                params![campaign, rank, cell],
                |r| r.get(0),
            )?;
            if taken {
                return Err(format!(
                    "Le rang {rank} est déjà attribué manuellement à une autre cellule"
                )
                .into());
            }
        }
        let tx = self.connection.unchecked_transaction()?;
        if best {
            tx.execute(
                "UPDATE ab_cell_verdicts SET best=0 WHERE campaign_id=? AND cell_id<>?",
                params![campaign, cell],
            )?;
        }
        tx.execute(
            "INSERT INTO ab_cell_verdicts(cell_id,campaign_id,favorite,best,rank,note) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(cell_id) DO UPDATE SET campaign_id=excluded.campaign_id,favorite=excluded.favorite,best=excluded.best,rank=excluded.rank,note=excluded.note,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![cell, campaign, favorite, best, rank, note],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Reference a provenance row (job/node/index) that already exists. The original
    /// file stays unique on disk: this inserts a link row, never a copy.
    pub fn register_ab_cell_image(
        &self,
        campaign: &str,
        cell: &str,
        job: &str,
        asset: &str,
    ) -> Result<Value> {
        let in_campaign: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM ab_cells WHERE id=?1 AND campaign_id=?2)",
            params![cell, campaign],
            |r| r.get(0),
        )?;
        if !in_campaign {
            return Err("Cellule étrangère à cette campagne".into());
        }
        let job_workflow: Option<String> = self.connection.query_row(
            "SELECT workflow_revision_id FROM generation_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )?;
        let campaign_workflow: String = self.connection.query_row(
            "SELECT workflow_revision_id FROM ab_campaigns WHERE id=?",
            [campaign],
            |r| r.get(0),
        )?;
        if job_workflow.as_deref() != Some(campaign_workflow.as_str()) {
            return Err(
                "Image refusée : le job ne provient pas du workflow de référence de la campagne"
                    .into(),
            );
        }
        let provenance: Option<(Option<String>, Option<i64>, String)> = self.connection.query_row(
            "SELECT node_id,output_index,output_kind FROM job_outputs WHERE job_id=?1 AND asset_id=?2",
            params![job, asset],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
        let Some((node, index, kind)) = provenance else {
            return Err("Aucune provenance connue (job/node/indice) pour cette image ; enregistrez d'abord la sortie du job".into());
        };
        let duplicate: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM ab_cell_images WHERE cell_id=?1 AND asset_id=?2)",
            params![cell, asset],
            |r| r.get(0),
        )?;
        if duplicate {
            return Err("Image déjà enregistrée pour cette cellule".into());
        }
        self.connection.execute(
            "INSERT INTO ab_cell_images(cell_id,job_id,asset_id,node_id,output_index,output_kind) VALUES(?1,?2,?3,?4,?5,?6)",
            params![cell, job, asset, node, index, kind],
        )?;
        Ok(
            json!({"cell":cell,"job":job,"asset":asset,"nodeId":node,"outputIndex":index,"kind":kind}),
        )
    }

    /// Freeze a winning configuration as a reusable preset; the JSON is copied exactly
    /// from the cell and stays attached to the campaign for consultation.
    pub fn save_ab_winner_preset(&self, campaign: &str, cell: &str, name: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 300 {
            return Err("Nom de preset invalide".into());
        }
        let in_campaign: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM ab_cells WHERE id=?1 AND campaign_id=?2)",
            params![cell, campaign],
            |r| r.get(0),
        )?;
        if !in_campaign {
            return Err("Cellule étrangère à cette campagne".into());
        }
        let config: String = self.connection.query_row(
            "SELECT config_json FROM ab_cells WHERE id=?",
            [cell],
            |r| r.get(0),
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO ab_presets(id,campaign_id,name,cell_id,config_json) VALUES(?1,?2,?3,?4,?5)",
            params![id, campaign, name, cell, config],
        )?;
        Ok(id)
    }

    pub fn ab_campaigns(&self) -> Result<Vec<Value>> {
        let mut query = self.connection.prepare(
            "SELECT c.id,c.name,c.workflow_name,c.created_at,
                (SELECT count(*) FROM ab_cells cells WHERE cells.campaign_id=c.id),
                (SELECT count(*) FROM ab_parameters p WHERE p.campaign_id=c.id)
             FROM ab_campaigns c ORDER BY c.rowid DESC",
        )?;
        let rows = query
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "workflow": r.get::<_, String>(2)?,
                    "createdAt": r.get::<_, String>(3)?,
                    "cells": r.get::<_, i64>(4)?,
                    "parameters": r.get::<_, i64>(5)?
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn ab_campaign(&self, id: &str) -> Result<Value> {
        let row: CampaignRecord = self.connection.query_row(
            "SELECT name,workflow_revision_id,workflow_name,workflow_source_sha256,model,loras_json,prompts_json,base_seed,steps,cfg,sampler,grid_limit,notes,created_at FROM ab_campaigns WHERE id=?",
            [id],
            |r| {
                Ok(CampaignRecord {
                    name: r.get(0)?,
                    workflow_revision_id: r.get(1)?,
                    workflow_name: r.get(2)?,
                    workflow_source_sha256: r.get(3)?,
                    model: r.get(4)?,
                    loras_json: r.get(5)?,
                    prompts_json: r.get(6)?,
                    base_seed: r.get(7)?,
                    steps: r.get(8)?,
                    cfg: r.get(9)?,
                    sampler: r.get(10)?,
                    grid_limit: r.get(11)?,
                    notes: r.get(12)?,
                    created_at: r.get(13)?,
                })
            },
        )?;
        let mut parameters = Vec::new();
        let mut query = self
            .connection
            .prepare("SELECT id,slot,kind,label,position FROM ab_parameters WHERE campaign_id=? ORDER BY position")?;
        let param_rows = query
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        for (parameter_id, slot, kind, label, position) in param_rows {
            let mut values = self.connection.prepare(
                "SELECT id,value_json,position FROM ab_parameter_values WHERE parameter_id=? ORDER BY position",
            )?;
            let value_rows = values
                .query_map([&parameter_id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(values);
            let values = value_rows
                .into_iter()
                .map(|(value_id, text, position)| {
                    Ok(json!({"id":value_id,"value":serde_json::from_str::<Value>(&text)?,"position":position}))
                })
                .collect::<Result<Vec<_>>>()?;
            parameters.push(json!({
                "id":parameter_id,"slot":slot,"kind":kind,"label":label,"position":position,"values":values
            }));
        }
        let mut cells = Vec::new();
        let mut query = self.connection.prepare(
            "SELECT id,position,config_json,choices_json FROM ab_cells WHERE campaign_id=? ORDER BY position",
        )?;
        let cell_rows = query
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        for (cell_id, position, config_text, choices_text) in cell_rows {
            let mut images = self.connection.prepare(
                "SELECT job_id,asset_id,node_id,output_index,output_kind FROM ab_cell_images WHERE cell_id=? ORDER BY rowid",
            )?;
            let image_rows = images
                .query_map([&cell_id], |r| {
                    Ok(json!({
                        "jobId":r.get::<_, String>(0)?,
                        "assetId":r.get::<_, String>(1)?,
                        "nodeId":r.get::<_, Option<String>>(2)?,
                        "outputIndex":r.get::<_, Option<i64>>(3)?,
                        "kind":r.get::<_, String>(4)?
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(images);
            let verdict = self
                .connection
                .query_row(
                    "SELECT favorite,best,rank,note FROM ab_cell_verdicts WHERE cell_id=?",
                    [&cell_id],
                    |r| {
                        Ok(json!({
                            "favorite":r.get::<_, bool>(0)?,
                            "best":r.get::<_, bool>(1)?,
                            "rank":r.get::<_, Option<i64>>(2)?,
                            "note":r.get::<_, String>(3)?
                        }))
                    },
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            cells.push(json!({
                "id":cell_id,
                "position":position,
                "config":serde_json::from_str::<Value>(&config_text)?,
                "choices":serde_json::from_str::<Value>(&choices_text)?,
                "images":image_rows,
                "verdict":verdict
            }));
        }
        let mut presets = Vec::new();
        let mut query = self.connection.prepare(
            "SELECT id,name,cell_id,config_json,created_at FROM ab_presets WHERE campaign_id=? ORDER BY rowid",
        )?;
        let preset_rows = query
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        for (preset_id, name, cell_id, config_text, created_at) in preset_rows {
            presets.push(json!({
                "id":preset_id,"name":name,"cellId":cell_id,
                "config":serde_json::from_str::<Value>(&config_text)?,"createdAt":created_at
            }));
        }
        Ok(json!({
            "id":id,
            "name":row.name,
            "workflow":{"id":row.workflow_revision_id,"name":row.workflow_name,"sourceSha256":row.workflow_source_sha256},
            "model":row.model,
            "loras":serde_json::from_str::<Value>(&row.loras_json)?,
            "prompts":serde_json::from_str::<Value>(&row.prompts_json)?,
            "baseSeed":row.base_seed,
            "steps":row.steps,
            "cfg":row.cfg,
            "sampler":row.sampler,
            "gridLimit":row.grid_limit,
            "notes":row.notes,
            "createdAt":row.created_at,
            "parameters":parameters,
            "cells":cells,
            "presets":presets
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    fn db() -> Database {
        Database::in_memory().unwrap()
    }

    /// One reference workflow revision usable by campaigns and jobs.
    fn workflow(connection: &rusqlite::Connection, id: &str) {
        connection
            .execute(
                "INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES(?1,'Test','fingerprint-sha256','comfy_api','{}')",
                [id],
            )
            .unwrap();
    }

    fn spec(name: &str, workflow_id: &str, grid_limit: i64) -> CampaignSpec {
        CampaignSpec {
            name: name.into(),
            workflow_revision_id: workflow_id.into(),
            model: "model_a.safetensors".into(),
            loras: json!([]),
            prompts: json!({}),
            base_seed: "7".into(),
            steps: 20,
            cfg: 4.5,
            sampler: "euler".into(),
            grid_limit,
            notes: String::new(),
        }
    }

    fn new_campaign(db: &Database, workflow_id: &str) -> String {
        let spec = spec("Campagne de test", workflow_id, 64);
        db.save_ab_campaign(&spec).unwrap()
    }
    /// Synthetic provenance fixture: library, asset, succeeded job and one job_output
    /// row. Nothing is written on disk: references only.
    fn synthetic_output(
        connection: &rusqlite::Connection,
        workflow: &str,
        job: &str,
        asset: &str,
        node: Option<&str>,
    ) {
        connection
            .execute(
                "INSERT OR IGNORE INTO libraries(id,label,root_path,platform) VALUES('lib','Bibliothèque','/tmp/ab-lab','linux')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO assets(id,library_id,relative_path,availability) VALUES(?1,'lib',?1,'present')",
                [asset],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state) VALUES(?1,?2,'42','{}','succeeded')",
                rusqlite::params![job, workflow],
            )
            .unwrap();
        if let Some(node) = node {
            connection
                .execute(
                    "INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES(?1,?2,?3,0,'saved')",
                    rusqlite::params![job, asset, node],
                )
                .unwrap();
        }
    }

    fn cell_config(db: &Database, cell: &str) -> Map<String, Value> {
        let text: String = db
            .connection
            .query_row("SELECT config_json FROM ab_cells WHERE id=?", [cell], |r| {
                r.get(0)
            })
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn cell_ids(db: &Database, campaign: &str) -> Vec<String> {
        let mut query = db
            .connection
            .prepare("SELECT id FROM ab_cells WHERE campaign_id=? ORDER BY position")
            .unwrap();
        query
            .query_map([campaign], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    #[test]
    fn campaign_creation_validates_and_freezes_workflow_reference() {
        let db = db();
        workflow(&db.connection, "w");
        let id = new_campaign(&db, "w");
        let stored = db.ab_campaign(&id).unwrap();
        assert_eq!(stored["name"], "Campagne de test");
        assert_eq!(stored["workflow"]["name"], "Test");
        assert_eq!(stored["workflow"]["sourceSha256"], "fingerprint-sha256");
        assert_eq!(stored["baseSeed"], "7");
        assert_eq!(stored["gridLimit"], 64);
        // u64 seeds are exact decimal strings; canonical form and range are enforced.
        let mut max_seed = spec("Max", "w", 8);
        max_seed.base_seed = "18446744073709551615".into();
        assert!(db.save_ab_campaign(&max_seed).is_ok());
        let rejected_seeds: Vec<(&str, String)> = vec![
            ("seed non canonique", "01".into()),
            ("seed hors u64", "18446744073709551616".into()),
            ("seed non numérique", "abc".into()),
            ("seed négative", "-1".into()),
        ];
        for (reason, seed) in rejected_seeds {
            let mut bad = spec("X", "w", 8);
            bad.base_seed = seed.clone();
            assert!(
                db.save_ab_campaign(&bad).is_err(),
                "seed acceptée ({reason}) : {seed}"
            );
        }
        let mut zero_steps = spec("X", "w", 8);
        zero_steps.steps = 0;
        assert!(db.save_ab_campaign(&zero_steps).is_err());
        let mut negative_cfg = spec("X", "w", 8);
        negative_cfg.cfg = -0.5;
        assert!(db.save_ab_campaign(&negative_cfg).is_err());
        assert!(db.save_ab_campaign(&spec("X", "w", 0)).is_err());
        assert!(db.save_ab_campaign(&spec("X", "w", 513)).is_err());
        assert!(db.save_ab_campaign(&spec("", "w", 8)).is_err());
        assert!(db
            .save_ab_campaign(&spec("X", "workflow-inconnu", 8))
            .is_err());
        let mut unknown_section = spec("X", "w", 8);
        unknown_section.prompts = json!({"SECTION": "x"});
        assert!(db.save_ab_campaign(&unknown_section).is_err());
        let mut loras_object = spec("X", "w", 8);
        loras_object.loras = json!({"name": "l"});
        assert!(db.save_ab_campaign(&loras_object).is_err());
        let mut lora_without_weights = spec("X", "w", 8);
        lora_without_weights.loras = json!([{"name": "l"}]);
        assert!(db.save_ab_campaign(&lora_without_weights).is_err());
    }

    #[test]
    fn parameters_split_constants_from_variants_strictly() {
        let db = db();
        workflow(&db.connection, "w");
        let campaign = new_campaign(&db, "w");
        let constant = db
            .add_ab_parameter(
                &campaign,
                "model",
                "constant",
                "Modèle imposé",
                &[json!("model_b.safetensors")],
            )
            .unwrap();
        let variant = db
            .add_ab_parameter(
                &campaign,
                "cfg",
                "variant",
                "Guidage",
                &[json!(4.5), json!(7.0)],
            )
            .unwrap();
        assert_ne!(constant, variant);
        // A constant holds exactly one value; a variant holds at least two distinct ones.
        assert!(db
            .add_ab_parameter(&campaign, "steps", "constant", "X", &[json!(20), json!(30)])
            .is_err());
        assert!(db
            .add_ab_parameter(&campaign, "steps", "variant", "X", &[json!(20)])
            .is_err());
        assert!(db
            .add_ab_parameter(&campaign, "steps", "variant", "X", &[])
            .is_err());
        assert!(db
            .add_ab_parameter(
                &campaign,
                "steps",
                "variant",
                "Doublon",
                &[json!(20), json!(20)]
            )
            .is_err());
        assert!(db
            .add_ab_parameter(&campaign, "inconnu", "constant", "X", &[json!("v")])
            .is_err());
        assert!(db
            .add_ab_parameter(&campaign, "cfg", "hybride", "X", &[json!(1.0), json!(2.0)])
            .is_err());
        // One parameter per slot: the same slot cannot be declared twice.
        assert!(db
            .add_ab_parameter(
                &campaign,
                "cfg",
                "variant",
                "Encore",
                &[json!(2.0), json!(3.0)]
            )
            .is_err());
        // Seed variants stay exact u64 decimal strings.
        assert!(db
            .add_ab_parameter(
                &campaign,
                "base_seed",
                "variant",
                "Seeds",
                &[json!("42"), json!("18446744073709551615")]
            )
            .is_ok());
        assert!(db
            .add_ab_parameter(
                &campaign,
                "base_seed",
                "variant",
                "Seeds",
                &[json!("042"), json!("42")]
            )
            .is_err());
    }

    #[test]
    fn grid_is_bounded_cartesian_product_prepared_without_execution() {
        let db = db();
        workflow(&db.connection, "w");
        let campaign = new_campaign(&db, "w");
        db.add_ab_parameter(
            &campaign,
            "model",
            "constant",
            "Modèle",
            &[json!("model_b.safetensors")],
        )
        .unwrap();
        db.add_ab_parameter(
            &campaign,
            "cfg",
            "variant",
            "Guidage",
            &[json!(4.5), json!(7.0)],
        )
        .unwrap();
        db.add_ab_parameter(
            &campaign,
            "sampler",
            "variant",
            "Échantillonneur",
            &[json!("euler"), json!("dpmpp_2m"), json!("heun")],
        )
        .unwrap();
        let report = db.prepare_ab_grid(&campaign).unwrap();
        assert_eq!(report["cells"], 6);
        assert_eq!(report["limit"], 64);
        let cells = cell_ids(&db, &campaign);
        assert_eq!(cells.len(), 6);
        // Deterministic odometer: constants applied everywhere, last variant varies fastest,
        // slots without a parameter keep the campaign reference value.
        let first = cell_config(&db, &cells[0]);
        assert_eq!(first["model"], "model_b.safetensors");
        assert_eq!(first["cfg"], 4.5);
        assert_eq!(first["sampler"], "euler");
        assert_eq!(first["steps"], 20);
        assert_eq!(first["baseSeed"], "7");
        let second = cell_config(&db, &cells[1]);
        assert_eq!(second["cfg"], 4.5);
        assert_eq!(second["sampler"], "dpmpp_2m");
        let fourth = cell_config(&db, &cells[3]);
        assert_eq!(fourth["cfg"], 7.0);
        assert_eq!(fourth["sampler"], "euler");
        // Re-preparing is refused: the grid exists.
        assert!(db.prepare_ab_grid(&campaign).is_err());

        // Over the explicit limit the preparation is refused and nothing is written.
        let refused = db.save_ab_campaign(&spec("Bornée", "w", 4)).unwrap();
        db.add_ab_parameter(&refused, "cfg", "variant", "A", &[json!(1.0), json!(2.0)])
            .unwrap();
        db.add_ab_parameter(&refused, "steps", "variant", "B", &[json!(10), json!(20)])
            .unwrap();
        db.add_ab_parameter(
            &refused,
            "sampler",
            "variant",
            "C",
            &[json!("x"), json!("y")],
        )
        .unwrap();
        let error = db.prepare_ab_grid(&refused).unwrap_err().to_string();
        assert!(error.contains("8"), "message sans le total réel : {error}");
        assert!(error.contains("4"), "message sans la limite : {error}");
        assert_eq!(cell_ids(&db, &refused).len(), 0);
    }

    #[test]
    fn campaign_persists_across_reopen_with_schema_v14() {
        let dir = std::env::temp_dir().join(format!("studio-ablab-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let result = (|| -> crate::Result<()> {
            let path = dir.join("studio.sqlite3");
            let db = Database::open(&path)?;
            workflow(&db.connection, "w");
            let campaign = new_campaign(&db, "w");
            db.add_ab_parameter(
                &campaign,
                "cfg",
                "variant",
                "Guidage",
                &[json!(4.5), json!(7.0)],
            )?;
            db.prepare_ab_grid(&campaign)?;
            let cells = cell_ids(&db, &campaign);
            db.set_ab_verdict(&campaign, &cells[1], true, true, Some(1), "meilleur rendu")?;
            let version: u32 = db
                .connection
                .pragma_query_value(None, "user_version", |r| r.get(0))?;
            assert!(
                version >= 14,
                "migration 0014 must be applied, got {version}"
            );
            let migration: String = db.connection.query_row(
                "SELECT name FROM schema_migrations WHERE version=14",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(migration, "ab-laboratory");
            drop(db);
            // Reopen: campaign, grid, verdicts survive the restart.
            let db = Database::open(&path)?;
            let stored = db.ab_campaign(&campaign)?;
            assert_eq!(stored["cells"].as_array().unwrap().len(), 2);
            assert_eq!(stored["cells"][1]["verdict"]["favorite"], true);
            assert_eq!(stored["cells"][1]["verdict"]["best"], true);
            assert_eq!(stored["cells"][1]["verdict"]["rank"], 1);
            assert_eq!(stored["cells"][1]["verdict"]["note"], "meilleur rendu");
            assert_eq!(stored["cells"][0]["verdict"], Value::Null);
            Ok(())
        })();
        let cleanup = std::fs::remove_dir_all(&dir);
        result.unwrap();
        cleanup.unwrap();
    }

    #[test]
    fn verdicts_are_manual_unique_and_persisted() {
        let db = db();
        workflow(&db.connection, "w");
        let campaign = new_campaign(&db, "w");
        db.add_ab_parameter(
            &campaign,
            "cfg",
            "variant",
            "Guidage",
            &[json!(4.5), json!(7.0)],
        )
        .unwrap();
        db.prepare_ab_grid(&campaign).unwrap();
        let cells = cell_ids(&db, &campaign);
        db.set_ab_verdict(&campaign, &cells[0], true, false, Some(2), "correct")
            .unwrap();
        db.set_ab_verdict(&campaign, &cells[1], false, true, Some(1), "gagnant")
            .unwrap();
        // One best per campaign: declaring another best clears the previous one.
        db.set_ab_verdict(&campaign, &cells[1], false, false, None, "")
            .unwrap();
        db.set_ab_verdict(&campaign, &cells[0], true, false, Some(2), "correct")
            .unwrap();
        db.set_ab_verdict(&campaign, &cells[1], false, true, Some(1), "gagnant")
            .unwrap();
        db.set_ab_verdict(&campaign, &cells[0], false, true, None, "")
            .unwrap();
        let stored = db.ab_campaign(&campaign).unwrap();
        assert_eq!(stored["cells"][0]["verdict"]["best"], true);
        assert_eq!(stored["cells"][1]["verdict"]["best"], false);
        // A manual rank is used at most once per campaign.
        let error = db
            .set_ab_verdict(&campaign, &cells[0], false, false, Some(1), "")
            .unwrap_err()
            .to_string();
        assert!(error.contains("rang 1"), "message inattendu : {error}");
        // Database constraint as backstop for concurrent writers.
        let taken: bool = db
            .connection
            .query_row("SELECT EXISTS(SELECT 1 FROM ab_cell_verdicts WHERE campaign_id=?1 AND rank=1 AND cell_id<>?2)", rusqlite::params![campaign, cells[0]], |r| r.get(0))
            .unwrap();
        assert!(taken);
        assert!(db
            .set_ab_verdict(&campaign, &cells[0], false, false, Some(0), "")
            .is_err());
        assert!(db
            .set_ab_verdict(&campaign, &cells[0], false, false, Some(-2), "")
            .is_err());
        // A cell of another campaign is refused.
        let other = new_campaign(&db, "w");
        assert!(db
            .set_ab_verdict(&other, &cells[0], false, false, None, "")
            .is_err());
    }

    #[test]
    fn diff_is_exact_field_by_field_with_single_variable_attribution() {
        let db = db();
        workflow(&db.connection, "w");
        let campaign = new_campaign(&db, "w");
        db.add_ab_parameter(
            &campaign,
            "cfg",
            "variant",
            "Guidage",
            &[json!(4.5), json!(7.0)],
        )
        .unwrap();
        db.prepare_ab_grid(&campaign).unwrap();
        let cells = cell_ids(&db, &campaign);
        let diff = db.diff_ab_cells(&campaign, &cells[0], &cells[1]).unwrap();
        let entries = diff["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 12);
        let differing: Vec<&str> = diff["differing"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .map(Option::unwrap)
            .collect();
        assert_eq!(differing, vec!["cfg"]);
        let entry = entries.iter().find(|e| e["key"] == "cfg").unwrap();
        assert_eq!(entry["a"], 4.5);
        assert_eq!(entry["b"], 7.0);
        assert_eq!(entry["differs"], true);
        assert_eq!(diff["attribution"]["allowed"], true);
        assert_eq!(diff["attribution"]["slot"], "cfg");
        // Foreign cells are refused.
        let other = new_campaign(&db, "w");
        assert!(db.diff_ab_cells(&other, &cells[0], &cells[1]).is_err());
    }

    #[test]
    fn diff_refuses_single_parameter_attribution_when_several_differ() {
        let db = db();
        workflow(&db.connection, "w");
        let campaign = new_campaign(&db, "w");
        db.add_ab_parameter(
            &campaign,
            "cfg",
            "variant",
            "Guidage",
            &[json!(4.5), json!(7.0)],
        )
        .unwrap();
        db.add_ab_parameter(
            &campaign,
            "steps",
            "variant",
            "Pas",
            &[json!(20), json!(30)],
        )
        .unwrap();
        db.prepare_ab_grid(&campaign).unwrap();
        let cells = cell_ids(&db, &campaign);
        // cells[0] and cells[3] differ in cfg AND steps: no single-parameter conclusion.
        let diff = db.diff_ab_cells(&campaign, &cells[0], &cells[3]).unwrap();
        let differing = diff["differing"].as_array().unwrap();
        assert_eq!(differing.len(), 2);
        assert!(differing.contains(&json!("cfg")));
        assert!(differing.contains(&json!("steps")));
        assert_eq!(diff["attribution"]["allowed"], false);
        let reason = diff["attribution"]["reason"].as_str().unwrap();
        assert!(reason.contains("2 paramètres"), "garde muette : {reason}");
        assert!(
            reason.contains("cfg") && reason.contains("steps"),
            "variables absentes : {reason}"
        );
        // Per-field entries stay exact even under the guard.
        let entry = diff["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["key"] == "steps")
            .unwrap();
        assert_eq!(entry["a"], 20);
        assert_eq!(entry["b"], 30);
    }

    #[test]
    fn winning_configuration_is_saved_as_reusable_preset() {
        let db = db();
        workflow(&db.connection, "w");
        let campaign = new_campaign(&db, "w");
        db.add_ab_parameter(
            &campaign,
            "cfg",
            "variant",
            "Guidage",
            &[json!(4.5), json!(7.0)],
        )
        .unwrap();
        db.prepare_ab_grid(&campaign).unwrap();
        let cells = cell_ids(&db, &campaign);
        let preset = db
            .save_ab_winner_preset(&campaign, &cells[1], "  Gagnant CFG 7  ")
            .unwrap();
        assert!(!preset.is_empty());
        let stored = db.ab_campaign(&campaign).unwrap();
        let presets = stored["presets"].as_array().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0]["name"], "Gagnant CFG 7");
        assert_eq!(presets[0]["cellId"], cells[1]);
        // The preset configuration is the exact cell configuration.
        assert_eq!(presets[0]["config"], stored["cells"][1]["config"]);
        assert_eq!(cell_config(&db, &cells[1])["cfg"], 7.0);
        assert!(db
            .save_ab_winner_preset(&campaign, &cells[0], "   ")
            .is_err());
        let other = new_campaign(&db, "w");
        assert!(db.save_ab_winner_preset(&other, &cells[0], "X").is_err());
    }

    #[test]
    fn cell_images_are_provenance_references_never_copies() {
        let db = db();
        workflow(&db.connection, "w");
        workflow(&db.connection, "other");
        let campaign = new_campaign(&db, "w");
        db.add_ab_parameter(
            &campaign,
            "cfg",
            "variant",
            "Guidage",
            &[json!(4.5), json!(7.0)],
        )
        .unwrap();
        db.prepare_ab_grid(&campaign).unwrap();
        let cells = cell_ids(&db, &campaign);
        synthetic_output(&db.connection, "w", "job-1", "asset-1", Some("9"));
        synthetic_output(&db.connection, "w", "job-2", "asset-2", None);
        // A provenance row may exist without a node id; the reference keeps it as is.
        db.connection
            .execute(
                "INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES('job-2','asset-2',NULL,0,'saved')",
                [],
            )
            .unwrap();
        synthetic_output(&db.connection, "other", "job-3", "asset-3", Some("5"));
        synthetic_output(&db.connection, "w", "job-4", "asset-4", None);

        let registered = db
            .register_ab_cell_image(&campaign, &cells[0], "job-1", "asset-1")
            .unwrap();
        // The reference keeps the full provenance triplet (job/node/index).
        assert_eq!(registered["nodeId"], "9");
        assert_eq!(registered["outputIndex"], 0);
        assert_eq!(registered["kind"], "saved");
        // A second image with provenance but without node id is still accepted by reference.
        let second = db
            .register_ab_cell_image(&campaign, &cells[0], "job-2", "asset-2")
            .unwrap();
        assert_eq!(second["nodeId"], Value::Null);
        assert_eq!(second["outputIndex"], 0);
        // Duplication is refused.
        assert!(db
            .register_ab_cell_image(&campaign, &cells[0], "job-1", "asset-1")
            .is_err());
        // A job from another workflow than the campaign reference is refused.
        let error = db
            .register_ab_cell_image(&campaign, &cells[0], "job-3", "asset-3")
            .unwrap_err()
            .to_string();
        assert!(error.contains("workflow"), "message inattendu : {error}");
        // An asset without provenance (job/node/index) is refused.
        assert!(db
            .register_ab_cell_image(&campaign, &cells[0], "job-4", "asset-4")
            .is_err());
        let stored = db.ab_campaign(&campaign).unwrap();
        let images = stored["cells"][0]["images"].as_array().unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0]["assetId"], "asset-1");
        // The original row is unique on disk: one asset row per image, no copy.
        let assets: i64 = db
            .connection
            .query_row(
                "SELECT count(*) FROM assets WHERE id IN ('asset-1','asset-2')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(assets, 2);
        let links: String = db
            .connection
            .query_row(
                "SELECT job_id||'/'||node_id FROM ab_cell_images WHERE cell_id=?1 AND asset_id='asset-1'",
                [&cells[0]],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(links, "job-1/9");
    }

    #[test]
    fn full_synthetic_campaign_journey_end_to_end() {
        let dir = std::env::temp_dir().join(format!("studio-ablab-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let result = (|| -> crate::Result<()> {
            let path = dir.join("studio.sqlite3");
            let db = Database::open(&path)?;
            workflow(&db.connection, "w");
            let journey = CampaignSpec {
                name: "Parcours complet".into(),
                workflow_revision_id: "w".into(),
                model: "model_ref.safetensors".into(),
                loras: json!([{"name":"detail_tweaker","weightModel":0.6,"weightClip":0.6}]),
                prompts: json!({"P1":"portrait studio","NEG":"flou"}),
                base_seed: "12345678901234567890".into(),
                steps: 25,
                cfg: 5.0,
                sampler: "dpmpp_2m".into(),
                grid_limit: 16,
                notes: "Campagne de qualification".into(),
            };
            let campaign = db.save_ab_campaign(&journey)?;
            db.add_ab_parameter(
                &campaign,
                "loras",
                "constant",
                "LoRA imposé",
                &[json!([{"name":"detail_tweaker","weightModel":0.8,"weightClip":0.8}])],
            )?;
            db.add_ab_parameter(
                &campaign,
                "cfg",
                "variant",
                "Guidage",
                &[json!(4.5), json!(7.0)],
            )?;
            db.add_ab_parameter(
                &campaign,
                "prompt_P1",
                "variant",
                "Sujet",
                &[json!("portrait studio"), json!("portrait extérieur")],
            )?;
            db.prepare_ab_grid(&campaign)?;
            let cells = cell_ids(&db, &campaign);
            assert_eq!(cells.len(), 4);
            // Synthetic images on two cells; the file itself never exists or moves.
            synthetic_output(&db.connection, "w", "job-a", "asset-a", Some("9"));
            synthetic_output(&db.connection, "w", "job-b", "asset-b", Some("9"));
            db.register_ab_cell_image(&campaign, &cells[0], "job-a", "asset-a")?;
            db.register_ab_cell_image(&campaign, &cells[3], "job-b", "asset-b")?;
            // Manual verdicts across the grid.
            db.set_ab_verdict(&campaign, &cells[0], false, false, Some(2), "correct")?;
            db.set_ab_verdict(&campaign, &cells[3], true, true, Some(1), "net et stable")?;
            // Diffs: one variable → admissible attribution; two variables → refusal.
            let allowed = db.diff_ab_cells(&campaign, &cells[0], &cells[1])?;
            assert_eq!(allowed["attribution"]["allowed"], true);
            assert_eq!(allowed["attribution"]["slot"], "promptP1");
            let refused = db.diff_ab_cells(&campaign, &cells[0], &cells[3])?;
            assert_eq!(refused["attribution"]["allowed"], false);
            // The winner becomes a reusable preset.
            let preset = db.save_ab_winner_preset(&campaign, &cells[3], "Extérieur gagnant")?;
            assert!(!preset.is_empty());
            drop(db);
            // Everything survives a restart.
            let db = Database::open(&path)?;
            let stored = db.ab_campaign(&campaign)?;
            assert_eq!(stored["workflow"]["sourceSha256"], "fingerprint-sha256");
            assert_eq!(stored["baseSeed"], "12345678901234567890");
            // The campaign reference keeps its own LoRA weights; the constant only
            // applies inside the cells' frozen configuration.
            assert_eq!(stored["loras"][0]["weightModel"], 0.6);
            assert_eq!(stored["parameters"].as_array().unwrap().len(), 3);
            assert_eq!(stored["presets"][0]["name"], "Extérieur gagnant");
            let cell = &stored["cells"][3];
            assert_eq!(cell["verdict"]["best"], true);
            assert_eq!(cell["verdict"]["rank"], 1);
            assert_eq!(cell["images"][0]["assetId"], "asset-b");
            assert_eq!(cell["images"][0]["nodeId"], "9");
            let config = &cell["config"];
            assert_eq!(config["loras"][0]["weightModel"], 0.8);
            assert_eq!(config["promptP1"], "portrait extérieur");
            assert_eq!(config["cfg"], 7.0);
            Ok(())
        })();
        let cleanup = std::fs::remove_dir_all(&dir);
        result.unwrap();
        cleanup.unwrap();
    }
}
