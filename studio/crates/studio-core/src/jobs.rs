//! Explicit job controls. Only an owned prompt is targeted; no global queue/interrupt calls.
use crate::{
    comfy::{queue_contains, Comfy},
    Database, Result,
};
use rusqlite::params;
use serde_json::{json, Value};
impl Database {
    pub(crate) fn snapshot_characters(
        &self,
        workflow: &str,
        people: &[crate::workflow::JobCharacter],
    ) -> Result<Vec<Value>> {
        if people.len() > 2 {
            return Err("Deux rôles de personnages maximum".into());
        }
        let fields = self.workflow(workflow)?.fields;
        let mut roles = std::collections::HashSet::new();
        let mut result = Vec::new();
        for person in people {
            if !["P1", "P2"].contains(&person.role.as_str()) || !roles.insert(&person.role) {
                return Err("Rôle de personnage invalide ou répété".into());
            }
            if !fields
                .iter()
                .any(|f| f.enabled && f.section.as_deref() == Some(&person.role))
            {
                return Err("Personnage sans mapping de prompt pour son rôle".into());
            }
            let (name, revision, identity): (String, u32, String) = self
                .connection
                .query_row(
                    "SELECT name,revision,identity_json FROM characters WHERE id=? AND archived=0",
                    [&person.character_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map_err(|_| {
                    format!(
                        "Personnage {} introuvable ou archivé : sélectionnez une fiche active",
                        person.role
                    )
                })?;
            let variant = if let Some(id) = &person.variant_id {
                let (name,revision,blocks):(String,u32,String)=self.connection.query_row("SELECT name,revision,prompt_blocks_json FROM character_variants WHERE id=?1 AND character_id=?2",params![id,person.character_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                json!({"id":id,"name":name,"revision":revision,"promptBlocks":serde_json::from_str::<Value>(&blocks)?})
            } else {
                Value::Null
            };
            result.push(json!({"role":person.role,"characterId":person.character_id,"name":name,"revision":revision,"identity":serde_json::from_str::<Value>(&identity)?,"variant":variant}));
        }
        Ok(result)
    }
    pub(crate) fn complete_job(&self, job: &str, outputs: &[Value]) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let changed=tx.execute("UPDATE generation_jobs SET state='succeeded',outputs_json=?2,failure_json=NULL WHERE id=?1 AND state IN ('queued','running','submitting','submission_unknown')",params![job,json!(outputs).to_string()])?;
        if changed == 1 {
            let snapshot: String = tx.query_row(
                "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
                [job],
                |r| r.get(0),
            )?;
            let snapshot: Value = serde_json::from_str(&snapshot)?;
            for person in snapshot["characters"].as_array().into_iter().flatten() {
                for output in outputs {
                    // This is the identity intended by the request, not a human visual validation.
                    if output["assetId"].as_str().is_some() {
                        tx.execute("INSERT OR IGNORE INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,'depicted','inferred')",params![output["assetId"].as_str(),person["characterId"].as_str()])?;
                    }
                }
            }
            for output in outputs {
                if output["assetId"].as_str().is_some() {
                    // The Saya review gate image is a preview copy of the pass: `output_kind`
                    // only allows saved/preview/unknown, and an unknown value was silently
                    // dropped by OR IGNORE, leaving the copy unlinked and visible in the gallery.
                    let kind = match output["kind"].as_str() {
                        Some("review") => "preview",
                        Some(k @ ("saved" | "preview")) => k,
                        _ => "unknown",
                    };
                    tx.execute("INSERT OR IGNORE INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES(?1,?2,?3,?4,?5)",params![job,output["assetId"].as_str(),output["nodeId"].as_str(),output["outputIndex"].as_i64(),kind])?;
                }
            }
            self.job_event(job, "outputs_recovered", &json!({"count":outputs.len()}))?;
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn job_event(&self, job: &str, kind: &str, payload: &Value) -> Result<()> {
        self.connection.execute(
            "INSERT INTO job_events(job_id,event_type,payload_json) VALUES(?1,?2,?3)",
            params![job, kind, payload.to_string()],
        )?;
        Ok(())
    }
    pub(crate) fn asset_generations(&self, asset: &str) -> Result<Vec<Value>> {
        let mut query=self.connection.prepare("SELECT j.id,j.workflow_revision_id,w.name,j.comfy_prompt_id,j.seed,j.request_snapshot_json,o.node_id,o.output_index,o.output_kind FROM job_outputs o JOIN generation_jobs j ON j.id=o.job_id JOIN workflow_revisions w ON w.id=j.workflow_revision_id WHERE o.asset_id=? ORDER BY j.created_at DESC")?;
        let rows=query.query_map([asset],|r|Ok(json!({"jobId":r.get::<_,String>(0)?,"workflowId":r.get::<_,String>(1)?,"workflow":r.get::<_,String>(2)?,"promptId":r.get::<_,Option<String>>(3)?,"seed":r.get::<_,String>(4)?,"requestJson":r.get::<_,String>(5)?,"nodeId":r.get::<_,Option<String>>(6)?,"outputIndex":r.get::<_,Option<i64>>(7)?,"kind":r.get::<_,String>(8)?})))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    pub fn job_details(&self, job: &str) -> Result<Value> {
        let snapshot: String = self.connection.query_row(
            "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )?;
        let mut query=self.connection.prepare("SELECT event_type,payload_json,observed_at FROM job_events WHERE job_id=? ORDER BY id DESC LIMIT 100")?;
        let rows = query.query_map([job], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (kind, payload, at) = row?;
            events.push(
                json!({"type":kind,"payload":serde_json::from_str::<Value>(&payload)?,"at":at}),
            );
        }
        // Keep JSON text exact: JS cannot round-trip arbitrary u64 seeds.
        Ok(json!({"requestJson":snapshot,"events":events}))
    }
    pub fn control_job(
        &self,
        job: &str,
        action: &str,
        confirmed: bool,
        client: &Comfy,
    ) -> Result<Value> {
        if !confirmed {
            return Err("Confirmation explicite requise pour ce job".into());
        }
        if !["cancel_queue", "interrupt"].contains(&action) {
            return Err("Action de file inconnue".into());
        }
        let (state, prompt): (String, Option<String>) = self.connection.query_row(
            "SELECT state,comfy_prompt_id FROM generation_jobs WHERE id=?",
            [job],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !["queued", "running", "submission_unknown"].contains(&state.as_str()) {
            return Err("Ce job n'est pas contrôlable dans cet état".into());
        }
        let prompt =
            prompt.ok_or("Identifiant moteur inconnu : synchroniser avant de contrôler")?;
        uuid::Uuid::parse_str(&prompt)?;
        let queue = client.get("/queue")?;
        let in_queue = queue_contains(
            &queue,
            if action == "interrupt" {
                "queue_running"
            } else {
                "queue_pending"
            },
            &prompt,
        )?;
        // After a disconnected run, an owned prompt can be absent from both
        // queue and history. A targeted delete is idempotent; the checks below
        // confirm absence before recording cancellation. Never interrupt globally.
        let cancel_missing = action == "cancel_queue"
            && state == "submission_unknown"
            && !queue_contains(&queue, "queue_running", &prompt)?;
        if !in_queue && !cancel_missing {
            return Err(
                "La position du job a changé ; synchroniser et confirmer la nouvelle action".into(),
            );
        }
        if action == "interrupt" {
            // Older engines ignore prompt_id and interrupt globally. Qualify versions explicitly.
            let stats = client.get("/system_stats")?;
            if stats
                .pointer("/system/comfyui_version")
                .and_then(Value::as_str)
                != Some("0.34.0")
            {
                return Err(
                    "Interruption ciblée non qualifiée pour cette version ComfyUI ; action refusée"
                        .into(),
                );
            }
        }
        self.job_event(
            job,
            "control_requested",
            &json!({"action":action,"promptId":prompt}),
        )?;
        let result = if action == "interrupt" {
            client.control("/interrupt", json!({"prompt_id":prompt}))
        } else {
            client.control("/queue", json!({"delete":[prompt]}))
        };
        if let Err(error) = result {
            self.job_event(
                job,
                "control_uncertain",
                &json!({"action":action,"message":error.to_string()}),
            )?;
            return Err(error);
        }
        self.job_event(
            job,
            "control_sent",
            &json!({"action":action,"promptId":prompt}),
        )?;
        if action == "cancel_queue" {
            let queue = client.get("/queue")?;
            if queue_contains(&queue, "queue_running", &prompt)?
                || queue_contains(&queue, "queue_pending", &prompt)?
            {
                return Ok(
                    json!({"confirmed":false,"message":"Le job est encore annoncé par ComfyUI. Aucune interruption n'a été envoyée."}),
                );
            }
            let history = client.get(&format!("/history/{prompt}"))?;
            if !history.is_object() {
                return Err("Historique ComfyUI invalide ; annulation non confirmée".into());
            }
            if history.get(&prompt).is_none() {
                let changed=self.connection.execute("UPDATE generation_jobs SET state='cancelled' WHERE id=? AND state IN ('queued','submission_unknown')",[job])?;
                if changed == 1 {
                    self.job_event(job,"cancel_observed",&json!({"promptId":prompt,"evidence":"absent_queue_and_history_after_delete"}))?;
                    return Ok(
                        json!({"confirmed":true,"message":"Job retiré de la file ; absence vérifiée dans la file et l'historique."}),
                    );
                }
            }
        }
        Ok(
            json!({"confirmed":false,"message":"Demande transmise ; attendre l'état confirmé par le moteur."}),
        )
    }
}
