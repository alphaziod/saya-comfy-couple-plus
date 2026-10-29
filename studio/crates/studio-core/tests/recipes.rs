//! Generation recipes: capture from real snapshots, byte-exact prompts,
//! versioning, portable exchange, and explicitly refused applications.
use serde_json::{json, Value};
use studio_core::{gallery::digest, workflow::Edit, Database, Result};

const PROMPT_P1: &str = "précis,  double espace\n(poids:1.3)\nligne 3";
const PROMPT_NEG: &str = "  négatif avec espaces et retour\nligne  ";
const SEED: u64 = 12345678901234567890;

fn schema() -> Value {
    json!({
        "CheckpointLoaderSimple": {"input":{"required":{"ckpt_name":[["ck_a.safetensors","ck_other.safetensors"]]}},"output":["MODEL","CLIP","VAE"]},
        "LoraLoader": {"input":{"required":{"lora_name":[["lora_x.safetensors"]],"strength_model":["FLOAT",{"default":1.0,"min":-10.0,"max":10.0}],"strength_clip":["FLOAT",{"default":1.0,"min":-10.0,"max":10.0}],"model":["MODEL"],"clip":["CLIP"]}},"output":["MODEL","CLIP"]},
        "CLIPTextEncode": {"input":{"required":{"text":["STRING",{"multiline":true}],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "EmptyLatentImage": {"input":{"required":{"width":["INT",{"min":16,"max":16384}],"height":["INT",{"min":16,"max":16384}],"batch_size":["INT",{"min":1,"max":64}]}},"output":["LATENT"]},
        "KSampler": {"input":{"required":{"seed":["INT",{"min":0,"max":18446744073709551615u64}],"steps":["INT",{"min":1,"max":10000}],"cfg":["FLOAT",{"min":0.0,"max":100.0}],"sampler_name":[["euler","dpmpp_2m"]],"scheduler":[["normal","karras"]],"denoise":["FLOAT",{"min":0.0,"max":1.0}],"model":["MODEL"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"latent_image":["LATENT"]}},"output":["LATENT"]},
        "VAEDecode": {"input":{"required":{"samples":["LATENT"],"vae":["VAE"]}},"output":["IMAGE"]},
        "SaveImage": {"input":{"required":{"filename_prefix":["STRING"],"images":["IMAGE"]}},"output":[],"output_node":true}
    })
}

fn graph(p1: &str, neg: &str, seed: u64) -> Value {
    json!({
        "1": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck_a.safetensors"}},
        "2": {"class_type":"LoraLoader","inputs":{"lora_name":"lora_x.safetensors","strength_model":0.8,"strength_clip":0.9,"model":["1",0],"clip":["1",1]}},
        "3": {"class_type":"CLIPTextEncode","inputs":{"text":p1,"clip":["2",1]}},
        "4": {"class_type":"CLIPTextEncode","inputs":{"text":neg,"clip":["2",1]}},
        "5": {"class_type":"EmptyLatentImage","inputs":{"width":1024,"height":768,"batch_size":1}},
        "6": {"class_type":"KSampler","inputs":{"seed":seed,"steps":28,"cfg":6.5,"sampler_name":"dpmpp_2m","scheduler":"karras","denoise":1.0,"model":["2",0],"positive":["3",0],"negative":["4",0],"latent_image":["5",0]}},
        "7": {"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}},
        "8": {"class_type":"SaveImage","inputs":{"filename_prefix":"saya","images":["7",0]}}
    })
}

fn binding(node: &str, input: &str, section: Option<&str>) -> Edit {
    Edit {
        section: section.map(str::to_owned),
        node_id: node.into(),
        input_name: input.into(),
        value: String::new(),
    }
}

/// Real preparation contract: workflow, explicit mapping, characters, variant,
/// edits with exact texts, then a draft job whose snapshot feeds the recipe.
fn prepared_fixture(db: &mut Database) -> Result<(String, String, String, String)> {
    let workflow = db.import_workflow(
        "Fixture",
        &graph("original p1", "original neg", 42).to_string(),
    )?;
    db.save_bindings(
        &workflow,
        &[
            binding("3", "text", Some("P1")),
            binding("4", "text", Some("NEG")),
            binding("6", "seed", None),
            binding("6", "cfg", None),
            binding("6", "steps", None),
            binding("6", "sampler_name", None),
            binding("6", "scheduler", None),
            binding("6", "denoise", None),
            binding("2", "strength_model", None),
            binding("2", "strength_clip", None),
        ],
    )?;
    let character = db.save_character(None, "Alpha".into(), json!({}))?;
    let variant = db.save_variant(
        &character,
        None,
        "Bleue",
        json!({"P1":"identité variante"}),
        json!({}),
    )?;
    let edits = vec![
        Edit {
            section: Some("P1".into()),
            node_id: "3".into(),
            input_name: "text".into(),
            value: PROMPT_P1.into(),
        },
        Edit {
            section: Some("NEG".into()),
            node_id: "4".into(),
            input_name: "text".into(),
            value: PROMPT_NEG.into(),
        },
        Edit {
            section: None,
            node_id: "6".into(),
            input_name: "seed".into(),
            value: SEED.to_string(),
        },
    ];
    let prepared = db.prepare_job_with_characters(
        &workflow,
        &edits,
        &schema(),
        &[studio_core::workflow::JobCharacter {
            role: "P1".into(),
            character_id: character.clone(),
            variant_id: Some(variant.clone()),
        }],
    )?;
    Ok((workflow, prepared.job_id, character, variant))
}

#[test]
fn recipe_captured_from_generation_snapshot_preserves_prompts_parameters_and_provenance() {
    let mut db = Database::in_memory().unwrap();
    let (workflow, job, character, variant) = prepared_fixture(&mut db).unwrap();
    let created = db
        .create_recipe_from_job(&job, "Ma recette", "description exacte")
        .unwrap();
    assert_eq!(created["version"], 1);
    let detail = db.recipe_detail(created["id"].as_str().unwrap()).unwrap();
    let content = &detail["content"];
    // Prompts byte for byte: spaces, newlines, weights.
    assert_eq!(
        content["prompts"]["P1"]["text"].as_str().unwrap(),
        PROMPT_P1
    );
    assert_eq!(
        content["prompts"]["NEG"]["text"].as_str().unwrap(),
        PROMPT_NEG
    );
    assert_eq!(content["prompts"]["P1"]["node"], "3");
    // Named parameters.
    let parameters = content["parameters"].as_array().unwrap();
    let find = |node: &str, input: &str| {
        parameters
            .iter()
            .find(|p| p["node"] == node && p["input"] == input)
            .unwrap_or_else(|| panic!("paramètre absent : {node}.{input}"))
    };
    assert_eq!(find("6", "seed")["value"], SEED.to_string());
    assert_eq!(find("6", "seed")["kind"], "seed");
    assert_eq!(find("6", "cfg")["value"], 6.5);
    assert_eq!(find("6", "steps")["value"], 28);
    assert_eq!(find("6", "sampler_name")["value"], "dpmpp_2m");
    assert_eq!(find("6", "scheduler")["value"], "karras");
    assert_eq!(find("6", "denoise")["value"], 1.0);
    // Checkpoint, LoRA with weights, custom parameters.
    assert_eq!(
        content["checkpoints"],
        json!([{"node":"1","name":"ck_a.safetensors"}])
    );
    assert_eq!(
        content["loras"],
        json!([{"node":"2","name":"lora_x.safetensors","strengthModel":0.8,"strengthClip":0.9,"composite":false}])
    );
    let custom = content["custom"].as_array().unwrap();
    let custom_find = |node: &str, input: &str| {
        custom
            .iter()
            .find(|p| p["node"] == node && p["input"] == input)
            .unwrap()
    };
    assert_eq!(custom_find("5", "width")["value"], 1024);
    assert_eq!(custom_find("5", "height")["value"], 768);
    assert_eq!(custom_find("5", "batch_size")["value"], 1);
    assert_eq!(custom_find("8", "filename_prefix")["value"], "saya");
    assert!(custom
        .iter()
        .all(|p| p["input"] != "strength_model" && p["input"] != "ckpt_name"));
    // Characters, variant, modules, dependencies, provenance, exact document.
    assert_eq!(content["characters"].as_array().unwrap().len(), 1);
    assert_eq!(content["characters"][0]["variant"]["id"], variant);
    assert_eq!(content["characters"][0]["characterId"], character);
    for module in [
        "workflow-bindings",
        "explicit-edits",
        "characters",
        "variants",
    ] {
        assert!(
            content["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m == module),
            "module {module} absent"
        );
    }
    let snapshot: String = db.job_details(&job).unwrap()["requestJson"]
        .as_str()
        .unwrap()
        .to_owned();
    // The document is the exact prepared snapshot graph (edited values), not the import copy.
    assert_eq!(
        content["workflow"]["document"],
        graph(PROMPT_P1, PROMPT_NEG, SEED)
    );
    assert_eq!(
        content["workflow"]["sourceHash"],
        digest(
            graph("original p1", "original neg", 42)
                .to_string()
                .as_bytes()
        )
    );
    assert_eq!(
        content["workflow"]["snapshotHash"],
        digest(snapshot.as_bytes())
    );
    assert_eq!(
        content["dependencies"]["modelVersions"]["status"],
        "unverified"
    );
    assert_eq!(content["provenance"]["jobId"], job);
    assert_eq!(content["workflow"]["workflowId"], workflow);
    assert!(content["reproduction"]
        .as_str()
        .unwrap()
        .contains("pixel-identique"));
    // Duplication then exact comparison of two independent recipes.
    let duplicate = db
        .duplicate_recipe(created["id"].as_str().unwrap(), "Copie")
        .unwrap();
    assert_ne!(duplicate["id"], created["id"]);
    assert_eq!(duplicate["contentSha256"], created["contentSha256"]);
    let comparison = db
        .compare_recipes(
            created["id"].as_str().unwrap(),
            duplicate["id"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(comparison["identical"], true);
    assert_eq!(comparison["differences"].as_array().unwrap().len(), 0);
}

#[test]
fn recipe_edit_creates_version_history_with_exact_diff_and_origin_immutability() {
    let mut db = Database::in_memory().unwrap();
    let (_, job, _, _) = prepared_fixture(&mut db).unwrap();
    let created = db.create_recipe_from_job(&job, "Historique", "").unwrap();
    let id = created["id"].as_str().unwrap().to_owned();
    // Untouched copy taken before the edit: the reference for the exact diff.
    let duplicate = db.duplicate_recipe(&id, "Avant édition").unwrap();
    let mut content = db.recipe_detail(&id).unwrap()["content"].clone();
    content["prompts"]["P1"]["text"] = json!(format!("{PROMPT_P1}\n ajout"));
    let steps = content["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| p["input"] == "steps")
        .unwrap();
    content["parameters"][steps]["value"] = json!(40);
    // Stale revision control refuses the write.
    assert!(db
        .save_recipe_edit(&id, content.clone(), "note", 2)
        .is_err());
    // Origin workflow is immutable.
    let mut moved = content.clone();
    moved["workflow"]["workflowId"] = json!("autre");
    assert!(db.save_recipe_edit(&id, moved, "note", 1).is_err());
    let saved = db
        .save_recipe_edit(&id, content.clone(), "steps et prompt modifiés", 1)
        .unwrap();
    assert_eq!(saved["version"], 2);
    let history = db.recipe_history(&id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["version"], 2);
    assert_eq!(history[0]["note"], "steps et prompt modifiés");
    assert_eq!(history[1]["version"], 1);
    assert_eq!(history[1]["content"]["prompts"]["P1"]["text"], PROMPT_P1);
    assert_eq!(history[0]["content"], content);
    // Exact field-by-field diff against the untouched duplicate.
    let comparison = db
        .compare_recipes(&id, duplicate["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(comparison["identical"], false);
    let differences = comparison["differences"].as_array().unwrap();
    let paths: Vec<&str> = differences
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["$.parameters[5].value", "$.prompts.P1.text"]);
    assert_eq!(differences[0]["left"], json!(40));
    assert_eq!(differences[0]["right"], json!(28));
    assert_eq!(
        differences[1]["left"],
        json!(format!("{PROMPT_P1}\n ajout"))
    );
    assert_eq!(differences[1]["right"], PROMPT_P1);
}

#[test]
fn recipe_export_import_roundtrip_refuses_invalid_documents() {
    let mut db = Database::in_memory().unwrap();
    let (_, job, _, _) = prepared_fixture(&mut db).unwrap();
    let created = db
        .create_recipe_from_job(&job, "Portable", "aller-retour")
        .unwrap();
    let exported = db.export_recipe(created["id"].as_str().unwrap()).unwrap();
    let other = Database::in_memory().unwrap();
    let imported = other.import_recipe(&exported.to_string()).unwrap();
    assert_ne!(imported["id"], created["id"]);
    assert_eq!(imported["contentSha256"], created["contentSha256"]);
    let detail = other
        .recipe_detail(imported["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(detail["content"], exported["content"]);
    assert_eq!(detail["description"], "aller-retour");
    // The origin workflow is absent in the fresh base: anchor stays empty, content keeps the origin.
    let round = other
        .export_recipe(imported["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(round, exported);
    // Invalid envelopes and content are refused without partial writes.
    let mut broken = exported.clone();
    broken["extra"] = json!(1);
    assert!(other.import_recipe(&broken.to_string()).is_err());
    let mut format = exported.clone();
    format["format"] = json!("autre-format");
    assert!(other.import_recipe(&format.to_string()).is_err());
    let mut seed = exported.clone();
    let seed_index = seed["content"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| p["input"] == "seed")
        .unwrap();
    seed["content"]["parameters"][seed_index]["value"] = json!("01");
    assert!(other.import_recipe(&seed.to_string()).is_err());
    let mut ui_graph = exported.clone();
    ui_graph["content"]["workflow"]["document"] = json!({"nodes":[]});
    assert!(other.import_recipe(&ui_graph.to_string()).is_err());
    assert_eq!(other.recipes().unwrap().len(), 1);
    let mut name = exported.clone();
    name["name"] = json!("  ");
    assert!(other.import_recipe(&name.to_string()).is_err());
}

#[test]
fn recipe_application_lists_every_missing_dependency_then_maps_compatible_edits() {
    let mut db = Database::in_memory().unwrap();
    let (workflow, job, character, variant) = prepared_fixture(&mut db).unwrap();
    let created = db.create_recipe_from_job(&job, "Appliquable", "").unwrap();
    let id = created["id"].as_str().unwrap().to_owned();
    // Incompatible target: other checkpoint, no LoRA node, no NEG mapping, no cfg,
    // unmapped custom field, archived character. Every gap is listed once.
    let target = db
        .import_workflow(
            "Incompatible",
            &json!({
                "1": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck_other.safetensors"}},
                "3": {"class_type":"CLIPTextEncode","inputs":{"text":"x","clip":["1",1]}},
                "5": {"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
                "6": {"class_type":"KSampler","inputs":{"seed":1,"steps":10,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["1",0],"positive":["3",0],"negative":["3",0],"latent_image":["5",0]}},
                "7": {"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}},
                "8": {"class_type":"SaveImage","inputs":{"filename_prefix":"saya","images":["7",0]}}
            })
            .to_string(),
        )
        .unwrap();
    db.save_bindings(&target, &[binding("6", "seed", None)])
        .unwrap();
    db.set_character_archived(&character, true).unwrap();
    let refusal = db.apply_recipe(&id, &target).unwrap_err().to_string();
    for expected in [
        "ck_a.safetensors",
        "lora_x.safetensors",
        "Section NEG",
        "cfg",
        "width",
        "Alpha",
    ] {
        assert!(
            refusal.contains(expected),
            "refus sans « {expected} » : {refusal}"
        );
    }
    assert!(refusal.starts_with("Application refusée"));
    // Reactive the character and apply to the origin workflow: explicit edits only.
    db.set_character_archived(&character, false).unwrap();
    let application = db.apply_recipe(&id, &workflow).unwrap();
    assert_eq!(application["workflowMatches"], true);
    assert_eq!(application["structureIdentical"], true);
    let edits = application["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 3, "éditions inattendues : {edits:?}");
    let p1 = edits.iter().find(|e| e["section"] == "P1").unwrap();
    assert_eq!(p1["value"], PROMPT_P1);
    let neg = edits.iter().find(|e| e["section"] == "NEG").unwrap();
    assert_eq!(neg["value"], PROMPT_NEG);
    let seed = edits.iter().find(|e| e["inputName"] == "seed").unwrap();
    assert_eq!(seed["value"], SEED.to_string());
    // Everything else already matches the stored workflow: 5 named parameters,
    // 4 custom fields, the checkpoint and both LoRA weights are unchanged.
    assert_eq!(application["unchangedCount"], 12);
    // The produced edits pass the real preparation contract unchanged.
    let parsed: Vec<studio_core::workflow::Edit> = edits
        .iter()
        .map(|e| studio_core::workflow::Edit {
            section: e["section"].as_str().map(str::to_owned),
            node_id: e["nodeId"].as_str().unwrap().to_owned(),
            input_name: e["inputName"].as_str().unwrap().to_owned(),
            value: e["value"].as_str().unwrap().to_owned(),
        })
        .collect();
    let prepared = db
        .prepare_job_with_characters(
            &workflow,
            &parsed,
            &schema(),
            &[studio_core::workflow::JobCharacter {
                role: "P1".into(),
                character_id: character,
                variant_id: Some(variant),
            }],
        )
        .unwrap();
    assert_eq!(prepared.changes.len(), 3);
    // Nothing was replaced: the stored workflow document keeps its original values.
    let stored = db.workflow(&workflow).unwrap();
    let original_p1 = stored
        .fields
        .iter()
        .find(|f| f.node_id == "3" && f.input_name == "text")
        .unwrap();
    assert_eq!(original_p1.value, "original p1");
}

#[test]
fn recipe_application_refuses_ambiguous_targets() {
    let mut db = Database::in_memory().unwrap();
    let (workflow, job, _, _) = prepared_fixture(&mut db).unwrap();
    let created = db.create_recipe_from_job(&job, "Ambiguïté", "").unwrap();
    // Renamed node ids: no exact coordinate; two cfg fields make the fallback ambiguous.
    let renamed = json!({
        "11": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck_a.safetensors"}},
        "12": {"class_type":"LoraLoader","inputs":{"lora_name":"lora_x.safetensors","strength_model":0.8,"strength_clip":0.9,"model":["11",0],"clip":["11",1]}},
        "13": {"class_type":"CLIPTextEncode","inputs":{"text":"a","clip":["12",1]}},
        "14": {"class_type":"CLIPTextEncode","inputs":{"text":"n","clip":["12",1]}},
        "15": {"class_type":"EmptyLatentImage","inputs":{"width":1024,"height":768,"batch_size":1}},
        "16": {"class_type":"KSampler","inputs":{"seed":5,"steps":28,"cfg":6.5,"sampler_name":"dpmpp_2m","scheduler":"karras","denoise":1.0,"model":["12",0],"positive":["13",0],"negative":["14",0],"latent_image":["15",0]}},
        "17": {"class_type":"VAEDecode","inputs":{"samples":["16",0],"vae":["11",2]}},
        "18": {"class_type":"SaveImage","inputs":{"filename_prefix":"saya","images":["17",0]}},
        "19": {"class_type":"KSampler","inputs":{"seed":6,"steps":28,"cfg":7.5,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["12",0],"positive":["13",0],"negative":["14",0],"latent_image":["15",0]}}
    });
    let target = db.import_workflow("Renommé", &renamed.to_string()).unwrap();
    db.save_bindings(
        &target,
        &[
            binding("13", "text", Some("P1")),
            binding("14", "text", Some("NEG")),
            binding("16", "seed", None),
            binding("16", "cfg", None),
            binding("19", "cfg", None),
            binding("12", "strength_model", None),
            binding("12", "strength_clip", None),
            binding("15", "width", None),
            binding("15", "height", None),
            binding("15", "batch_size", None),
            binding("18", "filename_prefix", None),
            binding("16", "steps", None),
            binding("16", "sampler_name", None),
            binding("16", "scheduler", None),
            binding("16", "denoise", None),
        ],
    )
    .unwrap();
    let refusal = db
        .apply_recipe(created["id"].as_str().unwrap(), &target)
        .unwrap_err()
        .to_string();
    assert!(refusal.contains("entrée cfg ambiguë"), "{refusal}");
    assert!(refusal.contains("Application refusée"));
    let _ = workflow;
}

#[test]
fn recipes_persist_after_reopening_sqlite_with_version_history() {
    let dir = std::env::temp_dir().join(format!("studio-recipes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let content_json: (String, String);
    let recipe_id;
    {
        let mut db = Database::open(&path).unwrap();
        let (_, job, _, _) = prepared_fixture(&mut db).unwrap();
        let created = db.create_recipe_from_job(&job, "Persistante", "").unwrap();
        recipe_id = created["id"].as_str().unwrap().to_owned();
        let content = db.recipe_detail(&recipe_id).unwrap()["content"].clone();
        let mut edited = content.clone();
        edited["prompts"]["P1"]["text"] = json!("modifié");
        let edited_json = edited.to_string();
        db.save_recipe_edit(&recipe_id, edited, "v2", 1).unwrap();
        content_json = (content.to_string(), edited_json);
    }
    let db = Database::open(&path).unwrap();
    assert!(db.status().unwrap().schema_version >= 11);
    let list = db.recipes().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["version"], 2);
    let detail = db.recipe_detail(&recipe_id).unwrap();
    assert_eq!(detail["content"]["prompts"]["P1"]["text"], "modifié");
    assert_eq!(detail["contentSha256"], digest(content_json.1.as_bytes()));
    let history = db.recipe_history(&recipe_id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1]["content"].to_string(), content_json.0);
    assert_eq!(history[0]["content"].to_string(), content_json.1);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v9_database_migrates_to_recipes_without_touching_existing_generations() {
    let dir = std::env::temp_dir().join(format!("studio-recipes-v9-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    {
        let old = rusqlite::Connection::open(&path).unwrap();
        for sql in [
            include_str!("../../../migrations/0001_initial.sql"),
            include_str!("../../../migrations/0002_functional.sql"),
            include_str!("../../../migrations/0003_gallery.sql"),
            include_str!("../../../migrations/0004_character_history.sql"),
            include_str!("../../../migrations/0005_prompt_proposals.sql"),
            include_str!("../../../migrations/0006_generation_provenance.sql"),
            include_str!("../../../migrations/0007_character_references.sql"),
            include_str!("../../../migrations/0008_reference_history.sql"),
            include_str!("../../../migrations/0009_index_runs.sql"),
        ] {
            old.execute_batch(sql).unwrap();
        }
        old.execute_batch("PRAGMA user_version=9;").unwrap();
        old.execute(
            "INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','Ancien',?1,'comfy_api','{}')",
            rusqlite::params![studio_core::gallery::digest(b"ancien")],
        )
        .unwrap();
        let snapshot =
            json!({"workflowId":"wf","prompt":graph("ancien", "négatif", 7)}).to_string();
        old.execute(
            "INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES('job','wf','7','succeeded',?1)",
            rusqlite::params![snapshot],
        )
        .unwrap();
    }
    let db = Database::open(&path).unwrap();
    assert!(db.status().unwrap().schema_version >= 11);
    // The historical snapshot is untouched; the recipe captures it as-is.
    assert_eq!(
        db.job_details("job").unwrap()["requestJson"],
        json!({"workflowId":"wf","prompt":graph("ancien", "négatif", 7)}).to_string()
    );
    let created = db.create_recipe_from_job("job", "Héritée", "").unwrap();
    let detail = db.recipe_detail(created["id"].as_str().unwrap()).unwrap();
    assert_eq!(detail["content"]["prompts"], json!({}));
    assert_eq!(
        detail["content"]["workflow"]["document"],
        graph("ancien", "négatif", 7)
    );
    assert_eq!(detail["content"]["provenance"]["jobState"], "succeeded");
    drop(db);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(reopened.recipes().unwrap().len(), 1);
    assert_eq!(
        reopened
            .recipe_detail(created["id"].as_str().unwrap())
            .unwrap()["content"]["workflow"]["snapshotHash"],
        digest(
            json!({"workflowId":"wf","prompt":graph("ancien", "négatif", 7)})
                .to_string()
                .as_bytes()
        )
    );
    let backups = std::fs::read_dir(dir.join("backups")).unwrap().count();
    assert_eq!(
        backups, 1,
        "une seule sauvegarde v9 ; réouverture idempotente"
    );
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn application_report_lists_problems_and_exact_before_after_without_applying() {
    let mut db = Database::in_memory().unwrap();
    let (workflow, job, character, variant) = prepared_fixture(&mut db).unwrap();
    let created = db.create_recipe_from_job(&job, "Rapport", "").unwrap();
    let id = created["id"].as_str().unwrap().to_owned();
    let report = db.recipe_application(&id, &workflow).unwrap();
    assert_eq!(report["ready"], true);
    assert_eq!(report["problems"], json!([]));
    let p1 = report["diff"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["section"] == "P1")
        .unwrap()
        .clone();
    assert_eq!(p1["before"], "original p1");
    assert_eq!(p1["after"], PROMPT_P1, "prompt must be byte-exact");
    let seed = report["diff"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["inputName"] == "seed")
        .unwrap()
        .clone();
    assert_eq!(seed["after"], "12345678901234567890");
    assert_eq!(
        report["jobCharacters"],
        json!([{"role":"P1","characterId":character,"variantId":variant}])
    );
    assert_eq!(report["checks"]["characters"], 1);
    // Strict path agrees and hides the report-only keys.
    let strict = db.apply_recipe(&id, &workflow).unwrap();
    assert!(strict.get("problems").is_none());
    assert_eq!(strict["edits"], report["edits"]);

    // Incompatible: archived character → listed, never an error, never applied.
    db.set_character_archived(&character, true).unwrap();
    let blocked = db.recipe_application(&id, &workflow).unwrap();
    assert_eq!(blocked["ready"], false);
    assert!(blocked["problems"][0].as_str().unwrap().contains("Alpha"));
    assert!(db.apply_recipe(&id, &workflow).is_err());
    let stored = db.workflow(&workflow).unwrap();
    assert!(stored
        .fields
        .iter()
        .any(|f| f.node_id == "3" && f.value == "original p1"));
}

#[test]
fn recipe_captures_model_versions_and_restore_provenance() {
    let mut db = Database::in_memory().unwrap();
    let (_workflow, job, _character, _variant) = prepared_fixture(&mut db).unwrap();
    let created = db.create_recipe_from_job(&job, "Versions", "").unwrap();
    let content = db.recipe_detail(created["id"].as_str().unwrap()).unwrap()["content"].clone();
    let models = content["dependencies"]["models"].as_array().unwrap();
    assert_eq!(models.len(), 2, "{models:?}");
    assert_eq!(models[0]["kind"], "checkpoint");
    assert_eq!(models[0]["name"], "ck_a.safetensors");
    assert_eq!(models[0]["version"]["status"], "unverified");
    assert_eq!(models[1]["kind"], "lora");
    assert_eq!(models[1]["name"], "lora_x.safetensors");
    assert_eq!(
        content["dependencies"]["passes"].as_array().unwrap().len(),
        0
    );
    assert_eq!(content["provenance"]["jobId"], job);
    assert_eq!(content["provenance"]["jobSeed"], SEED.to_string());
    assert_eq!(content["provenance"]["restore"]["seed"], SEED.to_string());
    assert_eq!(
        content["provenance"]["restore"]["prompts"]["P1"]["text"],
        PROMPT_P1
    );
    assert!(content["provenance"]["restore"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["input"] == "steps" && p["value"] == 28));
}
