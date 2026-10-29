use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    thread,
    time::Duration,
};
use studio_core::{comfy::Comfy, Database};

fn fixture() -> (PathBuf, Database, String) {
    let dir = std::env::temp_dir().join(format!("studio-jobs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let workflow = db
        .import_workflow(
            "synthetic",
            r#"{"n":{"class_type":"Text","inputs":{"text":"exact"}}}"#,
        )
        .unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
    let draft = db.prepare_job(&workflow, &[], &schema).unwrap();
    (dir, db, draft.job_id)
}
fn request(stream: &mut TcpStream) -> (String, Value) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).into_owned();
            let size = header
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + size {
                return (
                    header.lines().next().unwrap().into(),
                    if size == 0 {
                        Value::Null
                    } else {
                        serde_json::from_slice(&bytes[end + 4..end + 4 + size]).unwrap()
                    },
                );
            }
        }
    }
}
fn reply(stream: &mut TcpStream, value: Value) {
    let body = value.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .unwrap();
}
#[test]
fn stale_queue_cannot_overwrite_a_concurrent_terminal_event() {
    let (dir, mut db, job) = fixture();
    let prompt = uuid::Uuid::new_v4().to_string();
    let sql = rusqlite::Connection::open(dir.join("studio.sqlite3")).unwrap();
    sql.execute(
        "UPDATE generation_jobs SET state='running',comfy_prompt_id=? WHERE id=?",
        [&prompt, &job],
    )
    .unwrap();
    drop(sql);
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let path = dir.join("studio.sqlite3");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(request(&mut stream).0, "GET /queue HTTP/1.1");
        reply(
            &mut stream,
            json!({"queue_running":[[0,prompt,{},{}]],"queue_pending":[]}),
        );
        let (mut stream, _) = listener.accept().unwrap();
        assert!(request(&mut stream).0.starts_with("GET /history/"));
        let writer = Database::open(&path).unwrap();
        writer.record_execution_event(&json!({"type":"execution_error","data":{"prompt_id":prompt,"exception_message":"real failure"}})).unwrap();
        reply(&mut stream, json!({}));
    });
    db.sync_jobs(&client, &dir.join("results")).unwrap();
    server.join().unwrap();
    assert_eq!(db.jobs().unwrap()[0]["state"], "failed");
    assert_eq!(
        db.jobs().unwrap()[0]["error"]["exception_message"],
        "real failure"
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

fn mark(dir: &std::path::Path, job: &str, state: &str, prompt: Option<&str>) {
    rusqlite::Connection::open(dir.join("studio.sqlite3"))
        .unwrap()
        .execute(
            "UPDATE generation_jobs SET state=?2,comfy_prompt_id=?3 WHERE id=?1",
            rusqlite::params![job, state, prompt],
        )
        .unwrap();
}
fn serve(responses: Vec<(&'static str, Value)>) -> (Comfy, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let server = thread::spawn(move || {
        for (path, value) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let (req, body) = request(&mut stream);
            assert_eq!(req, path);
            assert!(body.is_null(), "Recovery must never POST");
            reply(&mut stream, value);
        }
    });
    (client, server)
}
#[test]
fn lost_submission_response_recovers_by_exact_snapshot_without_resending() {
    let (dir, mut db, job) = fixture();
    let prompt = uuid::Uuid::new_v4().to_string();
    let snapshot: String = rusqlite::Connection::open(dir.join("studio.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
            [&job],
            |r| r.get(0),
        )
        .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    mark(&dir, &job, "submission_unknown", None);
    let row = json!([0,prompt,snapshot["prompt"],{"saya_studio_job_id":job,"client_id":snapshot["clientId"]},[]]);
    let (client, server) = serve(vec![
        (
            "GET /queue HTTP/1.1",
            json!({"queue_running":[],"queue_pending":[row]}),
        ),
        ("GET /history?max_items=200 HTTP/1.1", json!({})),
        (
            Box::leak(format!("GET /history/{prompt} HTTP/1.1").into_boxed_str()),
            json!({}),
        ),
    ]);
    db.sync_jobs(&client, &dir.join("results")).unwrap();
    server.join().unwrap();
    assert_eq!(db.jobs().unwrap()[0]["promptId"], prompt);
    assert_eq!(db.jobs().unwrap()[0]["state"], "queued");
    drop(db);
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    assert_eq!(db.jobs().unwrap()[0]["promptId"], prompt);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn queue_cancel_never_becomes_global_interrupt_when_job_starts() {
    let (dir, db, job) = fixture();
    let prompt = uuid::Uuid::new_v4().to_string();
    mark(&dir, &job, "queued", Some(&prompt));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let pid = prompt.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(request(&mut stream).0, "GET /queue HTTP/1.1");
        reply(
            &mut stream,
            json!({"queue_running":[],"queue_pending":[[0,pid,{},{}]]}),
        );
        let (mut stream, _) = listener.accept().unwrap();
        let (req, body) = request(&mut stream);
        assert_eq!(req, "POST /queue HTTP/1.1");
        assert_eq!(body, json!({"delete":[pid]}));
        reply(&mut stream, json!({}));
        let (mut stream, _) = listener.accept().unwrap();
        assert_eq!(request(&mut stream).0, "GET /queue HTTP/1.1");
        reply(
            &mut stream,
            json!({"queue_running":[[0,pid,{},{}]],"queue_pending":[]}),
        );
    });
    assert!(db
        .control_job(&job, "cancel_queue", false, &client)
        .is_err());
    let result = db.control_job(&job, "cancel_queue", true, &client).unwrap();
    assert_eq!(result["confirmed"], false);
    server.join().unwrap();
    assert_ne!(db.jobs().unwrap()[0]["state"], "cancelled");
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ambiguous_or_foreign_candidates_are_not_attached() {
    for mode in ["two_ids", "foreign_client", "different_graph"] {
        let (dir, mut db, job) = fixture();
        mark(&dir, &job, "submission_unknown", None);
        let details = db.job_details(&job).unwrap();
        let snap: Value = serde_json::from_str(details["requestJson"].as_str().unwrap()).unwrap();
        let mut row = json!([0,uuid::Uuid::new_v4().to_string(),snap["prompt"],{"saya_studio_job_id":job,"client_id":snap["clientId"]},[]]);
        if mode == "foreign_client" {
            row[3]["client_id"] = json!("foreign");
        }
        if mode == "different_graph" {
            row[2]["n"]["inputs"]["text"] = json!("changed");
        }
        let mut rows = vec![row.clone()];
        if mode == "two_ids" {
            row[1] = json!(uuid::Uuid::new_v4().to_string());
            rows.push(row);
        }
        let (client, server) = serve(vec![
            (
                "GET /queue HTTP/1.1",
                json!({"queue_running":[],"queue_pending":rows}),
            ),
            ("GET /history?max_items=200 HTTP/1.1", json!({})),
        ]);
        db.sync_jobs(&client, &dir.join("results")).unwrap();
        server.join().unwrap();
        assert!(db.jobs().unwrap()[0]["promptId"].is_null());
        assert_eq!(db.jobs().unwrap()[0]["state"], "submission_unknown");
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn targeted_interrupt_requires_qualified_engine_and_keeps_pending_state_until_evidence() {
    for version in ["0.34.0", "old"] {
        let (dir, db, job) = fixture();
        let prompt = uuid::Uuid::new_v4().to_string();
        mark(&dir, &job, "running", Some(&prompt));
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
        let pid = prompt.clone();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
            reply(
                &mut s,
                json!({"queue_running":[[0,pid,{},{}]],"queue_pending":[]}),
            );
            let (mut s, _) = listener.accept().unwrap();
            assert_eq!(request(&mut s).0, "GET /system_stats HTTP/1.1");
            reply(&mut s, json!({"system":{"comfyui_version":version}}));
            if version == "0.34.0" {
                let (mut s, _) = listener.accept().unwrap();
                let (req, body) = request(&mut s);
                assert_eq!(req, "POST /interrupt HTTP/1.1");
                assert_eq!(body, json!({"prompt_id":pid}));
                reply(&mut s, json!({}));
            }
        });
        let result = db.control_job(&job, "interrupt", true, &client);
        if version == "0.34.0" {
            assert_eq!(result.unwrap()["confirmed"], false);
        } else {
            assert!(result.is_err());
        }
        server.join().unwrap();
        assert_eq!(db.jobs().unwrap()[0]["state"], "running");
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn queue_removal_is_observed_and_journal_survives_restart() {
    let (dir, db, job) = fixture();
    let prompt = uuid::Uuid::new_v4().to_string();
    mark(&dir, &job, "queued", Some(&prompt));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let pid = prompt.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
        reply(
            &mut s,
            json!({"queue_running":[],"queue_pending":[[0,pid,{},{}]]}),
        );
        let (mut s, _) = listener.accept().unwrap();
        let (req, body) = request(&mut s);
        assert_eq!(req, "POST /queue HTTP/1.1");
        assert_eq!(body, json!({"delete":[pid]}));
        reply(&mut s, json!({}));
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
        reply(&mut s, json!({"queue_running":[],"queue_pending":[]}));
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, format!("GET /history/{pid} HTTP/1.1"));
        reply(&mut s, json!({}));
    });
    assert_eq!(
        db.control_job(&job, "cancel_queue", true, &client).unwrap()["confirmed"],
        true
    );
    server.join().unwrap();
    drop(db);
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    assert_eq!(db.jobs().unwrap()[0]["state"], "cancelled");
    assert_eq!(
        db.job_details(&job).unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lost_post_response_cannot_be_submitted_again() {
    let (dir, db, job) = fixture();
    let details = db.job_details(&job).unwrap();
    let hash = studio_core::gallery::digest(details["requestJson"].as_str().unwrap().as_bytes());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let expected = job.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let (req, body) = request(&mut s);
        assert_eq!(req, "POST /prompt HTTP/1.1");
        assert_eq!(body["extra_data"]["saya_studio_job_id"], expected); /* Deliberately drop reply after acceptance. */
    });
    assert!(db.submit_job(&job, &hash, true, false, &client).is_err());
    server.join().unwrap();
    assert_eq!(db.jobs().unwrap()[0]["state"], "submission_unknown");
    assert!(db
        .submit_job(&job, &hash, true, false, &client)
        .unwrap_err()
        .to_string()
        .contains("déjà envoyé"));
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn one_reconciliation_failure_does_not_hide_another_jobs_interruption() {
    let (dir, mut db, first) = fixture();
    let workflow = db
        .import_workflow(
            "other",
            r#"{"n":{"class_type":"Text","inputs":{"text":"exact"}}}"#,
        )
        .unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
    let second = db.prepare_job(&workflow, &[], &schema).unwrap().job_id;
    let a = uuid::Uuid::new_v4().to_string();
    let b = uuid::Uuid::new_v4().to_string();
    mark(&dir, &first, "queued", Some(&a));
    mark(&dir, &second, "queued", Some(&b));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
        reply(&mut s, json!({"queue_running":[],"queue_pending":[]}));
        for _ in 0..2 {
            let (mut s, _) = listener.accept().unwrap();
            let (req, _) = request(&mut s);
            if req == format!("GET /history/{a} HTTP/1.1") {
                write!(
                    s,
                    "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            } else {
                assert_eq!(req, format!("GET /history/{b} HTTP/1.1"));
                reply(
                    &mut s,
                    json!({b.clone():{"status":{"status_str":"error","messages":[["execution_interrupted",{}]]}}}),
                );
            }
        }
    });
    assert!(db.sync_jobs(&client, &dir.join("results")).is_err());
    server.join().unwrap();
    let jobs = db.jobs().unwrap();
    let a = jobs.iter().find(|j| j["id"] == first).unwrap();
    let b = jobs.iter().find(|j| j["id"] == second).unwrap();
    assert_eq!(a["error"]["type"], "reconciliation_error");
    assert_eq!(b["state"], "interrupted");
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn completed_outputs_are_decoded_persisted_and_corrupt_images_remain_retryable() {
    for corrupt in [false, true] {
        let (dir, mut db, _) = fixture();
        let character = db
            .save_character(None, "Result identity".into(), json!({}))
            .unwrap();
        let workflow: String = rusqlite::Connection::open(dir.join("studio.sqlite3"))
            .unwrap()
            .query_row("SELECT id FROM workflow_revisions LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let edit = studio_core::workflow::Edit {
            node_id: "n".into(),
            input_name: "text".into(),
            value: "exact".into(),
            section: Some("P1".into()),
        };
        db.save_bindings(&workflow, std::slice::from_ref(&edit))
            .unwrap();
        let draft = db
            .prepare_job_with_characters(
                &workflow,
                &[edit],
                &json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}}),
                &[studio_core::workflow::JobCharacter {
                    role: "P1".into(),
                    character_id: character.clone(),
                    variant_id: None,
                }],
            )
            .unwrap();
        let job = draft.job_id;
        db.set_character_archived(&character, true).unwrap();
        let prompt = uuid::Uuid::new_v4().to_string();
        mark(&dir, &job, "running", Some(&prompt));
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
        let pid = prompt.clone();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
            reply(&mut s, json!({"queue_running":[],"queue_pending":[]}));
            let (mut s, _) = listener.accept().unwrap();
            assert_eq!(request(&mut s).0, format!("GET /history/{pid} HTTP/1.1"));
            reply(
                &mut s,
                json!({pid:{"status":{"status_str":"success","completed":true},"outputs":{"save":{"images":[{"filename":"synthetic.png","type":"output","subfolder":""}]}}}}),
            );
            let (mut s, _) = listener.accept().unwrap();
            assert!(request(&mut s).0.starts_with("GET /view?"));
            let mut png = std::io::Cursor::new(Vec::new());
            image::RgbImage::new(2, 2)
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let mut bytes = png.into_inner();
            if corrupt {
                bytes.truncate(12);
            }
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            s.write_all(&bytes).unwrap();
        });
        let result = db.sync_jobs(&client, &dir.join("results"));
        server.join().unwrap();
        let jobs = db.jobs().unwrap();
        if corrupt {
            assert!(result.is_err());
            assert_eq!(jobs[0]["state"], "running");
            assert_eq!(jobs[0]["error"]["type"], "reconciliation_error");
            assert_eq!(std::fs::read_dir(dir.join("results")).unwrap().count(), 0);
        } else {
            result.unwrap();
            assert_eq!(jobs[0]["state"], "succeeded");
            assert_eq!(jobs[0]["outputs"].as_array().unwrap().len(), 1);
            let associated = db
                .all_characters(true)
                .unwrap()
                .into_iter()
                .find(|c| c.id == character)
                .unwrap();
            assert_eq!(associated.image_ids.len(), 1);
            db.set_character_archived(&character, false).unwrap();
            let filtered = db
                .gallery(&studio_core::gallery::Filters {
                    character: character.clone(),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(filtered.total, 1);
            assert_eq!(jobs[0]["characters"][0]["characterId"], character);
            db.sync_jobs(&client, &dir.join("results")).unwrap();
        }
        drop(db);
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        assert_eq!(db.jobs().unwrap(), jobs);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn character_bindings_freeze_revisions_and_reject_wrong_variant_atomically() {
    use studio_core::workflow::{Edit, JobCharacter};
    let (dir, mut db, _) = fixture();
    let graph = json!({"a":{"class_type":"Text","inputs":{"text":"A, exact"}},"b":{"class_type":"Text","inputs":{"text":"B, exact"}}});
    let workflow = db
        .import_workflow("identities", &graph.to_string())
        .unwrap();
    let edits = vec![
        Edit {
            node_id: "a".into(),
            input_name: "text".into(),
            value: "A, exact".into(),
            section: Some("P1".into()),
        },
        Edit {
            node_id: "b".into(),
            input_name: "text".into(),
            value: "B, exact".into(),
            section: Some("P2".into()),
        },
    ];
    db.save_bindings(&workflow, &edits).unwrap();
    let a = db
        .save_character(None, "Alpha".into(), json!({"identity_prompt":"A, exact"}))
        .unwrap();
    let b = db
        .save_character(None, "Beta".into(), json!({"identity_prompt":"B, exact"}))
        .unwrap();
    let variant = db
        .save_variant(&a, None, "Variant", json!({"P1":"A, exact"}), json!({}))
        .unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
    let people = vec![
        JobCharacter {
            role: "P1".into(),
            character_id: a.clone(),
            variant_id: Some(variant.clone()),
        },
        JobCharacter {
            role: "P2".into(),
            character_id: b.clone(),
            variant_id: None,
        },
    ];
    let draft = db
        .prepare_job_with_characters(&workflow, &edits, &schema, &people)
        .unwrap();
    let snap: Value = serde_json::from_str(&draft.request_json).unwrap();
    assert_eq!(snap["characters"][0]["name"], "Alpha");
    assert_eq!(snap["characters"][1]["characterId"], b);
    db.save_character(Some(a), "Renamed".into(), json!({}))
        .unwrap();
    assert_eq!(
        db.job_details(&draft.job_id).unwrap()["requestJson"],
        draft.request_json
    );
    let count = db.jobs().unwrap().len();
    let wrong = [JobCharacter {
        role: "P2".into(),
        character_id: b,
        variant_id: Some(variant),
    }];
    assert!(db
        .prepare_job_with_characters(&workflow, &edits, &schema, &wrong)
        .is_err());
    assert_eq!(db.jobs().unwrap().len(), count);
    drop(db);
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    assert_eq!(
        db.job_details(&draft.job_id).unwrap()["requestJson"],
        draft.request_json
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

fn recover_synthetic_outputs(db: &mut Database, dir: &std::path::Path, job: &str, count: usize) {
    let downloads = if db.jobs().unwrap().iter().find(|j| j["id"] == job).unwrap()["outputs"]
        .as_array()
        .unwrap()
        .len()
        == count
    {
        0
    } else {
        count
    };
    let prompt = uuid::Uuid::new_v4().to_string();
    mark(dir, job, "running", Some(&prompt));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
        reply(&mut s, json!({"queue_running":[],"queue_pending":[]}));
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, format!("GET /history/{prompt} HTTP/1.1"));
        let images = (0..count)
            .map(
                |i| json!({"filename":format!("synthetic-{i}.png"),"type":"output","subfolder":""}),
            )
            .collect::<Vec<_>>();
        reply(
            &mut s,
            json!({prompt:{"status":{"status_str":"success","completed":true},"outputs":{"save":{"images":images}}}}),
        );
        for i in 0..downloads {
            let (mut s, _) = listener.accept().unwrap();
            assert!(request(&mut s).0.starts_with("GET /view?"));
            let mut png = std::io::Cursor::new(Vec::new());
            image::RgbImage::from_pixel(2, 2, image::Rgb([i as u8, 20, 40]))
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let bytes = png.into_inner();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            s.write_all(&bytes).unwrap();
        }
    });
    db.sync_jobs(&client, &dir.join("results")).unwrap();
    server.join().unwrap();
    db.sync_jobs(&client, &dir.join("results")).unwrap(); // No active job: no repeat download after restart/reconciliation.
}
#[test]
fn pair_and_solo_outputs_filter_by_identity_variant_workflow_and_job_after_restart() {
    use studio_core::{
        gallery::Filters,
        workflow::{Edit, JobCharacter},
    };
    let (dir, mut db, _) = fixture();
    let a = db
        .save_character(
            None,
            "Alpha".into(),
            json!({"identity_prompt":"alpha, exact\n"}),
        )
        .unwrap();
    let b = db
        .save_character(
            None,
            "Beta".into(),
            json!({"identity_prompt":"beta, exact"}),
        )
        .unwrap();
    let va = db
        .save_variant(
            &a,
            None,
            "A-blue",
            json!({"P1":"alpha, exact\n"}),
            json!({}),
        )
        .unwrap();
    let vb = db
        .save_variant(&b, None, "B-red", json!({"P1":"beta, exact"}), json!({}))
        .unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]},"FixtureImage":{"input":{"required":{"p1":["STRING"]},"optional":{"p2":["STRING"]}},"output":["IMAGE"]},"SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[]}});
    let mut graph = json!({"p1":{"class_type":"Text","inputs":{"text":"alpha, exact\n"}},"p2":{"class_type":"Text","inputs":{"text":"beta, exact"}},"source":{"class_type":"FixtureImage","inputs":{"p1":["p1",0],"p2":["p2",0]}},"save":{"class_type":"SaveImage","inputs":{"images":["source",0],"filename_prefix":"fixture"}}});
    let pair = db
        .import_workflow("Pair fixture", &graph.to_string())
        .unwrap();
    let edits = vec![
        Edit {
            node_id: "p1".into(),
            input_name: "text".into(),
            value: "alpha, exact\n".into(),
            section: Some("P1".into()),
        },
        Edit {
            node_id: "p2".into(),
            input_name: "text".into(),
            value: "beta, exact".into(),
            section: Some("P2".into()),
        },
    ];
    db.save_bindings(&pair, &edits).unwrap();
    let people = vec![
        JobCharacter {
            role: "P1".into(),
            character_id: a.clone(),
            variant_id: Some(va.clone()),
        },
        JobCharacter {
            role: "P2".into(),
            character_id: b.clone(),
            variant_id: Some(vb.clone()),
        },
    ];
    let paired = db
        .prepare_job_with_characters(&pair, &edits, &schema, &people)
        .unwrap();
    recover_synthetic_outputs(&mut db, &dir, &paired.job_id, 2);
    recover_synthetic_outputs(&mut db, &dir, &paired.job_id, 2); // Replay the same durable outputs: zero new download or association.
    graph.as_object_mut().unwrap().remove("p2");
    graph["source"]["inputs"]
        .as_object_mut()
        .unwrap()
        .remove("p2");
    let solo = db
        .import_workflow("Solo fixture", &graph.to_string())
        .unwrap();
    db.save_bindings(&solo, &edits[..1]).unwrap();
    let single = db
        .prepare_job_with_characters(
            &solo,
            &edits[..1],
            &schema,
            &[JobCharacter {
                role: "P1".into(),
                character_id: a.clone(),
                variant_id: None,
            }],
        )
        .unwrap();
    recover_synthetic_outputs(&mut db, &dir, &single.job_id, 1);
    let outputs = db
        .gallery(&Filters {
            job: paired.job_id.clone(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(outputs.total, 2);
    let selected = outputs.items[0].id.clone();
    db.annotate_asset(&selected, true, "fixture note", "accepted")
        .unwrap();
    let exact = Filters {
        characters: vec![a.clone(), b.clone()],
        variant: va.clone(),
        workflow: pair.clone(),
        job: paired.job_id.clone(),
        favorite: true,
        review: "accepted".into(),
        ..Default::default()
    };
    db.set_preference("galleryFilters", serde_json::to_value(&exact).unwrap())
        .unwrap();
    // Renaming/archiving cannot rewrite historical snapshots or remove retrieval links.
    db.save_character(Some(a.clone()), "Changed later".into(), json!({}))
        .unwrap();
    db.set_character_archived(&b, true).unwrap();
    drop(db);
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let restored: Filters =
        serde_json::from_value(db.preferences().unwrap()["galleryFilters"].clone()).unwrap();
    assert_eq!(db.gallery(&restored).unwrap().total, 1);
    for (filter, total) in [
        (
            Filters {
                character: a,
                ..Default::default()
            },
            3,
        ),
        (
            Filters {
                character: b.clone(),
                ..Default::default()
            },
            2,
        ),
        (
            Filters {
                characters: people.iter().map(|p| p.character_id.clone()).collect(),
                ..Default::default()
            },
            2,
        ),
        (
            Filters {
                variant: vb,
                ..Default::default()
            },
            2,
        ),
        (
            Filters {
                workflow: solo,
                job: paired.job_id.clone(),
                ..Default::default()
            },
            0,
        ),
        (
            Filters {
                job: single.job_id.clone(),
                ..Default::default()
            },
            1,
        ),
    ] {
        assert_eq!(db.gallery(&filter).unwrap().total, total);
    }
    let detail = db.asset_detail(&selected).unwrap();
    assert_eq!(detail["generations"][0]["jobId"], paired.job_id);
    assert_eq!(detail["generations"][0]["nodeId"], "save");
    assert_eq!(detail["generations"][0]["kind"], "saved");
    let snapshot: Value =
        serde_json::from_str(detail["generations"][0]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(snapshot["characters"][0]["name"], "Alpha");
    assert_eq!(snapshot["characters"][1]["characterId"], b);
    assert_eq!(snapshot["characters"][0]["variant"]["id"], va);
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 3);
    assert_eq!(db.facets().unwrap()["variant"].as_array().unwrap().len(), 2);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_durable_output_is_not_promoted_on_recovery() {
    let (dir, mut db, job) = fixture();
    recover_synthetic_outputs(&mut db, &dir, &job, 1);
    let asset = db.jobs().unwrap()[0]["outputs"][0]["assetId"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = db.asset_path(&asset).unwrap();
    std::fs::write(&path, b"corrupt test artifact").unwrap();
    let prompt = uuid::Uuid::new_v4().to_string();
    mark(&dir, &job, "running", Some(&prompt));
    let (client, server) = serve(vec![
        (
            "GET /queue HTTP/1.1",
            json!({"queue_running":[],"queue_pending":[]}),
        ),
        (
            Box::leak(format!("GET /history/{prompt} HTTP/1.1").into_boxed_str()),
            json!({prompt:{"status":{"completed":true},"outputs":{"save":{"images":[{"filename":"synthetic.png","type":"output"}]}}}}),
        ),
    ]);
    assert!(db.sync_jobs(&client, &dir.join("results")).is_err());
    server.join().unwrap();
    assert_eq!(db.jobs().unwrap()[0]["state"], "running");
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt test artifact");
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_owned_prompt_can_be_cancelled_after_disconnect() {
    let (dir, db, job) = fixture();
    let prompt = uuid::Uuid::new_v4().to_string();
    mark(&dir, &job, "submission_unknown", Some(&prompt));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
    let pid = prompt.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
        reply(&mut s, json!({"queue_running":[],"queue_pending":[]}));
        let (mut s, _) = listener.accept().unwrap();
        let (req, body) = request(&mut s);
        assert_eq!(req, "POST /queue HTTP/1.1");
        assert_eq!(body, json!({"delete":[pid]}));
        reply(&mut s, json!({}));
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, "GET /queue HTTP/1.1");
        reply(&mut s, json!({"queue_running":[],"queue_pending":[]}));
        let (mut s, _) = listener.accept().unwrap();
        assert_eq!(request(&mut s).0, format!("GET /history/{pid} HTTP/1.1"));
        reply(&mut s, json!({}));
    });
    assert_eq!(
        db.control_job(&job, "cancel_queue", true, &client).unwrap()["confirmed"],
        true
    );
    server.join().unwrap();
    drop(db);
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    assert_eq!(db.jobs().unwrap()[0]["state"], "cancelled");
    assert_eq!(
        db.job_details(&job).unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
