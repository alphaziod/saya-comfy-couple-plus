//! Durable image boundaries. Human validation is separate from execution compatibility.
use crate::{
    contracts::Seed,
    gallery::{digest, file_digest},
    Database, Result,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
};

fn upstream(graph: &Value, node: &str) -> Result<Value> {
    let nodes = graph.as_object().ok_or("Graphe API absent")?;
    let mut selected = BTreeMap::new();
    let mut pending = vec![node.to_owned()];
    while let Some(id) = pending.pop() {
        if selected.contains_key(&id) {
            continue;
        }
        if selected.len() >= 10_000 {
            return Err("Dépendances trop nombreuses".into());
        }
        let value = nodes.get(&id).ok_or("Dépendance amont absente")?;
        for input in value["inputs"]
            .as_object()
            .ok_or("Entrées amont invalides")?
            .values()
        {
            if let Some(link) = input.as_array() {
                if link.len() != 2 || !link[1].is_u64() {
                    return Err("Connexion amont ambiguë".into());
                }
                pending.push(link[0].as_str().ok_or("Source amont invalide")?.to_owned());
            }
        }
        selected.insert(id, value.clone());
    }
    Ok(serde_json::to_value(selected)?)
}
pub(crate) fn create_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
pub(crate) fn store_lock(root: &Path, busy: &str) -> Result<File> {
    if !root.exists() {
        let mut directory = fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(root)?;
    }
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err("Stockage checkpoint symbolique refusé".into());
    }
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let path = root.join(".store.lock");
    if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err("Verrou symbolique refusé".into());
    }
    let lock = options.open(path)?;
    lock.try_lock().map_err(|_| busy.to_string())?;
    Ok(lock)
}
fn artifact_path(root: &Path, manifest: &Value) -> Result<PathBuf> {
    if manifest["format"] != "saya-image-checkpoint" || manifest["version"] != 1 {
        return Err("Version de checkpoint inconnue".into());
    }
    let hash = manifest["sha256"].as_str().ok_or("Hash absent")?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Hash invalide".into());
    }
    let name = format!("{hash}.png");
    if manifest["file"] != name {
        return Err("Chemin de checkpoint invalide".into());
    }
    let path = root.join(name);
    if !fs::symlink_metadata(&path)?.file_type().is_file() {
        return Err("Checkpoint absent ou non régulier".into());
    }
    Ok(path)
}
impl Database {
    pub fn checkpoints(&self, job: &str) -> Result<Vec<Value>> {
        let mut query=self.connection.prepare("SELECT a.id,a.status,a.resume_capability,a.manifest_json,p.phase_key,p.attempt FROM pass_artifacts a JOIN pass_attempts p ON p.id=a.pass_attempt_id WHERE p.job_id=? ORDER BY p.rowid DESC,a.rowid DESC")?;
        let rows = query.query_map([job], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, u32>(5)?,
            ))
        })?;
        let mut values = Vec::new();
        for row in rows {
            let (id, status, capability, manifest, phase, attempt) = row?;
            values.push(json!({"id":id,"status":status,"resumeCapability":capability,"manifestJson":manifest,"manifest":serde_json::from_str::<Value>(&manifest)?,"phase":phase,"attempt":attempt}));
        }
        Ok(values)
    }
    pub fn capture_image_checkpoint(
        &self,
        root: &Path,
        job: &str,
        asset: &str,
        max_bytes: u64,
    ) -> Result<Value> {
        if !(1..=16 * 1024 * 1024 * 1024 * 1024).contains(&max_bytes) {
            return Err("Budget de stockage invalide".into());
        }
        let (node,index,snapshot,seed,expected):(String,u32,String,String,String)=self.connection.query_row("SELECT o.node_id,o.output_index,j.request_snapshot_json,j.seed,a.content_sha256 FROM job_outputs o JOIN generation_jobs j ON j.id=o.job_id JOIN assets a ON a.id=o.asset_id WHERE o.job_id=?1 AND o.asset_id=?2 AND o.output_kind='saved' AND j.state='succeeded'",params![job,asset],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        Seed::parse(&seed)?;
        let request: Value = serde_json::from_str(&snapshot)?;
        let graph = &request["prompt"];
        if graph[&node]["class_type"] != "SaveImage" {
            return Err(
                "Seule une sortie SaveImage native est qualifiée pour cette capture".into(),
            );
        }
        let link = graph[&node]["inputs"]["images"]
            .as_array()
            .ok_or("Source IMAGE non liée")?;
        if link.len() != 2 || !link[1].is_u64() {
            return Err("Source IMAGE ambiguë".into());
        }
        let source_node = link[0].as_str().ok_or("Node IMAGE absent")?;
        let dependencies = upstream(graph, source_node)?;
        let source = self.asset_path(asset)?;
        let stat = source.metadata()?;
        if stat.len() > 128 * 1024 * 1024 {
            return Err("Image supérieure à 128 Mio".into());
        }
        let mut reader = image::ImageReader::open(&source)?.with_guessed_format()?;
        if reader.format() != Some(image::ImageFormat::Png) {
            return Err("Cette frontière IMAGE exige un PNG".into());
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        reader.limits(limits);
        let decoded = reader.decode()?;
        let (width, height) = (decoded.width(), decoded.height());
        drop(decoded);
        if file_digest(&source)? != expected {
            return Err("Résultat modifié depuis sa récupération".into());
        }
        let _lock = store_lock(root, "Stockage des checkpoints occupé")?;
        let existing:Option<String>=self.connection.query_row("SELECT a.id FROM pass_artifacts a JOIN pass_attempts p ON p.id=a.pass_attempt_id WHERE p.job_id=?1 AND a.asset_id=?2 AND json_extract(a.manifest_json,'$.format')='saya-image-checkpoint' AND json_extract(a.manifest_json,'$.version')=1 ORDER BY a.rowid LIMIT 1",params![job,asset],|r|r.get(0)).optional()?;
        if let Some(id) = existing {
            self.verify_checkpoint(root, &id)?;
            return Ok(json!({"id":id,"resumeCapability":"unverified"}));
        }
        let destination = root.join(format!("{expected}.png"));
        if destination.exists() {
            if !fs::symlink_metadata(&destination)?.file_type().is_file()
                || file_digest(&destination)? != expected
            {
                return Err("Fichier de checkpoint existant incohérent".into());
            }
        } else {
            let mut used = 0u64;
            for entry in fs::read_dir(root)? {
                let meta = entry?.metadata()?;
                if meta.is_file() {
                    used = used
                        .checked_add(meta.len())
                        .ok_or("Taille de stockage invalide")?;
                }
            }
            if used
                .checked_add(stat.len())
                .is_none_or(|size| size > max_bytes)
            {
                return Err("Budget de checkpoints dépassé ; aucun fichier supprimé".into());
            }
            let temporary = root.join(format!(".partial-{}", uuid::Uuid::new_v4()));
            let mut output = create_file(&temporary)?;
            let copied = io::copy(&mut File::open(&source)?.take(stat.len() + 1), &mut output)?;
            if copied != stat.len() {
                return Err("Taille modifiée pendant la capture ; fichier partiel conservé".into());
            }
            output.sync_all()?;
            if file_digest(&temporary)? != expected {
                return Err("Image modifiée pendant la capture ; fichier partiel conservé".into());
            }
            fs::hard_link(&temporary, &destination)?;
            fs::remove_file(&temporary)?;
            #[cfg(unix)]
            File::open(root)?.sync_all()?;
        }
        let manifest = json!({"format":"saya-image-checkpoint","version":1,"jobId":job,"kind":"image_boundary","outputNode":node,"outputIndex":index,"source":link,"sourceSnapshotHash":digest(snapshot.as_bytes()),"upstream":dependencies,"upstreamHash":digest(dependencies.to_string().as_bytes()),"seed":seed,"resources":request["resources"],"modelVersions":{"status":"unverified"},"file":format!("{expected}.png"),"sha256":expected,"bytes":stat.len(),"width":width,"height":height,"precision":"saved PNG; single image only","adapter":"save-image-boundary-v1","resumeReason":"Graphe aval et versions de modèles non qualifiés. Aucun latent ni état sampler dans ce PNG."});
        let tx = self.connection.unchecked_transaction()?;
        let pass = uuid::Uuid::new_v4().to_string();
        let id = uuid::Uuid::new_v4().to_string();
        let phase = format!("SaveImage:{node}:{index}");
        tx.execute("INSERT INTO pass_attempts(id,job_id,phase_key,attempt,state,effective_parameters_json) VALUES(?1,?2,?3,(SELECT coalesce(max(attempt),0)+1 FROM pass_attempts WHERE job_id=?2 AND phase_key=?3),'awaiting_review',?4)",params![pass,job,phase,dependencies.to_string()])?;
        tx.execute("INSERT INTO pass_artifacts(id,pass_attempt_id,asset_id,kind,manifest_json,content_sha256) VALUES(?1,?2,?3,'image_boundary',?4,?5)",params![id,pass,asset,manifest.to_string(),expected])?;
        tx.commit()?;
        Ok(json!({"id":id,"resumeCapability":"unverified"}))
    }
    pub fn verify_checkpoint(&self, root: &Path, id: &str) -> Result<Value> {
        let (text, hash): (String, String) = self.connection.query_row(
            "SELECT manifest_json,content_sha256 FROM pass_artifacts WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let manifest: Value = serde_json::from_str(&text)?;
        let path = artifact_path(root, &manifest)?;
        if manifest["sha256"] != hash
            || path.metadata()?.len() != manifest["bytes"].as_u64().ok_or("Taille absente")?
            || file_digest(&path)? != hash
        {
            return Err("Intégrité du checkpoint invalide".into());
        }
        Ok(manifest)
    }
    pub fn check_checkpoint_dependencies(
        &self,
        root: &Path,
        id: &str,
        graph: &Value,
    ) -> Result<()> {
        let manifest = self.verify_checkpoint(root, id)?;
        let node = manifest["source"][0]
            .as_str()
            .ok_or("Source de checkpoint absente")?;
        let current = upstream(graph, node)?;
        if current != manifest["upstream"]
            || digest(current.to_string().as_bytes()) != manifest["upstreamHash"]
        {
            return Err("Dépendances ou paramètres amont modifiés ; reprise refusée".into());
        }
        Ok(())
    }
    pub fn review_checkpoint(&self, root: &Path, id: &str, accept: bool, note: &str) -> Result<()> {
        if note.len() > 20_000 {
            return Err("Note trop longue".into());
        }
        if accept {
            self.verify_checkpoint(root, id)?;
        }
        let tx = self.connection.unchecked_transaction()?;
        if tx.execute(
            "UPDATE pass_artifacts SET status=?2 WHERE id=?1 AND kind='image_boundary'",
            params![id, if accept { "validated" } else { "rejected" }],
        )? != 1
        {
            return Err("Checkpoint image introuvable".into());
        }
        tx.execute(
            "INSERT INTO human_reviews(id,artifact_id,decision,note) VALUES(?1,?2,?3,?4)",
            params![
                uuid::Uuid::new_v4().to_string(),
                id,
                if accept { "accept" } else { "reject" },
                note
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}
