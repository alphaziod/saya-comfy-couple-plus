//! A/B laboratory → workflow runner. Turns prepared grid cells into explicit
//! runner edits on the campaign's reference workflow, prepares draft jobs for
//! the cells the user selects (never the whole grid implicitly), links them to
//! their cell, and collects their recorded outputs. Nothing here submits a job:
//! submission stays the existing confirmed `submit` path, one draft at a time.
use crate::workflow::{Edit, Field};
use crate::{Database, Result};
use rusqlite::params;
use serde_json::{json, Map, Value};

/// Value written into a mapped field, in the runner's textual convention.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Numbers compare by value (a REAL `7` and a field `7.0` are the same setting).
fn same(field: &Field, value: &Value) -> bool {
    if field.value == text(value) {
        return true;
    }
    match (field.value.parse::<f64>(), value.as_f64()) {
        (Ok(a), Some(b)) => a == b,
        _ => false,
    }
}

struct Planner<'a> {
    fields: &'a [Field],
    graph: &'a Value,
    edits: Vec<Edit>,
    problems: Vec<String>,
}

impl Planner<'_> {
    /// One setting onto the unique field matching `pick`. An equal value
    /// needs no mapping; a different value requires an enabled, unique field.
    fn set(&mut self, label: &str, value: &Value, pick: impl Fn(&Field) -> bool) {
        let candidates: Vec<&Field> = self.fields.iter().filter(|f| pick(f)).collect();
        match candidates.as_slice() {
            [] => self.problems.push(format!(
                "{label} : aucune entrée correspondante dans le workflow de référence"
            )),
            [field] => {
                if same(field, value) {
                    return;
                }
                if !field.enabled {
                    self.problems.push(format!(
                        "{label} : l'entrée {}.{} n'est pas mappée ; activez-la dans Workflows",
                        field.node_id, field.input_name
                    ));
                    return;
                }
                self.edits.push(Edit {
                    section: field.section.clone(),
                    node_id: field.node_id.clone(),
                    input_name: field.input_name.clone(),
                    value: text(value),
                });
            }
            more => self.problems.push(format!(
                "{label} : {} entrées possibles ({}) ; cible ambiguë",
                more.len(),
                more.iter()
                    .map(|f| format!("{}.{}", f.node_id, f.input_name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    fn cell(&mut self, config: &Map<String, Value>) {
        for section in ["P1", "P2", "QUALITY", "ACTION", "BG", "NEG"] {
            let Some(prompt) = config
                .get(&format!("prompt{section}"))
                .filter(|v| !v.is_null())
            else {
                continue;
            };
            self.set(&format!("Prompt {section}"), prompt, |f| {
                f.enabled && f.section.as_deref() == Some(section)
            });
        }
        if let Some(seed) = config.get("baseSeed").filter(|v| !v.is_null()) {
            self.set("Seed", seed, |f| f.value_type == "seed");
        }
        for (key, input) in [
            ("steps", "steps"),
            ("cfg", "cfg"),
            ("sampler", "sampler_name"),
        ] {
            if let Some(value) = config
                .get(key)
                .filter(|v| !v.is_null() && v.as_str() != Some(""))
            {
                self.set(key, value, |f| f.input_name == input);
            }
        }
        if let Some(model) = config
            .get("model")
            .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        {
            self.set("Modèle", model, |f| f.input_name == "ckpt_name");
        }
        for lora in config
            .get("loras")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = lora["name"].as_str().unwrap_or_default().to_owned();
            let nodes: Vec<String> = self
                .graph
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, node)| node["inputs"]["lora_name"].as_str() == Some(name.as_str()))
                .map(|(id, _)| id.clone())
                .collect();
            if nodes.len() != 1 {
                self.problems.push(format!(
                    "LoRA {name} : {} node(s) LoraLoader correspondant(s) dans le workflow de référence ; un seul attendu",
                    nodes.len()
                ));
                continue;
            }
            for (key, input) in [
                ("weightModel", "strength_model"),
                ("weightClip", "strength_clip"),
            ] {
                if let Some(weight) = lora.get(key).filter(|v| !v.is_null()) {
                    let node = nodes[0].clone();
                    self.set(&format!("LoRA {name} {input}"), weight, |f| {
                        f.node_id == node && f.input_name == input
                    });
                }
            }
        }
    }
}

impl Database {
    fn campaign_reference(&self, campaign: &str) -> Result<(String, String, String)> {
        self.connection
            .query_row(
                "SELECT c.workflow_revision_id,c.workflow_source_sha256,w.source_sha256 FROM ab_campaigns c LEFT JOIN workflow_revisions w ON w.id=c.workflow_revision_id WHERE c.id=?",
                [campaign],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default())),
            )
            .map_err(|_| "Campagne inconnue".into())
    }

    /// Plan of every prepared cell: explicit runner edits, problems (never a
    /// partial plan), and jobs already prepared for it. Read-only.
    pub fn ab_plan(&self, campaign: &str) -> Result<Value> {
        let (workflow, campaign_hash, current_hash) = self.campaign_reference(campaign)?;
        let mut global = Vec::new();
        if current_hash.is_empty() {
            global.push("Workflow de référence introuvable".to_owned());
        } else if campaign_hash != current_hash {
            global.push(
                "Le workflow de référence a changé depuis la création de la campagne".to_owned(),
            );
        }
        let fields = if global.is_empty() {
            self.workflow(&workflow)?.fields
        } else {
            Vec::new()
        };
        let graph: Value = if global.is_empty() {
            serde_json::from_str(&self.connection.query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [&workflow],
                |r| r.get::<_, String>(0),
            )?)?
        } else {
            Value::Null
        };
        let mut query = self.connection.prepare(
            "SELECT id,position,config_json FROM ab_cells WHERE campaign_id=? ORDER BY position",
        )?;
        let rows = query
            .query_map([campaign], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        if rows.is_empty() {
            global.push("Grille non préparée".to_owned());
        }
        let mut cells = Vec::new();
        for (id, position, config) in rows {
            let config: Map<String, Value> = serde_json::from_str(&config)?;
            let mut planner = Planner {
                fields: &fields,
                graph: &graph,
                edits: Vec::new(),
                problems: Vec::new(),
            };
            if global.is_empty() {
                planner.cell(&config);
            }
            let jobs: Vec<Value> = self
                .connection
                .prepare("SELECT j.id,j.state FROM ab_cell_jobs l JOIN generation_jobs j ON j.id=l.job_id WHERE l.cell_id=? ORDER BY l.created_at")?
                .query_map([&id], |r| Ok(json!({"id":r.get::<_, String>(0)?,"state":r.get::<_, String>(1)?})))?
                .collect::<rusqlite::Result<_>>()?;
            cells.push(json!({
                "cellId": id, "position": position,
                "edits": planner.edits, "problems": planner.problems, "jobs": jobs,
            }));
        }
        let ready = global.is_empty()
            && cells
                .iter()
                .all(|c| c["problems"].as_array().is_some_and(Vec::is_empty));
        Ok(
            json!({"campaign": campaign, "workflowId": workflow, "problems": global, "ready": ready, "cells": cells}),
        )
    }

    /// Draft jobs for the explicitly selected cells only. All-or-nothing: if any
    /// selected cell has a problem, nothing is prepared. Nothing is submitted.
    pub fn prepare_ab_cells(
        &self,
        campaign: &str,
        cells: &[String],
        node_info: &Value,
    ) -> Result<Value> {
        if cells.is_empty() || cells.len() > 64 {
            return Err("Sélectionnez de 1 à 64 cellules à préparer".into());
        }
        let plan = self.ab_plan(campaign)?;
        if let Some(problem) = plan["problems"].as_array().and_then(|p| p.first()) {
            return Err(format!(
                "Préparation refusée : {}",
                problem.as_str().unwrap_or_default()
            )
            .into());
        }
        let workflow = plan["workflowId"].as_str().unwrap_or_default().to_owned();
        let mut selected = Vec::new();
        for id in cells {
            let cell = plan["cells"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["cellId"] == id.as_str())
                .ok_or_else(|| format!("Cellule étrangère à cette campagne : {id}"))?;
            if let Some(problem) = cell["problems"].as_array().and_then(|p| p.first()) {
                return Err(format!(
                    "Cellule {} : {}",
                    cell["position"],
                    problem.as_str().unwrap_or_default()
                )
                .into());
            }
            selected.push(cell.clone());
        }
        let mut drafts = Vec::new();
        for cell in selected {
            let edits: Vec<Edit> = serde_json::from_value(cell["edits"].clone())?;
            let prepared = self.prepare_job_with_characters(&workflow, &edits, node_info, &[])?;
            self.connection.execute(
                "INSERT INTO ab_cell_jobs(cell_id,job_id) VALUES(?1,?2)",
                params![cell["cellId"].as_str().unwrap_or_default(), prepared.job_id],
            )?;
            self.record_job_origin(
                &prepared.job_id,
                &crate::origins::OriginInput {
                    kind: "ab_cell".into(),
                    id: cell["cellId"].as_str().map(str::to_owned),
                },
            )?;
            drafts.push(json!({"cellId": cell["cellId"], "position": cell["position"], "prepared": prepared}));
        }
        Ok(json!({"campaign": campaign, "drafts": drafts}))
    }

    /// Registers the recorded outputs of every job prepared from a cell into the
    /// cell's image list (provenance rows only, never a file copy). Idempotent.
    pub fn sync_ab_results(&self, campaign: &str) -> Result<Value> {
        let rows: Vec<(String, String, String)> = self
            .connection
            .prepare(
                "SELECT l.cell_id,o.job_id,o.asset_id FROM ab_cell_jobs l JOIN ab_cells c ON c.id=l.cell_id
                 JOIN job_outputs o ON o.job_id=l.job_id
                 WHERE c.campaign_id=?1 AND NOT EXISTS(SELECT 1 FROM ab_cell_images i WHERE i.cell_id=l.cell_id AND i.asset_id=o.asset_id)
                 ORDER BY c.position,o.rowid",
            )?
            .query_map([campaign], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut registered = 0;
        for (cell, job, asset) in rows {
            self.register_ab_cell_image(campaign, &cell, &job, &asset)?;
            registered += 1;
        }
        Ok(json!({"registered": registered}))
    }

    /// Campaign defaults read from a recipe: reference workflow, exact prompts,
    /// seed, steps, cfg, sampler, checkpoint and plain LoRA weights.
    pub fn ab_spec_from_recipe(&self, recipe: &str) -> Result<Value> {
        let detail = self.recipe_detail(recipe)?;
        let content = &detail["content"];
        let workflow: Option<String> = self.connection.query_row(
            "SELECT workflow_id FROM recipes WHERE id=?",
            [recipe],
            |r| r.get(0),
        )?;
        let workflow = workflow
            .ok_or("Recette sans workflow local d'origine : importez d'abord le workflow")?;
        let mut prompts = Map::new();
        for (section, entry) in content["prompts"].as_object().into_iter().flatten() {
            prompts.insert(section.clone(), entry["text"].clone());
        }
        let parameter = |input: &str| {
            content["parameters"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["input"] == input || (input == "seed" && p["kind"] == "seed"))
                .map(|p| p["value"].clone())
                .unwrap_or(Value::Null)
        };
        let loras: Vec<Value> = content["loras"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["composite"] != json!(true))
            .map(|l| json!({"name": l["name"], "weightModel": l["strengthModel"], "weightClip": l["strengthClip"]}))
            .collect();
        Ok(json!({
            "name": format!("A/B · {}", detail["name"].as_str().unwrap_or("recette")),
            "workflow": workflow,
            "model": content["checkpoints"][0]["name"].as_str().unwrap_or_default(),
            "loras": loras,
            "prompts": prompts,
            "baseSeed": text(&parameter("seed")),
            "steps": parameter("steps"),
            "cfg": parameter("cfg"),
            "sampler": parameter("sampler_name"),
        }))
    }
}
