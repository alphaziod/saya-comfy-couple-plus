use serde_json::json;
use studio_core::{live::LiveJob, Database};
#[test]
fn client_identity_is_persistent_and_shared_by_drafts() {
    let db = Database::in_memory().unwrap();
    let first = db.comfy_client_id().unwrap();
    assert_eq!(first, db.comfy_client_id().unwrap());
    uuid::Uuid::parse_str(&first).unwrap();
    let graph = json!({"n":{"class_type":"Text","inputs":{"text":"exact"}}});
    let id = db.import_workflow("fixture", &graph.to_string()).unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
    for _ in 0..2 {
        let draft = db.prepare_job(&id, &[], &schema).unwrap();
        let data: serde_json::Value = serde_json::from_str(&draft.request_json).unwrap();
        assert_eq!(data["clientId"], first);
    }
}
#[test]
fn progress_is_scoped_and_never_inferred_from_an_unidentified_event() {
    let mut job = LiveJob::default();
    assert!(!job.apply("ours", &json!({"type":"executing","data":{"node":"3"}})));
    assert!(!job.apply(
        "ours",
        &json!({"type":"progress","data":{"prompt_id":"other","node":"3","value":3,"max":10}})
    ));
    assert!(job.apply(
        "ours",
        &json!({"type":"progress","data":{"prompt_id":"ours","node":"3","value":3,"max":10}})
    ));
    assert_eq!(job.node.as_deref(), Some("3"));
    assert_eq!(job.value, Some(3.0));
    assert_eq!(job.max, Some(10.0));
    assert!(!job.apply(
        "ours",
        &json!({"type":"progress","data":{"prompt_id":"ours","node":"3","value":99,"max":10}})
    ));
    assert!(job.apply(
        "ours",
        &json!({"type":"executing","data":{"prompt_id":"ours","node":"4"}})
    ));
    assert!(job.value.is_none());
    assert!(job.apply("ours",&json!({"type":"execution_error","data":{"prompt_id":"ours","node_id":"4","exception_message":"fixture error"}})));
    assert_eq!(
        job.error.as_ref().unwrap()["exception_message"],
        "fixture error"
    );
}
#[test]
fn previews_require_explicit_identity_and_a_valid_bounded_image() {
    use std::io::Cursor;
    let mut png = Cursor::new(Vec::new());
    image::RgbImage::new(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let metadata = json!({"prompt_id":"ours","node_id":"3","image_type":"image/png"}).to_string();
    let mut bytes = 4u32.to_be_bytes().to_vec();
    bytes.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
    bytes.extend(metadata.as_bytes());
    bytes.extend(png.into_inner());
    let preview = studio_core::live::decode_preview(&bytes).unwrap().unwrap();
    assert_eq!(preview.prompt_id, "ours");
    assert!(preview.image.starts_with("data:image/png;base64,"));
    assert!(studio_core::live::decode_preview(&[0, 0, 0, 1, 0, 0, 0, 2])
        .unwrap()
        .is_none());
    assert!(studio_core::live::decode_preview(&bytes[..10]).is_err());
    bytes[8 + metadata.len()] ^= 0xff; // Corrupt the image signature, not unused PNG trailer bytes.
    assert!(studio_core::live::decode_preview(&bytes).is_err());
}

#[test]
fn client_id_survives_restart_and_invalid_nodes_do_not_replace_progress() {
    let dir = std::env::temp_dir().join(format!("studio-client-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let db = Database::open(&path).unwrap();
    let id = db.comfy_client_id().unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.comfy_client_id().unwrap(), id);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
    let mut job = LiveJob::default();
    assert!(job.apply("p",&json!({"type":"progress_state","data":{"prompt_id":"p","nodes":{"1":{"prompt_id":"p","state":"running","value":1,"max":2}}}})));
    assert_eq!(job.nodes["1"]["value"], 1);
    assert!(!job.apply("p",&json!({"type":"progress_state","data":{"prompt_id":"p","nodes":{"1":{"prompt_id":"other","state":"running","value":2,"max":2}}}})));
    assert_eq!(job.nodes["1"]["value"], 1);
    job.apply(
        "p",
        &json!({"type":"execution_success","data":{"prompt_id":"p"}}),
    );
    assert!(!job.apply(
        "p",
        &json!({"type":"progress","data":{"prompt_id":"p","node":"1","value":2,"max":2}})
    ));
}
