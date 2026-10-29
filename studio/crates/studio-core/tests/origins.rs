//! Verifiable job origins and provenance views. Temporary databases only.
use serde_json::{json, Value};
use studio_core::{origins::OriginInput, workflow::Edit, Database};

fn schema() -> Value {
    json!({
        "Text": {"input":{"required":{"text":["STRING"]}},"output":["CONDITIONING"]},
        "Out": {"input":{"required":{"c":["CONDITIONING"]}},"output":[],"output_node":true}
    })
}

fn origin(kind: &str, id: Option<&str>) -> OriginInput {
    OriginInput {
        kind: kind.into(),
        id: id.map(str::to_owned),
    }
}

#[test]
fn origins_are_validated_frozen_and_visible_in_provenance() {
    let dir = std::env::temp_dir().join(format!("studio-origins-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let mut db = Database::open(&path).unwrap();
    let workflow = db
        .import_workflow("W", &json!({"1":{"class_type":"Text","inputs":{"text":"a"}},"2":{"class_type":"Out","inputs":{"c":["1",0]}}}).to_string())
        .unwrap();
    db.save_bindings(
        &workflow,
        &[Edit {
            section: Some("P1".into()),
            node_id: "1".into(),
            input_name: "text".into(),
            value: String::new(),
        }],
    )
    .unwrap();
    let edit = |text: &str| {
        vec![Edit {
            section: Some("P1".into()),
            node_id: "1".into(),
            input_name: "text".into(),
            value: text.into(),
        }]
    };
    let first = db
        .prepare_job_with_characters(&workflow, &edit("b"), &schema(), &[])
        .unwrap();
    let recipe = db
        .create_recipe_from_job(&first.job_id, "Recette", "")
        .unwrap();
    let recipe_id = recipe["id"].as_str().unwrap();

    // Invalid origins are refused before anything is written.
    assert!(db
        .check_job_origin(&origin("recipe", Some("missing")))
        .is_err());
    assert!(db.check_job_origin(&origin("manual", Some("x"))).is_err());
    assert!(db.check_job_origin(&origin("scene", None)).is_err());
    assert!(db.check_job_origin(&origin("guess", Some("x"))).is_err());
    assert!(db.check_job_origin(&origin("manual", None)).is_ok());

    let second = db
        .prepare_job_with_characters(&workflow, &edit("c"), &schema(), &[])
        .unwrap();
    db.record_job_origin(&second.job_id, &origin("recipe", Some(recipe_id)))
        .unwrap();
    assert!(
        db.record_job_origin(&second.job_id, &origin("manual", None))
            .is_err(),
        "one origin per job"
    );
    let provenance = db.job_provenance(&second.job_id).unwrap();
    assert_eq!(provenance["origin"]["kind"], "recipe");
    assert_eq!(provenance["origin"]["label"], "Recette");
    assert_eq!(provenance["origin"]["version"], 1);
    assert_eq!(provenance["state"], "draft");
    assert_eq!(provenance["workflow"]["name"], "W");
    assert_eq!(provenance["path"].as_array().unwrap().len(), 1);
    // A job without recorded origin says so (null), never a guess.
    assert_eq!(
        db.job_provenance(&first.job_id).unwrap()["origin"],
        Value::Null
    );

    // Image provenance: recorded output → job → origin; unknown image → empty.
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','L','/l','linux');
        INSERT INTO assets(id,library_id,relative_path,availability) VALUES('img','lib','out.png','present'),('other','lib','x.png','present');").unwrap();
    raw.execute("INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES(?1,'img','2',0,'saved')", [&second.job_id]).unwrap();
    let asset = db.asset_provenance("img").unwrap();
    assert_eq!(asset["jobs"][0]["origin"]["kind"], "recipe");
    assert_eq!(asset["jobs"][0]["outputs"][0]["path"], "out.png");
    assert_eq!(db.asset_provenance("other").unwrap()["jobs"], json!([]));
    drop(raw);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
