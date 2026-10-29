//! A/B lab → runner: plan per cell, explicit selection, draft jobs only,
//! output collection. Temporary databases only; no engine, no GPU.
use serde_json::{json, Value};
use studio_core::{ablab::CampaignSpec, workflow::Edit, Database};

const P1: &str = "précis,  double espace\n(poids:1.3)";
const SEED: &str = "18446744073709551615";

fn schema() -> Value {
    json!({
        "CheckpointLoaderSimple": {"input":{"required":{"ckpt_name":[["ck_a.safetensors"]]}},"output":["MODEL","CLIP","VAE"]},
        "LoraLoader": {"input":{"required":{"lora_name":[["lora_x.safetensors"]],"strength_model":["FLOAT"],"strength_clip":["FLOAT"],"model":["MODEL"],"clip":["CLIP"]}},"output":["MODEL","CLIP"]},
        "CLIPTextEncode": {"input":{"required":{"text":["STRING"],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "EmptyLatentImage": {"input":{"required":{"width":["INT"],"height":["INT"],"batch_size":["INT"]}},"output":["LATENT"]},
        "KSampler": {"input":{"required":{"seed":["INT",{"min":0,"max":18446744073709551615u64}],"steps":["INT"],"cfg":["FLOAT"],"sampler_name":[["euler","dpmpp_2m"]],"scheduler":[["normal"]],"denoise":["FLOAT"],"model":["MODEL"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"latent_image":["LATENT"]}},"output":["LATENT"]},
        "VAEDecode": {"input":{"required":{"samples":["LATENT"],"vae":["VAE"]}},"output":["IMAGE"]},
        "SaveImage": {"input":{"required":{"filename_prefix":["STRING"],"images":["IMAGE"]}},"output":[],"output_node":true}
    })
}

fn graph() -> Value {
    json!({
        "1": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck_a.safetensors"}},
        "2": {"class_type":"LoraLoader","inputs":{"lora_name":"lora_x.safetensors","strength_model":0.8,"strength_clip":0.8,"model":["1",0],"clip":["1",1]}},
        "3": {"class_type":"CLIPTextEncode","inputs":{"text":"old p1","clip":["2",1]}},
        "4": {"class_type":"CLIPTextEncode","inputs":{"text":"old neg","clip":["2",1]}},
        "5": {"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "6": {"class_type":"KSampler","inputs":{"seed":1,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["2",0],"positive":["3",0],"negative":["4",0],"latent_image":["5",0]}},
        "7": {"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}},
        "8": {"class_type":"SaveImage","inputs":{"filename_prefix":"ab","images":["7",0]}}
    })
}

fn bind(node: &str, input: &str, section: Option<&str>) -> Edit {
    Edit {
        section: section.map(str::to_owned),
        node_id: node.into(),
        input_name: input.into(),
        value: String::new(),
    }
}

fn setup(mapped_steps: bool) -> (Database, std::path::PathBuf, String, String) {
    let dir = std::env::temp_dir().join(format!("studio-ab-runner-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let mut db = Database::open(&path).unwrap();
    let workflow = db.import_workflow("Ref", &graph().to_string()).unwrap();
    let mut fields = vec![
        bind("3", "text", Some("P1")),
        bind("4", "text", Some("NEG")),
        bind("6", "seed", None),
        bind("6", "cfg", None),
        bind("6", "sampler_name", None),
        bind("2", "strength_model", None),
        bind("2", "strength_clip", None),
    ];
    if mapped_steps {
        fields.push(bind("6", "steps", None));
    }
    db.save_bindings(&workflow, &fields).unwrap();
    let campaign = db
        .save_ab_campaign(&CampaignSpec {
            name: "Essai".into(),
            workflow_revision_id: workflow.clone(),
            model: "ck_a.safetensors".into(),
            loras: json!([{"name":"lora_x.safetensors","weightModel":0.8,"weightClip":0.8}]),
            prompts: json!({"P1": P1, "NEG": "  négatif\nfin  "}),
            base_seed: SEED.into(),
            steps: 20,
            cfg: 6.0,
            sampler: "euler".into(),
            grid_limit: 16,
            notes: String::new(),
        })
        .unwrap();
    db.add_ab_parameter(&campaign, "cfg", "variant", "CFG", &[json!(4.5), json!(7)])
        .unwrap();
    db.add_ab_parameter(
        &campaign,
        "sampler",
        "variant",
        "Sampler",
        &[json!("euler"), json!("dpmpp_2m")],
    )
    .unwrap();
    (db, dir, workflow, campaign)
}

#[test]
fn plan_maps_every_cell_onto_mapped_fields_and_prepares_only_selected_drafts() {
    let (db, dir, workflow, campaign) = setup(true);
    db.prepare_ab_grid(&campaign).unwrap();
    let plan = db.ab_plan(&campaign).unwrap();
    assert_eq!(plan["ready"], true, "{plan}");
    let cells = plan["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 4);
    let edit = |cell: &Value, input: &str| -> Option<String> {
        cell["edits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["inputName"] == input)
            .map(|e| e["value"].as_str().unwrap().to_owned())
    };
    // Reference values reach the workflow byte-exact; equal values emit no edit.
    assert_eq!(edit(&cells[0], "text").as_deref(), Some(P1));
    assert_eq!(edit(&cells[0], "seed").as_deref(), Some(SEED));
    assert_eq!(edit(&cells[0], "steps"), None, "steps already 20");
    assert_eq!(
        edit(&cells[0], "strength_model"),
        None,
        "LoRA weight unchanged"
    );
    // Odometer order: last variant (sampler) fastest.
    assert_eq!(edit(&cells[0], "cfg").as_deref(), Some("4.5"));
    assert_eq!(edit(&cells[0], "sampler_name"), None);
    assert_eq!(edit(&cells[1], "sampler_name").as_deref(), Some("dpmpp_2m"));
    assert_eq!(edit(&cells[3], "cfg").as_deref(), Some("7.0"));

    // Only the two selected cells become drafts; nothing is submitted.
    let jobs_before: i64 = count(&dir, "SELECT count(*) FROM generation_jobs");
    let selected = vec![
        cells[0]["cellId"].as_str().unwrap().to_owned(),
        cells[3]["cellId"].as_str().unwrap().to_owned(),
    ];
    let drafts = db
        .prepare_ab_cells(&campaign, &selected, &schema())
        .unwrap();
    assert_eq!(drafts["drafts"].as_array().unwrap().len(), 2);
    assert_eq!(
        count(&dir, "SELECT count(*) FROM generation_jobs") - jobs_before,
        2
    );
    assert_eq!(
        count(
            &dir,
            "SELECT count(*) FROM generation_jobs WHERE state!='draft'"
        ),
        0
    );
    let request: Value = serde_json::from_str(
        drafts["drafts"][1]["prepared"]["requestJson"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(request["prompt"]["6"]["inputs"]["cfg"], json!(7.0));
    assert_eq!(request["prompt"]["6"]["inputs"]["sampler_name"], "dpmpp_2m");
    assert_eq!(
        request["prompt"]["6"]["inputs"]["seed"],
        json!(18446744073709551615u64)
    );
    assert_eq!(request["prompt"]["3"]["inputs"]["text"], P1);
    let origin = db
        .job_provenance(drafts["drafts"][0]["prepared"]["jobId"].as_str().unwrap())
        .unwrap();
    assert_eq!(origin["origin"]["kind"], "ab_cell");
    assert_eq!(origin["abCell"]["position"], 0);
    let replanned = db.ab_plan(&campaign).unwrap();
    assert_eq!(replanned["cells"][0]["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(replanned["cells"][1]["jobs"].as_array().unwrap().len(), 0);
    // Stored workflow untouched.
    let stored = db.workflow(&workflow).unwrap();
    assert!(stored
        .fields
        .iter()
        .any(|f| f.node_id == "3" && f.value == "old p1"));

    // Outputs recorded for a draft's job are collected once into its cell.
    let job = drafts["drafts"][0]["prepared"]["jobId"]
        .as_str()
        .unwrap()
        .to_owned();
    let raw = rusqlite::Connection::open(dir.join("studio.sqlite3")).unwrap();
    raw.execute_batch("INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','L','/l','linux');
        INSERT INTO assets(id,library_id,relative_path,availability) VALUES('img','lib','a.png','present');").unwrap();
    raw.execute("INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES(?1,'img','8',0,'saved')", [&job]).unwrap();
    assert_eq!(db.sync_ab_results(&campaign).unwrap()["registered"], 1);
    assert_eq!(
        db.sync_ab_results(&campaign).unwrap()["registered"],
        0,
        "idempotent"
    );
    drop(raw);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unmapped_variable_blocks_the_plan_and_prepares_nothing() {
    let (db, dir, _, campaign) = setup(false);
    db.add_ab_parameter(
        &campaign,
        "steps",
        "variant",
        "Steps",
        &[json!(20), json!(30)],
    )
    .unwrap();
    db.prepare_ab_grid(&campaign).unwrap();
    let plan = db.ab_plan(&campaign).unwrap();
    assert_eq!(plan["ready"], false);
    let blocked: Vec<&Value> = plan["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["problems"].as_array().unwrap().is_empty())
        .collect();
    // steps=20 equals the workflow value (no mapping needed); steps=30 needs it.
    assert_eq!(plan["cells"].as_array().unwrap().len(), 8);
    assert_eq!(blocked.len(), 4, "exactly the steps=30 half");
    let all: Vec<String> = plan["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["cellId"].as_str().unwrap().to_owned())
        .collect();
    let before = count(&dir, "SELECT count(*) FROM generation_jobs");
    let error = db
        .prepare_ab_cells(&campaign, &all, &schema())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("steps") && error.contains("pas mappée"),
        "{error}"
    );
    assert_eq!(
        count(&dir, "SELECT count(*) FROM generation_jobs"),
        before,
        "all-or-nothing"
    );
    assert!(db.prepare_ab_cells(&campaign, &[], &schema()).is_err());
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn campaign_defaults_come_from_a_recipe_and_changed_workflow_blocks_the_plan() {
    let (db, dir, workflow, campaign) = setup(true);
    let prepared = db
        .prepare_job_with_characters(
            &workflow,
            &[Edit {
                section: Some("P1".into()),
                node_id: "3".into(),
                input_name: "text".into(),
                value: P1.into(),
            }],
            &schema(),
            &[],
        )
        .unwrap();
    let recipe = db
        .create_recipe_from_job(&prepared.job_id, "Base", "")
        .unwrap();
    let spec = db
        .ab_spec_from_recipe(recipe["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(spec["workflow"], workflow);
    assert_eq!(spec["prompts"]["P1"], P1);
    assert_eq!(spec["model"], "ck_a.safetensors");
    assert_eq!(spec["sampler"], "euler");
    assert_eq!(spec["loras"][0]["name"], "lora_x.safetensors");
    // A changed reference workflow (hash) blocks the campaign plan.
    db.prepare_ab_grid(&campaign).unwrap();
    let raw = rusqlite::Connection::open(dir.join("studio.sqlite3")).unwrap();
    raw.execute(
        "UPDATE ab_campaigns SET workflow_source_sha256='changed' WHERE id=?",
        [&campaign],
    )
    .unwrap();
    let plan = db.ab_plan(&campaign).unwrap();
    assert_eq!(plan["ready"], false);
    assert!(plan["problems"][0].as_str().unwrap().contains("a changé"));
    drop(raw);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prompt_and_seed_variants_keep_other_experiment_fields_constant() {
    let (db, _dir, workflow, _) = setup(false);
    let campaign = db
        .save_ab_campaign(&CampaignSpec {
            name: "Prompts et seeds".into(),
            workflow_revision_id: workflow,
            model: "ck_a.safetensors".into(),
            loras: json!([{"name":"lora_x.safetensors","weightModel":0.8,"weightClip":0.8}]),
            prompts: json!({"P1": "base prompt", "NEG": "base negative"}),
            base_seed: "99".into(),
            steps: 20,
            cfg: 6.0,
            sampler: "euler".into(),
            grid_limit: 16,
            notes: String::new(),
        })
        .unwrap();
    db.add_ab_parameter(
        &campaign,
        "prompt_P1",
        "variant",
        "Prompt P1",
        &[json!("A"), json!("B")],
    )
    .unwrap();
    db.add_ab_parameter(
        &campaign,
        "base_seed",
        "variant",
        "Seed",
        &[json!("7"), json!("8")],
    )
    .unwrap();
    db.prepare_ab_grid(&campaign).unwrap();
    let plan = db.ab_plan(&campaign).unwrap();
    assert_eq!(plan["ready"], true, "{plan}");
    let cells = plan["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 4);
    for cell in cells {
        let edits = cell["edits"].as_array().unwrap();
        let text = edits.iter().find(|e| e["inputName"] == "text").unwrap();
        let seed = edits.iter().find(|e| e["inputName"] == "seed").unwrap();
        assert!(text["value"] == "A" || text["value"] == "B", "{text}");
        assert!(seed["value"] == "7" || seed["value"] == "8", "{seed}");
        assert!(!edits
            .iter()
            .any(|e| e["inputName"] == "cfg" && e["value"] != 6.0));
        assert!(!edits.iter().any(|e| e["inputName"] == "sampler_name"));
    }
    let texts: Vec<&str> = cells
        .iter()
        .filter_map(|c| {
            c["edits"]
                .as_array()?
                .iter()
                .find(|e| e["inputName"] == "text")?["value"]
                .as_str()
        })
        .collect();
    assert_eq!(texts, ["A", "A", "B", "B"]);
}

fn count(dir: &std::path::Path, sql: &str) -> i64 {
    rusqlite::Connection::open(dir.join("studio.sqlite3"))
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}
