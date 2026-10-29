//! Image lifecycle of one generation lineage and startup removal of disposable previews.
//!
//! A lineage is one Saya run (casting trials included), never a seed: the same seed used
//! in another run or another config is never touched. Rules (Saya, 2026-10-29):
//! 1. at most one phase-1 image;
//! 2. at most one final (phase 6);
//! 3. a phase-1 image and a final coexist only while a new attempt (restart) runs; once that
//!    attempt validates phase 6, the new final replaces the old one and the phase-1 image goes.
//!
//! Replaced images are deleted for real through `remove_unused_asset`, which spares
//! favourites, accepted images and anything a character, a reference or an A/B cell uses,
//! and only erases files inside Studio's own folders.
//! Previews (captures of passes 2-5, ComfyUI-side copies of finished runs) are deleted
//! physically at every start; the phase-1 state in ComfyUI is kept so a run can be resumed.
//! The app owns the kept images (capture root); nothing else is kept on the ComfyUI side.

use crate::{
    saya::{read_run, write_run},
    Database, Result,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const IN_FLIGHT: [&str; 4] = ["submitting", "submitted", "queued", "running"];

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn phase_done(run: &Value, phase: &str) -> bool {
    run["phases"][phase]["state"] == "succeeded" && run["phases"][phase]["asset"].is_string()
}

fn in_flight(run: &Value) -> bool {
    run["phases"].as_object().is_some_and(|phases| {
        phases
            .values()
            .any(|p| p["state"].as_str().is_some_and(|s| IN_FLIGHT.contains(&s)))
    })
}

fn phase_job(run: &Value, phase: u32) -> Option<String> {
    run["phases"][phase.to_string()]["job"]
        .as_str()
        .map(str::to_owned)
}

/// Assets a run currently shows: its final when finished, otherwise its phase-1 image.
fn representatives(run: &Value) -> HashSet<String> {
    let phase = if phase_done(run, "6") { "6" } else { "1" };
    run["phases"][phase]["asset"]
        .as_str()
        .map(|a| HashSet::from([a.to_owned()]))
        .unwrap_or_default()
}

impl Database {
    /// Folders whose files Studio owns and may erase: results buffer, casting, capture root.
    fn owned_image_roots(&self) -> Result<Vec<PathBuf>> {
        let data = self
            .pass_state_root()?
            .parent()
            .ok_or("Base sans dossier")?
            .canonicalize()?;
        let mut owned = vec![data.join("results"), data.join("casting")];
        if let Ok(Some(root)) = self.capture_root() {
            owned.push(root);
        }
        Ok(owned)
    }

    fn job_assets(&self, job: &str) -> Result<Vec<String>> {
        let mut query = self
            .connection
            .prepare("SELECT asset_id FROM job_outputs WHERE job_id=?")?;
        let assets = query
            .query_map([job], |r| r.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(assets)
    }

    /// Preview copies of a job (Saya review gate), only when the job also saved a real image.
    fn job_preview_assets(&self, job: &str) -> Result<Vec<String>> {
        let mut query = self.connection.prepare(
            "SELECT asset_id FROM job_outputs WHERE job_id=?1 AND output_kind='preview'
               AND EXISTS (SELECT 1 FROM job_outputs WHERE job_id=?1 AND output_kind<>'preview')",
        )?;
        let assets = query
            .query_map([job], |r| r.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(assets)
    }

    /// Older deliveries recorded the review copy with kind "review", which `job_outputs`
    /// rejected: link those copies as previews so the gallery and the cleanup see them.
    fn link_review_copies(&self) -> Result<usize> {
        let mut query = self.connection.prepare(
            "SELECT id,outputs_json FROM generation_jobs WHERE outputs_json LIKE '%\"review\"%'",
        )?;
        let rows = query
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut linked = 0;
        for (job, outputs) in rows {
            let Ok(Value::Array(outputs)) = serde_json::from_str::<Value>(&outputs) else {
                continue;
            };
            for output in outputs.iter().filter(|o| o["kind"] == "review") {
                if let Some(asset) = output["assetId"].as_str() {
                    linked += self.connection.execute(
                        "INSERT OR IGNORE INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind)
                         SELECT ?1,?2,?3,?4,'preview' WHERE EXISTS (SELECT 1 FROM assets WHERE id=?2)",
                        rusqlite::params![job, asset, output["nodeId"].as_str(), output["outputIndex"].as_i64()],
                    )?;
                }
            }
        }
        Ok(linked)
    }

    /// Assets shown by any OTHER run (delivery deduplication can share one asset).
    fn assets_of_other_runs(&self, runs: &[Value], run_id: &str) -> Result<HashSet<String>> {
        let mut used = HashSet::new();
        for run in runs {
            if run["id"].as_str() == Some(run_id) {
                continue;
            }
            if let Some(phases) = run["phases"].as_object() {
                used.extend(
                    phases
                        .values()
                        .filter_map(|p| p["asset"].as_str().map(str::to_owned)),
                );
            }
            for job in strings(&run["retiredFinals"]) {
                used.extend(self.job_assets(&job)?);
            }
        }
        Ok(used)
    }

    fn remove_job_images(
        &self,
        jobs: &[String],
        keep: &HashSet<String>,
        used: &HashSet<String>,
        owned: &[PathBuf],
    ) -> Result<usize> {
        let mut removed = 0;
        for job in jobs {
            for asset in self.job_assets(job)? {
                if !keep.contains(&asset) && !used.contains(&asset) {
                    removed += self.remove_unused_asset(&asset, owned)?;
                }
            }
        }
        Ok(removed)
    }

    /// Applies rules 1-3 to one run; returns the number of files deleted. Idempotent: the
    /// jobs already handled are listed in the manifest (`compactedJobs`).
    pub fn compact_lineage(&self, run_id: &str) -> Result<usize> {
        self.compact_lineage_in(run_id, None)
    }

    /// `runs`: manifests already loaded by the caller (startup cleanup), read once.
    fn compact_lineage_in(&self, run_id: &str, runs: Option<&[Value]>) -> Result<usize> {
        let mut value = read_run(self, run_id)?;
        let handled: HashSet<String> = strings(&value["compactedJobs"]).into_iter().collect();
        let finals: HashSet<String> = strings(&value["retiredFinals"]).into_iter().collect();
        let mut doomed = Vec::new();
        if phase_done(&value, "1") {
            // Rule 1: the new phase-1 image replaces every older one of this lineage
            // (older intermediates are previews anyway). Older finals wait for rule 3.
            doomed.extend(
                strings(&value["retiredJobs"])
                    .into_iter()
                    .filter(|j| !finals.contains(j)),
            );
        }
        if phase_done(&value, "6") {
            // Rules 2-3: the validated final replaces older finals and the phase-1 image.
            doomed.extend(finals.iter().cloned());
            doomed.extend(phase_job(&value, 1));
        }
        doomed.retain(|j| !handled.contains(j));
        doomed.sort();
        doomed.dedup();
        if doomed.is_empty() {
            return Ok(0);
        }
        let loaded;
        let runs = match runs {
            Some(runs) => runs,
            None => {
                loaded = self.saya_runs()?;
                &loaded
            }
        };
        let removed = self.remove_job_images(
            &doomed,
            &representatives(&value),
            &self.assets_of_other_runs(runs, run_id)?,
            &self.owned_image_roots()?,
        )?;
        let mut all = strings(&value["compactedJobs"]);
        all.extend(doomed);
        value["compactedJobs"] = json!(all);
        write_run(self, run_id, &value)?;
        Ok(removed)
    }

    /// Physically deletes disposable previews (called at application start): captures of
    /// passes 2-5 of every run that is not running, and for finished runs the results-buffer
    /// leftovers of passes 2-6 plus the ComfyUI-side copies in the run's own isolated folder
    /// under `comfy_output` (phase 2-6 images and state, `image/` saves). The phase-1 state is
    /// kept so the image can be resumed. Returns counts; every deleted path is journaled.
    pub fn cleanup_previews(&self, comfy_output: Option<&Path>) -> Result<Value> {
        let owned = self.owned_image_roots()?;
        let results = owned[0].clone();
        let journal_dir = self.pass_state_root()?.join("preview-cleanup");
        let mut journal: Vec<String> = Vec::new();
        let (mut lineage, mut captures, mut buffer, mut comfy) = (0, 0, 0, 0);
        let mut errors = Vec::new();
        let linked = self.link_review_copies()?;
        let mut reviews = 0;
        let runs = self.saya_runs()?;
        for run in &runs {
            let Some(id) = run["id"].as_str().map(str::to_owned) else {
                continue;
            };
            if in_flight(run) {
                continue;
            }
            match self.compact_lineage_in(&id, Some(&runs)) {
                Ok(n) => lineage += n,
                Err(e) => errors.push(format!("{id}: {e}")),
            }
            let Ok(run) = read_run(self, &id) else {
                continue;
            };
            let intermediates: Vec<String> = (2..=5).filter_map(|p| phase_job(&run, p)).collect();
            captures += self.remove_job_images(
                &intermediates,
                &representatives(&run),
                &self.assets_of_other_runs(&runs, &id)?,
                &owned,
            )?;
            // Review-gate copies duplicate the saved image of the same pass.
            let all_jobs: Vec<String> = (1..=6)
                .filter_map(|p| phase_job(&run, p))
                .chain(strings(&run["retiredJobs"]))
                .collect();
            let keep = representatives(&run);
            for job in &all_jobs {
                for asset in self.job_preview_assets(job)? {
                    if !keep.contains(&asset) {
                        reviews += self.remove_unused_asset(&asset, &owned)?;
                    }
                }
            }
            if !phase_done(&run, "6") {
                continue;
            }
            let later: Vec<String> = (2..=6).filter_map(|p| phase_job(&run, p)).collect();
            if let Ok(entries) = std::fs::read_dir(&results) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if entry.path().is_file()
                        && later.iter().any(|job| name.starts_with(&format!("{job}-")))
                    {
                        std::fs::remove_file(entry.path())?;
                        journal.push(entry.path().display().to_string());
                        buffer += 1;
                    }
                }
            }
            // Without a known ComfyUI output folder there is nothing to clean on that side.
            if let (Some(comfy_output), Some(root)) = (comfy_output, run["isolatedRoot"].as_str()) {
                match clean_comfy_root(comfy_output, root, &mut journal) {
                    Ok(n) => comfy += n,
                    Err(e) => errors.push(format!("{id}: {e}")),
                }
            }
        }
        if !journal.is_empty() {
            std::fs::create_dir_all(&journal_dir)?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            std::fs::write(
                journal_dir.join(format!("{stamp}.txt")),
                journal.join("\n") + "\n",
            )?;
        }
        Ok(json!({"lineageFiles": lineage, "captureFiles": captures, "reviewCopies": reviews,
                  "reviewLinked": linked, "bufferFiles": buffer, "comfyFiles": comfy, "errors": errors}))
    }
}

/// Removes, inside `<output>/<root>`, the copies a finished run no longer needs:
/// `phase_2..6_*` images/state and the `image/` saves. `phase_1_*` stays (resume).
/// `root` must be a plain relative path that stays under `output`.
fn clean_comfy_root(output: &Path, root: &str, journal: &mut Vec<String>) -> Result<usize> {
    let relative = Path::new(root);
    if root.is_empty() || !relative.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!("Racine de run refusée : {root}").into());
    }
    let dir = output.join(relative);
    if !dir.is_dir() || !output.is_dir() {
        return Ok(0);
    }
    let dir = crate::fsutil::ensure_within(output, &dir)?;
    let mut removed = 0;
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type()?;
        if file_type.is_file() && is_later_phase_file(&name) {
            std::fs::remove_file(&path)?;
            journal.push(path.display().to_string());
            removed += 1;
        } else if file_type.is_dir() && name == "image" {
            removed += count_files(&path);
            std::fs::remove_dir_all(&path)?;
            journal.push(format!("{}/", path.display()));
        }
    }
    Ok(removed)
}

fn is_later_phase_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("phase_") else {
        return false;
    };
    // `phase_1_*` stays: it resumes the image. Studio owns the final in the capture root.
    rest.chars().next().is_some_and(|c| ('2'..='6').contains(&c))
        && rest.chars().nth(1) == Some('_')
        && (name.ends_with(".png") || name.ends_with(".json"))
}

fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| {
                    if e.path().is_dir() {
                        count_files(&e.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::{saya::write_run, Database};
    use serde_json::json;
    use std::path::{Path, PathBuf};

    struct Bench {
        _dir: PathBuf,
        db: Database,
        captures: PathBuf,
    }
    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self._dir);
        }
    }

    fn bench() -> Bench {
        let dir = std::env::temp_dir().join(format!("saya-lineage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        let captures = dir.join("captures");
        std::fs::create_dir_all(&captures).unwrap();
        db.set_preference("captureRoot", json!(captures.to_str().unwrap()))
            .unwrap();
        let root = captures.to_string_lossy().replace('\'', "''");
        db.connection
            .execute_batch(&format!(
                "INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Captures','{root}','linux');
                 INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','Test','h','comfy_api','{{}}');"
            ))
            .unwrap();
        Bench { _dir: dir, db, captures }
    }

    /// One job with one synthetic image inside the capture root.
    fn image(b: &Bench, job: &str, asset: &str) -> PathBuf {
        let path = b.captures.join(format!("{asset}.png"));
        std::fs::write(&path, asset.as_bytes()).unwrap();
        b.db.connection
            .execute_batch(&format!(
                "INSERT INTO assets(id,library_id,relative_path,availability) VALUES('{asset}','lib','{asset}.png','present');
                 INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES('{job}','wf','1','succeeded','{{}}');
                 INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES('{job}','{asset}','9',0,'saved');"
            ))
            .unwrap();
        path
    }

    fn done(job: &str, asset: &str) -> serde_json::Value {
        json!({"job":job,"state":"succeeded","asset":asset})
    }

    #[test]
    fn validated_final_replaces_phase_one_and_older_final_of_the_same_lineage() {
        let b = bench();
        let old_p1 = image(&b, "j-old1", "a-old1");
        let old_final = image(&b, "j-old6", "a-old6");
        let p1 = image(&b, "j-new1", "a-new1");
        let final_ = image(&b, "j-new6", "a-new6");
        // Another lineage with the SAME seed: never touched.
        let other = image(&b, "j-other1", "a-other1");
        write_run(&b.db, "run-aaaa-0001", &json!({"id":"run-aaaa-0001","masterSeed":"7",
            "phases":{"1":done("j-new1","a-new1")},
            "retiredJobs":["j-old1","j-old6"],"retiredFinals":["j-old6"]})).unwrap();
        write_run(&b.db, "run-bbbb-0001", &json!({"id":"run-bbbb-0001","masterSeed":"7",
            "phases":{"1":done("j-other1","a-other1")}})).unwrap();
        // Restart in progress: new phase 1 + old final coexist, the old phase 1 goes.
        b.db.compact_lineage("run-aaaa-0001").unwrap();
        assert!(!old_p1.exists(), "rule 1: older phase 1 deleted");
        assert!(old_final.exists(), "rule 3: old final kept while the retry runs");
        assert!(p1.exists());
        // The retry validates phase 6: one image left, the new final.
        write_run(&b.db, "run-aaaa-0001", &json!({"id":"run-aaaa-0001","masterSeed":"7",
            "phases":{"1":done("j-new1","a-new1"),"6":done("j-new6","a-new6")},
            "retiredJobs":["j-old1","j-old6"],"retiredFinals":["j-old6"],
            "compactedJobs":["j-old1"]})).unwrap();
        b.db.compact_lineage("run-aaaa-0001").unwrap();
        assert!(!old_final.exists(), "rule 2: older final deleted");
        assert!(!p1.exists(), "rule 3: phase 1 replaced by the final");
        assert!(final_.exists(), "the validated final stays");
        assert!(other.exists(), "same seed, other lineage: untouched");
        let removed: i64 = b.db.connection.query_row(
            "SELECT count(*) FROM assets WHERE catalog_removed=1", [], |r| r.get(0)).unwrap();
        assert_eq!(removed, 3, "rows kept for provenance but out of the gallery");
    }

    #[test]
    fn protected_and_shared_images_survive_replacement() {
        let b = bench();
        let fav = image(&b, "j-fav1", "a-fav1");
        let shared = image(&b, "j-sh1", "a-sh1");
        image(&b, "j-f6", "a-f6");
        b.db.connection.execute("UPDATE assets SET favorite=1 WHERE id='a-fav1'", []).unwrap();
        write_run(&b.db, "run-cccc-0001", &json!({"id":"run-cccc-0001",
            "phases":{"1":done("j-fav1","a-fav1"),"6":done("j-f6","a-f6")},
            "retiredJobs":["j-sh1"]})).unwrap();
        // Delivery dedup made another run show the same asset.
        write_run(&b.db, "run-dddd-0001", &json!({"id":"run-dddd-0001",
            "phases":{"1":done("j-x","a-sh1")}})).unwrap();
        b.db.compact_lineage("run-cccc-0001").unwrap();
        assert!(fav.exists(), "favourite never deleted");
        assert!(shared.exists(), "asset shown by another run never deleted");
    }

    #[test]
    fn startup_cleanup_removes_previews_but_keeps_resume_state() {
        let b = bench();
        let comfy = b._dir.join("comfy-output");
        let root = comfy.join("saya-tests/ui-run");
        std::fs::create_dir_all(root.join("image/2026-10-29")).unwrap();
        for name in ["phase_1_base.png", "phase_1_base.json", "phase_3_refiner.png",
                     "phase_5_detailers.json", "phase_6_final.png", "notes.txt"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        std::fs::write(root.join("image/2026-10-29/copy.png"), b"x").unwrap();
        image(&b, "j1", "a1");
        let pass3 = image(&b, "j3", "a3");
        image(&b, "j6", "a6");
        // Review-gate copy of pass 1, recorded the old way (kind "review", unlinked).
        let review = b.captures.join("a1-review.png");
        std::fs::write(&review, b"review").unwrap();
        b.db.connection.execute_batch(
            "INSERT INTO assets(id,library_id,relative_path,availability) VALUES('a1r','lib','a1-review.png','present');
             UPDATE generation_jobs SET outputs_json='[{\"assetId\":\"a1\",\"kind\":\"unknown\",\"nodeId\":\"9\"},{\"assetId\":\"a1r\",\"kind\":\"review\",\"nodeId\":\"54\",\"outputIndex\":0}]' WHERE id='j1';").unwrap();
        let results = b.db.pass_state_root().unwrap().parent().unwrap().join("results");
        std::fs::create_dir_all(&results).unwrap();
        std::fs::write(results.join("j3-node-0.latent"), b"x").unwrap();
        std::fs::write(results.join("j1-node-0.latent"), b"x").unwrap();
        write_run(&b.db, "run-eeee-0001", &json!({"id":"run-eeee-0001","isolatedRoot":"saya-tests/ui-run",
            "phases":{"1":done("j1","a1"),"3":done("j3","a3"),"6":done("j6","a6")}})).unwrap();
        // A running run and a hostile root are left alone.
        let running = comfy.join("saya-tests/ui-live");
        std::fs::create_dir_all(&running).unwrap();
        std::fs::write(running.join("phase_2_hires.png"), b"x").unwrap();
        write_run(&b.db, "run-ffff-0001", &json!({"id":"run-ffff-0001","isolatedRoot":"saya-tests/ui-live",
            "phases":{"1":done("j1","a1"),"2":{"job":"j-live","state":"running"}}})).unwrap();
        write_run(&b.db, "run-gggg-0001", &json!({"id":"run-gggg-0001","isolatedRoot":"../outside",
            "phases":{"6":done("j6","a6")}})).unwrap();
        let report = b.db.cleanup_previews(Some(&comfy)).unwrap();
        assert!(root.join("phase_1_base.png").exists() && root.join("phase_1_base.json").exists(),
            "phase-1 state kept for resuming");
        for gone in ["phase_3_refiner.png", "phase_5_detailers.json", "phase_6_final.png", "image"] {
            assert!(!root.join(gone).exists(), "{gone} deleted");
        }
        assert!(root.join("notes.txt").exists(), "unknown files untouched");
        assert!(!pass3.exists(), "pass-3 capture deleted");
        assert!(!review.exists(), "review-gate copy deleted: {report}");
        assert_eq!(report["reviewLinked"], 1);
        assert!(!results.join("j3-node-0.latent").exists());
        assert!(results.join("j1-node-0.latent").exists(), "phase-1 latent kept");
        assert!(running.join("phase_2_hires.png").exists(), "running run untouched");
        assert_eq!(report["errors"].as_array().unwrap().len(), 1, "hostile root reported: {report}");
        assert!(Path::new(&b.db.pass_state_root().unwrap().join("preview-cleanup")).is_dir(), "journal written");
    }
}

#[cfg(test)]
mod step_tests {
    use crate::{saya::{read_run, write_run}, Database};
    use serde_json::json;

    #[test]
    fn step_batch_releases_one_trial_per_call_in_index_order() {
        let dir = std::env::temp_dir().join(format!("saya-step-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        let ids: Vec<String> = (0..3).map(|i| format!("step-run-000{i}")).collect();
        // Written out of order on purpose: release follows the casting index.
        for (i, id) in ids.iter().enumerate().rev() {
            write_run(&db, id, &json!({"id":id,"state":"running","phases":{"1":{"job":format!("j{i}"),"state":"waiting"}},
                "casting":{"catalogue":"/tmp/cat.json","index":i}})).unwrap();
        }
        db.casting_batch_step(&ids).unwrap();
        assert_eq!(read_run(&db, &ids[0]).unwrap()["casting"]["released"], true);
        assert_eq!(read_run(&db, &ids[1]).unwrap()["casting"]["released"], false);
        // The first trial is being generated: Next releases index 1, then index 2.
        let mut first = read_run(&db, &ids[0]).unwrap();
        first["phases"]["1"]["state"] = json!("running");
        write_run(&db, &ids[0], &first).unwrap();
        assert_eq!(db.casting_next(&ids[0]).unwrap(), ids[1]);
        assert_eq!(db.casting_next(&ids[1]).unwrap(), ids[2]);
        let exhausted = db.casting_next(&ids[2]).unwrap_err().to_string();
        assert!(exhausted.contains("épuisé"), "{exhausted}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
