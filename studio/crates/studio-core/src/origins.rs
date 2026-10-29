//! Verifiable origin of prepared jobs (recipe, scene, A/B cell, module or
//! manual) and read-only provenance views joining origins, the generation
//! tree and recorded outputs. A historical branch never implies that a
//! computation can be resumed: `resumable` comes only from the tree's own
//! verified checkpoint rules.
use crate::{Database, Result};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OriginInput {
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
}

impl Database {
    /// Records where a prepared job comes from. The source must exist now;
    /// its label and version are frozen at that moment.
    pub fn record_job_origin(&self, job: &str, origin: &OriginInput) -> Result<()> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id=?)",
            [job],
            |r| r.get(0),
        )?;
        if !exists {
            return Err("Job inconnu : origine non enregistrée".into());
        }
        let (label, version) = self.resolve_origin(origin)?;
        self.connection.execute(
            "INSERT INTO job_origins(job_id,kind,source_id,source_label,source_version) VALUES(?1,?2,?3,?4,?5)",
            params![job, origin.kind, origin.id.as_deref(), label, version],
        )?;
        Ok(())
    }

    /// Validates an origin without writing anything.
    pub fn check_job_origin(&self, origin: &OriginInput) -> Result<()> {
        self.resolve_origin(origin).map(|_| ())
    }

    fn resolve_origin(&self, origin: &OriginInput) -> Result<(String, Option<i64>)> {
        let source = origin.id.as_deref();
        let (label, version): (String, Option<i64>) = match (origin.kind.as_str(), source) {
            ("manual", None) => (String::new(), None),
            ("recipe", Some(id)) => self
                .connection
                .query_row("SELECT name,version FROM recipes WHERE id=?", [id], |r| Ok((r.get(0)?, Some(r.get(1)?))))
                .optional()?
                .ok_or("Recette d'origine introuvable")?,
            ("scene", Some(id)) => self
                .connection
                .query_row("SELECT name FROM scene_manifests WHERE id=?", [id], |r| Ok((r.get(0)?, None)))
                .optional()?
                .ok_or("Scène d'origine introuvable")?,
            ("ab_cell", Some(id)) => self
                .connection
                .query_row(
                    "SELECT c.name || ' · cellule ' || (x.position+1) FROM ab_cells x JOIN ab_campaigns c ON c.id=x.campaign_id WHERE x.id=?",
                    [id],
                    |r| Ok((r.get(0)?, None)),
                )
                .optional()?
                .ok_or("Cellule A/B d'origine introuvable")?,
            ("module", Some(id)) => self
                .connection
                .query_row("SELECT name,current_version FROM workflow_modules WHERE id=?", [id], |r| Ok((r.get(0)?, Some(r.get(1)?))))
                .optional()?
                .ok_or("Module d'origine introuvable")?,
            ("manual", Some(_)) => return Err("Origine manuelle : aucune source attendue".into()),
            (_, None) => return Err("Origine : identifiant de source requis".into()),
            _ => return Err("Origine inconnue (recipe, scene, ab_cell, module, manual)".into()),
        };
        Ok((label, version))
    }

    /// Provenance of one job: state, workflow, origin, place in the tree and
    /// recorded outputs. Read-only.
    pub fn job_provenance(&self, job: &str) -> Result<Value> {
        let (state, workflow, workflow_name, created, seed): (String, String, Option<String>, String, String) = self
            .connection
            .query_row(
                "SELECT j.state,j.workflow_revision_id,w.name,j.created_at,j.seed FROM generation_jobs j LEFT JOIN workflow_revisions w ON w.id=j.workflow_revision_id WHERE j.id=?",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?
            .ok_or("Job inconnu")?;
        let origin = self
            .connection
            .query_row(
                "SELECT kind,source_id,source_label,source_version,created_at FROM job_origins WHERE job_id=?",
                [job],
                |r| {
                    Ok(json!({"kind": r.get::<_, String>(0)?, "id": r.get::<_, Option<String>>(1)?,
                        "label": r.get::<_, String>(2)?, "version": r.get::<_, Option<i64>>(3)?, "at": r.get::<_, String>(4)?}))
                },
            )
            .optional()?;
        let ab_cell = self
            .connection
            .query_row(
                "SELECT c.id,c.name,x.id,x.position FROM ab_cell_jobs l JOIN ab_cells x ON x.id=l.cell_id JOIN ab_campaigns c ON c.id=x.campaign_id WHERE l.job_id=?",
                [job],
                |r| Ok(json!({"campaign": r.get::<_, String>(0)?, "campaignName": r.get::<_, String>(1)?, "cell": r.get::<_, String>(2)?, "position": r.get::<_, i64>(3)?})),
            )
            .optional()?;
        let outputs: Vec<Value> = self
            .connection
            .prepare("SELECT o.asset_id,a.relative_path,o.node_id,o.output_index,o.output_kind FROM job_outputs o LEFT JOIN assets a ON a.id=o.asset_id WHERE o.job_id=? ORDER BY o.rowid")?
            .query_map([job], |r| {
                Ok(json!({"asset": r.get::<_, String>(0)?, "path": r.get::<_, Option<String>>(1)?, "nodeId": r.get::<_, Option<String>>(2)?,
                    "outputIndex": r.get::<_, Option<i64>>(3)?, "kind": r.get::<_, String>(4)?}))
            })?
            .collect::<rusqlite::Result<_>>()?;
        // Pipeline passes carry their orchestration context in the frozen
        // request snapshot (run, step, pipeline): surfaced, never inferred.
        let snapshot: String = self.connection.query_row(
            "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )?;
        let orchestration = serde_json::from_str::<Value>(&snapshot)
            .ok()
            .and_then(|v| v.get("orchestration").cloned())
            .filter(|v| v.is_object());
        let path = self.generation_path(job)?["path"].clone();
        let branches = self.generation_branches(job)?["branches"]
            .as_array()
            .map_or(0, Vec::len);
        Ok(json!({
            "job": job, "state": state, "seed": seed, "createdAt": created,
            "workflow": {"id": workflow, "name": workflow_name},
            "origin": origin, "abCell": ab_cell, "pipeline": orchestration, "outputs": outputs,
            "path": path, "branchCount": branches,
        }))
    }

    /// Every job that recorded this image as an output, with its provenance.
    /// An imported image without a job has an empty list (honest unknown).
    pub fn asset_provenance(&self, asset: &str) -> Result<Value> {
        let jobs: Vec<String> = self
            .connection
            .prepare("SELECT job_id FROM job_outputs WHERE asset_id=? ORDER BY rowid")?
            .query_map([asset], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let provenance = jobs
            .iter()
            .map(|job| self.job_provenance(job))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"asset": asset, "jobs": provenance}))
    }
}
