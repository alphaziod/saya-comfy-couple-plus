//! Generation tree. Relations reference existing rows only; historical filiation is always true,
//! while resumability is reported from the existing IMAGE checkpoint tables and never implies a latent.
use crate::{Database, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_LABEL: usize = 300;
const MAX_NODES: usize = 10_000;
const MAX_DIFFS: usize = 200;
pub(crate) const RELATIONS: [&str; 6] = [
    "variant",
    "detailer",
    "upscale",
    "seed_change",
    "correction",
    "checkpoint_restore",
];

#[derive(Clone)]
struct Relation {
    id: String,
    parent: String,
    child: Option<String>,
    checkpoint: Option<String>,
    kind: String,
    label: String,
    state: String,
    created_at: String,
    child_exists: bool,
    checkpoint_status: Option<String>,
    checkpoint_capability: Option<String>,
    checkpoint_kind: Option<String>,
    checkpoint_asset: Option<String>,
    checkpoint_review: Option<String>,
}

impl Relation {
    fn info(&self) -> Value {
        json!({
            "id": self.id,
            "kind": self.kind,
            "label": self.label,
            "state": self.state,
            "createdAt": self.created_at,
            "checkpointId": self.checkpoint,
            "checkpoint": self.checkpoint_info()
        })
    }
    // Human validation and verified capability stay distinct, and only an IMAGE boundary is ever
    // reported resumable: no PNG is presented as a restorable latent or sampler state.
    fn checkpoint_info(&self) -> Value {
        match &self.checkpoint {
            Some(id) => {
                let capability = self
                    .checkpoint_capability
                    .as_deref()
                    .unwrap_or("unverified");
                let status = self.checkpoint_status.as_deref().unwrap_or("candidate");
                let resumable = status == "validated"
                    && capability == "verified"
                    && self.checkpoint_kind.as_deref() == Some("image_boundary");
                json!({
                    "id": id,
                    "status": status,
                    "resumeCapability": capability,
                    "review": self.checkpoint_review,
                    "kind": self.checkpoint_kind,
                    "assetId": self.checkpoint_asset,
                    "resumable": resumable,
                    "resumeKind": self.checkpoint_kind.clone().unwrap_or_else(|| "image_boundary".into())
                })
            }
            None => Value::Null,
        }
    }
}

fn load_relations(connection: &rusqlite::Connection) -> Result<Vec<Relation>> {
    let mut query = connection.prepare(
        "SELECT r.id,r.parent_job_id,r.child_job_id,r.parent_checkpoint_id,r.relation,r.label,r.state,r.created_at,
        (cj.id IS NOT NULL),
        pa.status,pa.resume_capability,pa.kind,pa.asset_id,
        (SELECT hr.decision FROM human_reviews hr WHERE hr.artifact_id=r.parent_checkpoint_id ORDER BY hr.reviewed_at DESC,hr.rowid DESC LIMIT 1)
        FROM generation_relations r
        LEFT JOIN generation_jobs cj ON cj.id=r.child_job_id
        LEFT JOIN workflow_revisions w ON w.id=cj.workflow_revision_id
        LEFT JOIN pass_artifacts pa ON pa.id=r.parent_checkpoint_id
        ORDER BY r.created_at,r.id",
    )?;
    let rows = query.query_map([], |r| {
        Ok(Relation {
            id: r.get(0)?,
            parent: r.get(1)?,
            child: r.get(2)?,
            checkpoint: r.get(3)?,
            kind: r.get(4)?,
            label: r.get(5)?,
            state: r.get(6)?,
            created_at: r.get(7)?,
            child_exists: r.get(8)?,
            checkpoint_status: r.get(9)?,
            checkpoint_capability: r.get(10)?,
            checkpoint_kind: r.get(11)?,
            checkpoint_asset: r.get(12)?,
            checkpoint_review: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn orphan_node(job: Option<&str>, parent: Value, reason: &str) -> Value {
    json!({
        "jobId": job,
        "parent": parent,
        "orphan": true,
        "orphanReason": reason,
        "children": []
    })
}

struct Forest {
    relations: Vec<Relation>,
    by_parent: HashMap<String, Vec<usize>>,
    by_child: HashMap<String, usize>,
}

impl Forest {
    fn load(database: &Database) -> Result<Self> {
        let relations = load_relations(&database.connection)?;
        let mut by_parent: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_child: HashMap<String, usize> = HashMap::new();
        for (index, relation) in relations.iter().enumerate() {
            by_parent
                .entry(relation.parent.clone())
                .or_default()
                .push(index);
            if let Some(child) = &relation.child {
                by_child.insert(child.clone(), index);
            }
        }
        Ok(Self {
            relations,
            by_parent,
            by_child,
        })
    }
    fn parent_of(&self, job: &str) -> Option<&Relation> {
        self.by_child.get(job).map(|index| &self.relations[*index])
    }
}

impl Database {
    fn validate_relation_kind(&self, relation: &str) -> Result<()> {
        if !RELATIONS.contains(&relation) {
            return Err("Type de relation inconnu".into());
        }
        Ok(())
    }
    fn validate_label(&self, label: &str) -> Result<()> {
        if label.len() > MAX_LABEL {
            return Err("Étiquette trop longue".into());
        }
        Ok(())
    }
    fn require_job(&self, job: &str, role: &str) -> Result<()> {
        let found: Option<String> = self
            .connection
            .query_row("SELECT id FROM generation_jobs WHERE id=?1", [job], |r| {
                r.get(0)
            })
            .optional()?;
        if found.is_none() {
            return Err(format!("Génération {role} introuvable").into());
        }
        Ok(())
    }
    fn checkpoint_parent(&self, checkpoint: &str) -> Result<String> {
        self.connection
            .query_row(
                "SELECT p.job_id FROM pass_artifacts a JOIN pass_attempts p ON p.id=a.pass_attempt_id WHERE a.id=?1",
                [checkpoint],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| "Checkpoint introuvable".into())
    }
    fn refuse_cycle(&self, parent: &str, child: &str) -> Result<()> {
        if parent == child {
            return Err("Cycle refusé : une génération ne peut pas être son propre parent".into());
        }
        let forest = Forest::load(self)?;
        let mut current = parent.to_owned();
        for _ in 0..MAX_NODES {
            let Some(relation) = forest.parent_of(&current) else {
                return Ok(());
            };
            if relation.parent == child {
                return Err("Cycle refusé : cette génération est déjà un ancêtre".into());
            }
            current = relation.parent.clone();
        }
        Err("Chaîne de filiation trop profonde ; insertion refusée".into())
    }
    fn child_is_free(&self, child: &str) -> Result<()> {
        match Forest::load(self)?.parent_of(child) {
            None => Ok(()),
            Some(relation) => {
                Err(format!("Cette génération a déjà un parent ({})", relation.id).into())
            }
        }
    }

    /// Records a parent/child relation between two existing generations. References only; no copy.
    pub fn record_generation_relation(
        &self,
        parent: &str,
        child: &str,
        relation: &str,
        label: &str,
        checkpoint: Option<&str>,
    ) -> Result<String> {
        self.validate_relation_kind(relation)?;
        self.validate_label(label)?;
        self.require_job(parent, "parente")?;
        self.require_job(child, "enfant")?;
        if parent == child {
            return Err("Cycle refusé : une génération ne peut pas être son propre parent".into());
        }
        self.child_is_free(child)?;
        if let Some(checkpoint) = checkpoint {
            let owner = self.checkpoint_parent(checkpoint)?;
            if owner != parent {
                return Err("Ce checkpoint appartient à une autre génération".into());
            }
        }
        self.refuse_cycle(parent, child)?;
        let id = uuid::Uuid::new_v4().to_string();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO generation_relations(id,parent_job_id,child_job_id,parent_checkpoint_id,relation,label,state) VALUES(?1,?2,?3,?4,?5,?6,'linked')",
            params![id, parent, child, checkpoint, relation, label],
        )?;
        tx.execute(
            "INSERT INTO job_events(job_id,event_type,payload_json) VALUES(?1,'tree_relation_recorded',?2)",
            params![child, json!({"relationId":id,"parent":parent,"kind":relation,"checkpoint":checkpoint}).to_string()],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Records a prepared (never executed) branch from a saved state or an existing generation.
    pub fn plan_branch_from_saved_state(
        &self,
        parent: &str,
        relation: &str,
        label: &str,
        checkpoint: Option<&str>,
    ) -> Result<String> {
        self.validate_relation_kind(relation)?;
        self.validate_label(label)?;
        self.require_job(parent, "parente")?;
        if let Some(checkpoint) = checkpoint {
            let owner = self.checkpoint_parent(checkpoint)?;
            if owner != parent {
                return Err("Ce checkpoint appartient à une autre génération".into());
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO generation_relations(id,parent_job_id,child_job_id,parent_checkpoint_id,relation,label,state) VALUES(?1,?2,NULL,?3,?4,?5,'planned')",
            params![id, parent, checkpoint, relation, label],
        )?;
        tx.execute(
            "INSERT INTO job_events(job_id,event_type,payload_json) VALUES(?1,'tree_branch_planned',?2)",
            params![parent, json!({"relationId":id,"kind":relation,"checkpoint":checkpoint}).to_string()],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// Attaches an existing generation as the child of a previously planned branch.
    pub fn link_generation_child(&self, relation: &str, child: &str) -> Result<()> {
        self.require_job(child, "enfant")?;
        let (parent, state): (String, String) = self.connection.query_row(
            "SELECT parent_job_id,state FROM generation_relations WHERE id=?1",
            [relation],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if state != "planned" {
            return Err("Seule une branche planifiée peut recevoir un enfant".into());
        }
        if parent == child {
            return Err("Cycle refusé : une génération ne peut pas être son propre parent".into());
        }
        self.child_is_free(child)?;
        self.refuse_cycle(&parent, child)?;
        let tx = self.connection.unchecked_transaction()?;
        if tx.execute(
            "UPDATE generation_relations SET child_job_id=?2,state='linked' WHERE id=?1 AND state='planned'",
            params![relation, child],
        )? != 1
        {
            return Err("Branche planifiée introuvable".into());
        }
        tx.execute(
            "INSERT INTO job_events(job_id,event_type,payload_json) VALUES(?1,'tree_branch_linked',?2)",
            params![child, json!({"relationId":relation,"parent":parent}).to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// One tree node: full job data, or an orphan marker when the row is gone.
    fn node_from_table(&self, job: &str, parent: Value) -> Result<Value> {
        type JobRow = (String, String, String, String, Option<String>, String, i64);
        let found: Option<JobRow> = self
            .connection
            .query_row(
                "SELECT j.id,j.state,j.seed,j.workflow_revision_id,w.name,j.created_at,
                (SELECT count(*) FROM job_outputs o WHERE o.job_id=j.id)
                FROM generation_jobs j JOIN workflow_revisions w ON w.id=j.workflow_revision_id WHERE j.id=?1",
                [job],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, state, seed, workflow, workflow_name, created, outputs)) = found else {
            return Ok(orphan_node(
                Some(job),
                parent,
                "Génération absente de la base ; filiation conservée par la relation.",
            ));
        };
        Ok(json!({
            "jobId": id,
            "parent": parent,
            "orphan": false,
            "orphanReason": Value::Null,
            "state": state,
            "seed": seed,
            "workflowId": workflow,
            "workflow": workflow_name,
            "createdAt": created,
            "outputs": outputs
        }))
    }

    /// Complete tree from a root: descendants, planned branches, and every relation whose
    /// parent generation is missing (visible orphans, no crash).
    pub fn generation_tree(&self, root: &str) -> Result<Value> {
        self.require_job(root, "racine")?;
        let forest = Forest::load(self)?;
        let mut budget = MAX_NODES;
        let mut expanded = HashSet::new();
        let root_node = self.build_tree_node(root, &forest, &mut budget, &mut expanded)?;
        let mut orphans = Vec::new();
        for relation in &forest.relations {
            let present: bool = self
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id=?1)",
                    [&relation.parent],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if !present {
                orphans.push(json!({
                    "relation": relation.info(),
                    "parentJobId": relation.parent,
                    "parentPresent": false
                }));
            }
        }
        Ok(json!({"root": root_node, "orphanRelations": orphans}))
    }

    fn build_tree_node(
        &self,
        job: &str,
        forest: &Forest,
        budget: &mut usize,
        expanded: &mut HashSet<String>,
    ) -> Result<Value> {
        if *budget == 0 {
            return Err("Arbre trop vaste ; affichage refusé".into());
        }
        *budget -= 1;
        let relation = forest.parent_of(job);
        let mut node =
            self.node_from_table(job, relation.map(Relation::info).unwrap_or(Value::Null))?;
        let mut children = Vec::new();
        if expanded.insert(job.to_owned()) {
            for index in forest.by_parent.get(job).into_iter().flatten() {
                let child_relation = &forest.relations[*index];
                match &child_relation.child {
                    Some(child) => {
                        if expanded.contains(child) {
                            children.push(orphan_node(
                                Some(child),
                                child_relation.info(),
                                "Cycle détecté dans les relations ; sous-arbre non déplié.",
                            ));
                            continue;
                        }
                        children.push(self.build_tree_node(child, forest, budget, expanded)?);
                    }
                    None => children.push(json!({
                        "planned": true,
                        "jobId": Value::Null,
                        "parent": child_relation.info(),
                        "orphan": false,
                        "orphanReason": Value::Null,
                        "children": []
                    })),
                }
            }
        }
        if let Value::Object(map) = &mut node {
            map.insert("children".into(), Value::Array(children));
        }
        Ok(node)
    }

    /// Path from the origin down to the requested generation, including orphaned ancestors.
    pub fn generation_path(&self, job: &str) -> Result<Value> {
        let forest = Forest::load(self)?;
        if forest.parent_of(job).is_none() && !forest.by_parent.contains_key(job) {
            self.require_job(job, "demandée")?;
        }
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = job.to_owned();
        for _ in 0..MAX_NODES {
            if !seen.insert(current.clone()) {
                return Err("Cycle détecté dans les relations ; chemin refusé".into());
            }
            let relation = forest.parent_of(&current);
            chain.push((
                current.clone(),
                relation.map(Relation::info).unwrap_or(Value::Null),
                relation.map(|r| r.child_exists).unwrap_or(false),
            ));
            match relation.map(|r| r.parent.clone()) {
                Some(parent) => current = parent,
                None => break,
            }
        }
        chain.reverse();
        let mut nodes = Vec::new();
        for (job_id, parent, _) in &chain {
            nodes.push(self.node_from_table(job_id, parent.clone())?);
        }
        Ok(json!({"path": nodes}))
    }

    /// Direct branches (children and planned branches) of one generation.
    pub fn generation_branches(&self, job: &str) -> Result<Value> {
        let forest = Forest::load(self)?;
        if !forest.by_parent.contains_key(job) {
            self.require_job(job, "demandée")?;
        }
        let mut branches = Vec::new();
        for index in forest.by_parent.get(job).into_iter().flatten() {
            let relation = &forest.relations[*index];
            let child = match &relation.child {
                Some(child) => self.node_from_table(child, relation.info())?,
                None => json!({"planned": true, "jobId": null}),
            };
            branches.push(json!({"relation": relation.info(), "child": child}));
        }
        Ok(json!({"branches": branches}))
    }

    /// Exact comparison of two branches: seeds, workflow, snapshot JSON paths, provenance outputs.
    pub fn generation_compare(&self, left: &str, right: &str) -> Result<Value> {
        self.require_job(left, "de gauche")?;
        self.require_job(right, "de droite")?;
        let row = |job: &str| -> Result<(String, String, String, String, String, String)> {
            Ok(self.connection.query_row(
                "SELECT j.seed,j.workflow_revision_id,w.name,j.state,j.created_at,j.request_snapshot_json
                FROM generation_jobs j JOIN workflow_revisions w ON w.id=j.workflow_revision_id WHERE j.id=?1",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?)
        };
        let (left_seed, left_workflow, left_name, left_state, left_created, left_snapshot) =
            row(left)?;
        let (right_seed, right_workflow, right_name, right_state, right_created, right_snapshot) =
            row(right)?;
        let left_value: Value = serde_json::from_str(&left_snapshot)?;
        let right_value: Value = serde_json::from_str(&right_snapshot)?;
        let mut diffs = Vec::new();
        diff_json("$", &left_value, &right_value, &mut diffs);
        let truncated = diffs.len() > MAX_DIFFS;
        diffs.truncate(MAX_DIFFS);
        let outputs = |job: &str| -> Result<Vec<Value>> {
            let mut query = self.connection.prepare(
                "SELECT asset_id,node_id,output_index,output_kind FROM job_outputs WHERE job_id=?1 ORDER BY asset_id",
            )?;
            let rows = query.query_map([job], |r| {
                Ok(json!({
                    "assetId": r.get::<_, String>(0)?,
                    "nodeId": r.get::<_, Option<String>>(1)?,
                    "outputIndex": r.get::<_, Option<i64>>(2)?,
                    "kind": r.get::<_, String>(3)?
                }))
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        let left_outputs = outputs(left)?;
        let right_outputs = outputs(right)?;
        let characters = |job: &str| -> Result<Vec<Value>> {
            let mut query = self.connection.prepare(
                "SELECT role,character_id,variant_id FROM job_characters WHERE job_id=?1 ORDER BY role",
            )?;
            let rows = query.query_map([job], |r| {
                Ok(json!({
                    "role": r.get::<_, String>(0)?,
                    "characterId": r.get::<_, String>(1)?,
                    "variantId": r.get::<_, Option<String>>(2)?
                }))
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        };
        let left_characters = characters(left)?;
        let right_characters = characters(right)?;
        Ok(json!({
            "left": left,
            "right": right,
            "seeds": {"left": left_seed, "right": right_seed, "equal": left_seed == right_seed},
            "workflows": {"left": left_workflow, "leftName": left_name, "right": right_workflow, "rightName": right_name, "equal": left_workflow == right_workflow},
            "states": {"left": left_state, "right": right_state, "equal": left_state == right_state},
            "createdAt": {"left": left_created, "right": right_created},
            "snapshotDiffs": diffs,
            "snapshotDiffsTruncated": truncated,
            "outputs": {"left": left_outputs, "right": right_outputs, "equal": left_outputs == right_outputs},
            "characters": {"left": left_characters, "right": right_characters, "equal": left_characters == right_characters},
            "commonAncestor": self.common_ancestor(left, right)?
        }))
    }

    fn common_ancestor(&self, left: &str, right: &str) -> Result<Option<String>> {
        let forest = Forest::load(self)?;
        let mut ancestors = BTreeMap::new();
        let mut current = left.to_owned();
        for _ in 0..MAX_NODES {
            if ancestors.insert(current.clone(), ancestors.len()).is_some() {
                break; // Defensive against externally corrupted relations.
            }
            match forest.parent_of(&current) {
                Some(relation) => current = relation.parent.clone(),
                None => break,
            }
        }
        let mut current = right.to_owned();
        for _ in 0..MAX_NODES {
            if ancestors.contains_key(&current) {
                return Ok(Some(current));
            }
            match forest.parent_of(&current) {
                Some(relation) => current = relation.parent.clone(),
                None => return Ok(None),
            }
        }
        Ok(None)
    }
}

fn push_diff(diffs: &mut Vec<Value>, path: &str, left: Value, right: Value) {
    if diffs.len() > MAX_DIFFS {
        return; // One extra entry flags truncation without losing the bound.
    }
    diffs.push(json!({"path": path, "left": left, "right": right}));
}

fn diff_json(path: &str, left: &Value, right: &Value, diffs: &mut Vec<Value>) {
    if left == right || diffs.len() > MAX_DIFFS {
        return;
    }
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: HashSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let child = format!("{path}.{key}");
                match (a.get(key), b.get(key)) {
                    (Some(l), Some(r)) => diff_json(&child, l, r, diffs),
                    (Some(l), None) => push_diff(diffs, &child, json!(l), Value::Null),
                    (None, Some(r)) => push_diff(diffs, &child, Value::Null, json!(r)),
                    (None, None) => unreachable!(),
                }
                if diffs.len() > MAX_DIFFS {
                    return;
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for index in 0..a.len().max(b.len()) {
                let child = format!("{path}[{index}]");
                match (a.get(index), b.get(index)) {
                    (Some(l), Some(r)) => diff_json(&child, l, r, diffs),
                    (Some(l), None) => push_diff(diffs, &child, json!(l), Value::Null),
                    (None, Some(r)) => push_diff(diffs, &child, Value::Null, json!(r)),
                    (None, None) => unreachable!(),
                }
                if diffs.len() > MAX_DIFFS {
                    return;
                }
            }
        }
        _ => push_diff(diffs, path, json!(left), json!(right)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use rusqlite::params;

    const SNAPSHOT: &str = r#"{"prompt":{"1":{"class_type":"KSampler","inputs":{"seed":1,"steps":20}}},"resources":[]}"#;

    fn workflow(db: &Database, id: &str) {
        db.connection
            .execute(
                "INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES(?1,?1,'hash','comfy_api','{}')",
                [id],
            )
            .unwrap();
    }
    fn job(db: &Database, id: &str, seed: &str, snapshot: &str) {
        db.connection
            .execute(
                "INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state) VALUES(?1,'w',?2,?3,'succeeded')",
                params![id, seed, snapshot],
            )
            .unwrap();
    }
    fn relation_count(db: &Database) -> i64 {
        db.connection
            .query_row("SELECT count(*) FROM generation_relations", [], |r| {
                r.get(0)
            })
            .unwrap()
    }
    fn find_node(node: &Value, job: &str) -> Option<Value> {
        if node["jobId"].as_str() == Some(job) {
            return Some(node.clone());
        }
        node["children"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|c| find_node(c, job))
    }
    fn node_by_id(node: &Value, job: &str) -> Value {
        find_node(node, job).unwrap_or_else(|| panic!("node {job} absent from tree"))
    }
    fn checkpoint(
        db: &Database,
        id: &str,
        owner: &str,
        kind: &str,
        status: &str,
        capability: &str,
    ) {
        db.connection
            .execute(
                "INSERT INTO pass_attempts(id,job_id,phase_key,attempt,state) VALUES(?1,?2,?3,1,'awaiting_review')",
                params![format!("{id}-attempt"), owner, format!("SaveImage:{id}")],
            )
            .unwrap();
        db.connection
            .execute(
                "INSERT INTO pass_artifacts(id,pass_attempt_id,kind,status,resume_capability,manifest_json) VALUES(?1,?2,?3,?4,?5,'{}')",
                params![id, format!("{id}-attempt"), kind, status, capability],
            )
            .unwrap();
    }

    #[test]
    fn relations_validate_jobs_kinds_labels_and_ownership() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        job(&db, "root", "1", SNAPSHOT);
        job(&db, "child", "2", SNAPSHOT);
        job(&db, "other", "3", SNAPSHOT);
        checkpoint(
            &db,
            "ck-other",
            "other",
            "image_boundary",
            "candidate",
            "unverified",
        );
        assert!(db
            .record_generation_relation("missing", "child", "variant", "", None)
            .is_err());
        assert!(db
            .record_generation_relation("root", "missing", "variant", "", None)
            .is_err());
        assert!(db
            .record_generation_relation("root", "child", "magic", "", None)
            .is_err());
        assert!(db
            .record_generation_relation("root", "child", "variant", &"x".repeat(301), None)
            .is_err());
        assert!(db
            .record_generation_relation("root", "root", "variant", "", None)
            .is_err());
        // A checkpoint must belong to the parent generation it is attached to.
        assert!(db
            .record_generation_relation("root", "child", "variant", "", Some("ck-other"))
            .is_err());
        let id = db
            .record_generation_relation("root", "child", "variant", "décor alternatif", None)
            .unwrap();
        assert_eq!(relation_count(&db), 1);
        // One parent per child: the tree shape cannot fork under a single child.
        assert!(db
            .record_generation_relation("other", "child", "upscale", "", None)
            .is_err());
        assert_eq!(relation_count(&db), 1);
        let branches = db.generation_branches("root").unwrap();
        assert_eq!(branches["branches"].as_array().unwrap().len(), 1);
        assert_eq!(branches["branches"][0]["relation"]["id"], id.as_str());
        assert_eq!(branches["branches"][0]["relation"]["kind"], "variant");
        assert_eq!(branches["branches"][0]["child"]["jobId"], "child");
    }

    #[test]
    fn cycles_are_refused_at_insertion_and_through_planned_links() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        for id in ["root", "a", "b"] {
            job(&db, id, "1", SNAPSHOT);
        }
        db.record_generation_relation("root", "a", "variant", "", None)
            .unwrap();
        db.record_generation_relation("a", "b", "detailer", "", None)
            .unwrap();
        // Closing any loop back to an ancestor is refused, whatever the entry point.
        assert!(db
            .record_generation_relation("b", "root", "upscale", "", None)
            .is_err());
        assert!(db
            .record_generation_relation("b", "a", "upscale", "", None)
            .is_err());
        assert!(db
            .record_generation_relation("a", "root", "upscale", "", None)
            .is_err());
        assert_eq!(relation_count(&db), 2);
        // Planned branches go through the same refusal when linked later.
        let planned = db
            .plan_branch_from_saved_state("b", "seed_change", "", None)
            .unwrap();
        assert!(db.link_generation_child(&planned, "root").is_err());
        assert!(db.link_generation_child(&planned, "a").is_err());
        let tree = db.generation_tree("root").unwrap();
        let planned_node = node_by_id(&tree["root"], "b")["children"][0].clone();
        assert_eq!(planned_node["planned"], true);
        assert_eq!(planned_node["jobId"], Value::Null);
        // Only a planned branch can receive a child, and the link itself is cycle-checked.
        let child = "c";
        job(&db, child, "9", SNAPSHOT);
        db.link_generation_child(&planned, child).unwrap();
        assert!(db.link_generation_child(&planned, child).is_err());
        let branches = db.generation_branches("b").unwrap();
        assert_eq!(branches["branches"][0]["child"]["jobId"], child);
        assert_eq!(branches["branches"][0]["relation"]["state"], "linked");
    }

    #[test]
    fn tree_navigates_multi_branch_forest_with_paths() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        for id in ["root", "variant", "detailer", "upscale", "correct"] {
            job(&db, id, "1", SNAPSHOT);
        }
        db.record_generation_relation("root", "variant", "variant", "", None)
            .unwrap();
        db.record_generation_relation("root", "detailer", "detailer", "", None)
            .unwrap();
        db.record_generation_relation("root", "correct", "correction", "", None)
            .unwrap();
        db.record_generation_relation("detailer", "upscale", "upscale", "", None)
            .unwrap();
        let tree = db.generation_tree("root").unwrap();
        let root = &tree["root"];
        assert_eq!(root["children"].as_array().unwrap().len(), 3);
        assert_eq!(node_by_id(root, "upscale")["state"], "succeeded");
        assert_eq!(node_by_id(root, "upscale")["seed"], "1");
        assert_eq!(node_by_id(root, "upscale")["parent"]["kind"], "upscale");
        let path = db.generation_path("upscale").unwrap();
        let ids: Vec<&str> = path["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["jobId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["root", "detailer", "upscale"]);
        // An isolated generation is a valid one-node tree.
        let lone = db.generation_tree("variant").unwrap();
        assert_eq!(lone["root"]["jobId"], "variant");
        assert_eq!(lone["root"]["children"].as_array().unwrap().len(), 0);
        assert!(db.generation_tree("missing").is_err());
    }

    #[test]
    fn relations_survive_reopening_the_database() {
        let directory = std::env::temp_dir().join(format!(
            "studio-tree-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("studio.sqlite3");
        {
            let db = Database::open(&path).unwrap();
            workflow(&db, "w");
            for id in ["root", "a"] {
                job(&db, id, "1", SNAPSHOT);
            }
            checkpoint(&db, "ck", "root", "image_boundary", "validated", "verified");
            db.record_generation_relation("root", "a", "variant", "garder", Some("ck"))
                .unwrap();
            db.plan_branch_from_saved_state("a", "upscale", "plus tard", None)
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let tree = db.generation_tree("root").unwrap();
        assert_eq!(tree["root"]["children"].as_array().unwrap().len(), 1);
        let child = &tree["root"]["children"][0];
        assert_eq!(child["jobId"], "a");
        // The root→a relation carries the validated IMAGE checkpoint: resumable and honest.
        assert_eq!(child["parent"]["checkpoint"]["resumable"], true);
        assert_eq!(
            child["parent"]["checkpoint"]["resumeCapability"],
            "verified"
        );
        assert_eq!(
            child["parent"]["checkpoint"]["resumeKind"],
            "image_boundary"
        );
        // The planned branch prepared on 'a' survived with no child and no checkpoint claim.
        assert_eq!(child["children"].as_array().unwrap().len(), 1);
        let planned = &child["children"][0];
        assert_eq!(planned["planned"], true);
        assert_eq!(planned["jobId"], Value::Null);
        assert_eq!(planned["parent"]["state"], "planned");
        assert_eq!(planned["parent"]["checkpoint"], Value::Null);
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_ancestors_stay_visible_as_orphans() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        for id in ["root", "a", "b"] {
            job(&db, id, "1", SNAPSHOT);
        }
        db.record_generation_relation("root", "a", "variant", "", None)
            .unwrap();
        db.record_generation_relation("a", "b", "detailer", "", None)
            .unwrap();
        // Removing the top parent outside the application erases nothing below it: both
        // children keep their filiation and the gap stays visible. (Job events keep their
        // own foreign key, so they are removed with the job first.)
        db.connection
            .execute("DELETE FROM job_events WHERE job_id='root'", [])
            .unwrap();
        db.connection
            .execute("DELETE FROM generation_jobs WHERE id='root'", [])
            .unwrap();
        assert_eq!(relation_count(&db), 2);
        // The tree rooted at the surviving child answers normally, no crash.
        let tree = db.generation_tree("a").unwrap();
        assert_eq!(tree["root"]["jobId"], "a");
        assert_eq!(tree["root"]["orphan"], false);
        assert_eq!(tree["root"]["parent"]["kind"], "variant");
        assert_eq!(tree["root"]["children"].as_array().unwrap().len(), 1);
        assert_eq!(node_by_id(&tree["root"], "b")["orphan"], false);
        // The dangling relation is reported from any root as a visible orphan.
        let orphans = tree["orphanRelations"].as_array().unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0]["parentJobId"], "root");
        assert_eq!(orphans[0]["parentPresent"], false);
        assert_eq!(orphans[0]["relation"]["kind"], "variant");
        // The path to the origin renders the missing ancestor as an explicit orphan node.
        let path = db.generation_path("b").unwrap();
        assert_eq!(path["path"].as_array().unwrap().len(), 3);
        assert_eq!(path["path"][0]["jobId"], "root");
        assert_eq!(path["path"][0]["orphan"], true);
        assert!(path["path"][0]["orphanReason"]
            .as_str()
            .unwrap()
            .contains("absente"));
        assert_eq!(path["path"][1]["jobId"], "a");
        assert_eq!(path["path"][2]["jobId"], "b");
        // Deleting a child row cascades only its own relation.
        db.connection
            .execute("DELETE FROM job_events WHERE job_id='b'", [])
            .unwrap();
        db.connection
            .execute("DELETE FROM generation_jobs WHERE id='b'", [])
            .unwrap();
        assert_eq!(relation_count(&db), 1);
        assert!(db.generation_tree("a").is_ok());
        assert!(db.generation_tree("missing-root").is_err());
    }

    #[test]
    fn comparison_reports_exact_parameter_and_provenance_differences() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        let left = r#"{"prompt":{"1":{"class_type":"KSampler","inputs":{"seed":1,"steps":20}}},"resources":["a"]}"#;
        let right = r#"{"prompt":{"1":{"class_type":"KSampler","inputs":{"seed":2,"steps":20}},"9":{"class_type":"Detailer","inputs":{"denoise":0.4}}},"resources":[]}"#;
        job(&db, "root", "1", SNAPSHOT);
        job(&db, "left", "111", left);
        job(&db, "right", "222", right);
        db.connection
            .execute(
                "INSERT INTO characters(id,name,identity_json) VALUES('c','Test','{}')",
                [],
            )
            .unwrap();
        db.connection.execute("INSERT INTO job_characters(job_id,role,character_id,snapshot_json) VALUES('left','P1','c','{}')", []).unwrap();
        db.connection.execute("INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','lib','/tmp/none','linux')", []).unwrap();
        for asset in ["asset-l", "asset-r"] {
            db.connection
                .execute(
                    "INSERT INTO assets(id,library_id,relative_path) VALUES(?1,'lib',?1)",
                    [asset],
                )
                .unwrap();
        }
        db.connection.execute("INSERT INTO job_outputs(job_id,asset_id,output_kind) VALUES('left','asset-l','saved')", []).unwrap();
        db.record_generation_relation("root", "left", "variant", "", None)
            .unwrap();
        db.record_generation_relation("root", "right", "variant", "", None)
            .unwrap();
        let report = db.generation_compare("left", "right").unwrap();
        assert_eq!(report["seeds"]["equal"], false);
        assert_eq!(report["seeds"]["left"], "111");
        assert_eq!(report["workflows"]["equal"], true);
        let paths: std::collections::BTreeSet<&str> = report["snapshotDiffs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            ["$.prompt.1.inputs.seed", "$.prompt.9", "$.resources[0]"]
                .into_iter()
                .collect()
        );
        let added = report["snapshotDiffs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["path"] == "$.prompt.9")
            .unwrap();
        assert_eq!(added["left"], Value::Null);
        assert_eq!(added["right"]["class_type"], "Detailer");
        assert_eq!(report["outputs"]["equal"], false);
        assert_eq!(report["characters"]["left"][0]["role"], "P1");
        assert_eq!(report["commonAncestor"], "root");
        // Identical snapshots produce an empty exact diff.
        let same = db.generation_compare("root", "root").unwrap();
        assert_eq!(same["snapshotDiffs"].as_array().unwrap().len(), 0);
        assert_eq!(same["seeds"]["equal"], true);
        assert_eq!(same["commonAncestor"], "root");
        assert!(db.generation_compare("left", "missing").is_err());
    }

    #[test]
    fn planned_branches_distinct_history_from_resumability() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        job(&db, "root", "1", SNAPSHOT);
        job(&db, "other", "2", SNAPSHOT);
        // A validated IMAGE checkpoint with verified capability is the only resumable case.
        checkpoint(
            &db,
            "ck-image",
            "root",
            "image_boundary",
            "validated",
            "verified",
        );
        checkpoint(
            &db,
            "ck-candidate",
            "root",
            "image_boundary",
            "candidate",
            "unverified",
        );
        checkpoint(
            &db,
            "ck-latent",
            "root",
            "latent_state",
            "validated",
            "verified",
        );
        checkpoint(
            &db,
            "ck-foreign",
            "other",
            "image_boundary",
            "validated",
            "verified",
        );
        // Filiation to a saved state is recorded as preparation only; nothing executes.
        let planned = db
            .plan_branch_from_saved_state(
                "root",
                "checkpoint_restore",
                "reprendre",
                Some("ck-image"),
            )
            .unwrap();
        let tree = db.generation_tree("root").unwrap();
        let planned_node = &tree["root"]["children"][0];
        assert_eq!(planned_node["planned"], true);
        assert_eq!(planned_node["jobId"], Value::Null);
        assert_eq!(planned_node["parent"]["state"], "planned");
        assert_eq!(planned_node["parent"]["checkpoint"]["resumable"], true);
        // A candidate or unverified checkpoint stays explicitly non-resumable, and a latent
        // artifact is never presented as an executable image resumption.
        let candidate = db
            .plan_branch_from_saved_state("root", "seed_change", "candidat", Some("ck-candidate"))
            .unwrap();
        let latent = db
            .plan_branch_from_saved_state("root", "variant", "latent", Some("ck-latent"))
            .unwrap();
        let branches = db.generation_branches("root").unwrap();
        let info = |relation: &str| {
            branches["branches"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["relation"]["id"] == relation)
                .unwrap()["relation"]["checkpoint"]
                .clone()
        };
        assert_eq!(info(&candidate)["resumable"], false);
        assert_eq!(info(&candidate)["resumeCapability"], "unverified");
        assert_eq!(info(&latent)["resumable"], false);
        assert_eq!(info(&latent)["resumeKind"], "latent_state");
        // Checkpoint ownership is enforced for planned branches too.
        assert!(db
            .plan_branch_from_saved_state("root", "variant", "", Some("ck-foreign"))
            .is_err());
        assert!(db
            .plan_branch_from_saved_state("root", "variant", "", Some("missing"))
            .is_err());
        // Attaching a real generation turns the prepared branch into plain filiation.
        job(&db, "resumed", "3", SNAPSHOT);
        db.link_generation_child(&planned, "resumed").unwrap();
        assert_eq!(
            db.generation_branches("root").unwrap()["branches"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let path = db.generation_path("resumed").unwrap();
        assert_eq!(path["path"][0]["jobId"], "root");
    }

    #[test]
    fn relations_reference_rows_without_duplicating_images_or_outputs() {
        let db = Database::in_memory().unwrap();
        workflow(&db, "w");
        job(&db, "root", "1", SNAPSHOT);
        job(&db, "child", "2", SNAPSHOT);
        db.connection.execute("INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','lib','/tmp/none','linux')", []).unwrap();
        for asset in ["asset-a", "asset-b"] {
            db.connection
                .execute(
                    "INSERT INTO assets(id,library_id,relative_path) VALUES(?1,'lib',?1)",
                    [asset],
                )
                .unwrap();
        }
        db.connection.execute("INSERT INTO job_outputs(job_id,asset_id,output_kind) VALUES('root','asset-a','saved')", []).unwrap();
        db.connection.execute("INSERT INTO job_outputs(job_id,asset_id,output_kind) VALUES('child','asset-b','saved')", []).unwrap();
        let count = |sql: &str| -> i64 { db.connection.query_row(sql, [], |r| r.get(0)).unwrap() };
        let (assets, outputs, jobs) = (
            count("SELECT count(*) FROM assets"),
            count("SELECT count(*) FROM job_outputs"),
            count("SELECT count(*) FROM generation_jobs"),
        );
        db.record_generation_relation("root", "child", "upscale", "", None)
            .unwrap();
        // Recording filiation writes exactly one relation row; no asset, output or job is copied.
        assert_eq!(count("SELECT count(*) FROM generation_relations"), 1);
        assert_eq!(count("SELECT count(*) FROM assets"), assets);
        assert_eq!(count("SELECT count(*) FROM job_outputs"), outputs);
        assert_eq!(count("SELECT count(*) FROM generation_jobs"), jobs);
        let tree = db.generation_tree("root").unwrap();
        assert_eq!(node_by_id(&tree["root"], "child")["outputs"], 1);
        // The event trail references the relation, it never re-describes the images.
        let events: i64 =
            count("SELECT count(*) FROM job_events WHERE event_type='tree_relation_recorded'");
        assert_eq!(events, 1);
    }
}
