//! Seed-mode fidelity for the rgthree `Seed (rgthree)` node in universal
//! preparation. Offline: no engine, no user workflow mutation.
//!
//! Covered production contract:
//! - `-1` (Randomize) and `-2`/`-3` (increment/decrement sentinels) are passed
//!   through unchanged to the real rgthree Seed node, never to independent
//!   KSampler/KSamplerAdvanced seeds.
//! - a fixed u64 above 2^53 is injected exactly (JSON number is never rounded
//!   through f64), and independent seeds keep their values.
//! - without a Seed node, a sentinel is refused (no silent broadcast) while a
//!   fixed seed fills the documented `seed`/`noise_seed` fallback.
//! - the fixed seed survives profile export/import, so a persisted session
//!   (continue/restart) re-applies the last fixed seed exactly.
use serde_json::{json, Value};
use studio_core::Database;

const ABOVE_2_POW_53: u64 = 9_007_199_254_740_993; // 2^53 + 1
const ABOVE_2_POW_53_TEXT: &str = "9007199254740993";

fn graph(with_seed_node: bool) -> Value {
    let mut graph = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"text":"P1 old","clip":["1",1]}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":"bg old","clip":["1",1]}},
        "4":{"class_type":"CLIPTextEncode","inputs":{"text":"action old","clip":["1",1]}},
        "5":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "6":{"class_type":"KSampler","inputs":{"seed":1,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["1",0],"positive":["2",0],"negative":["4",0],"latent_image":["5",0]}},
        "7":{"class_type":"KSampler","inputs":{"seed":99,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["1",0],"positive":["2",0],"negative":["4",0],"latent_image":["5",0]}},
        "8":{"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}},
        "9":{"class_type":"SaveImage","inputs":{"images":["8",0],"filename_prefix":"seed-modes"}},
        "12":{"class_type":"KSamplerAdvanced","inputs":{"add_noise":"enable","noise_seed":55,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","start_at_step":0,"end_at_step":10000,"return_with_leftover_noise":"disable","model":["1",0],"positive":["2",0],"negative":["4",0],"latent_image":["5",0]}}
    });
    if with_seed_node {
        graph["11"] = json!({"class_type":"Seed (rgthree)","inputs":{"seed":7}});
    }
    graph
}

/// Import the fixture, map P1/BG/ACTION and persist a profile carrying
/// `master_seed` (the same field the Svelte quick controls write).
fn setup(db: &Database, with_seed_node: bool, master_seed: &str) -> (String, String) {
    let workflow = db
        .import_workflow("Seed modes", &graph(with_seed_node).to_string())
        .unwrap();
    let p1 = db
        .save_character(
            None,
            "P1".into(),
            json!({"identity_prompt":"p1","anatomy_prompt":"a1"}),
        )
        .unwrap();
    for (key, node, input) in [
        ("P1", "2", "text"),
        ("BG", "3", "text"),
        ("ACTION", "4", "text"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":p1},
        "p2":{"characterId":""},
        "bg":{"source":"fixed","text":"bg fixed"},
        "action":{"source":"fixed","text":"action fixed"},
        "masterSeed":master_seed,
        "mappings":{
            "P1":{"nodeId":"2","inputName":"text"},
            "BG":{"nodeId":"3","inputName":"text"},
            "ACTION":{"nodeId":"4","inputName":"text"}
        },
        "quick":[]
    });
    let profile = db
        .save_prepare_profile(None, "Seed modes", &workflow, "solo", &config)
        .unwrap();
    (workflow, profile["id"].as_str().unwrap().to_owned())
}

fn prepared_prompt(db: &Database, profile: &str) -> Value {
    let result = db.prepare_universal(profile, &Value::Null).unwrap();
    serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap()
}

#[test]
fn rgthree_sentinel_minus_one_targets_seed_node_not_independent_seeds() {
    let db = Database::in_memory().unwrap();
    let (workflow, profile) = setup(&db, true, "-1");
    let result = db.prepare_universal(&profile, &Value::Null).unwrap();
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();

    // Randomize Each Time: -1 is resolved by the rgthree node itself.
    assert_eq!(request["prompt"]["11"]["inputs"]["seed"], json!(-1));
    // Independent KSampler / KSamplerAdvanced seeds are not overwritten.
    assert_eq!(request["prompt"]["6"]["inputs"]["seed"], json!(1));
    assert_eq!(request["prompt"]["7"]["inputs"]["seed"], json!(99));
    assert_eq!(request["prompt"]["12"]["inputs"]["noise_seed"], json!(55));
    assert_eq!(result["resolution"]["masterSeed"], "-1");
    // The source workflow stays read-only.
    assert_eq!(
        db.workflow_document(&workflow).unwrap().1,
        graph(true).to_string()
    );
}

#[test]
fn rgthree_increment_decrement_sentinels_are_passed_through() {
    for (sentinel, expected) in [("-2", -2), ("-3", -3)] {
        let db = Database::in_memory().unwrap();
        let (_, profile) = setup(&db, true, sentinel);
        let request = prepared_prompt(&db, &profile);
        assert_eq!(
            request["prompt"]["11"]["inputs"]["seed"],
            json!(expected),
            "sentinel {sentinel} must reach the rgthree node unchanged"
        );
        assert_eq!(request["prompt"]["6"]["inputs"]["seed"], json!(1));
        assert_eq!(request["prompt"]["7"]["inputs"]["seed"], json!(99));
        assert_eq!(request["prompt"]["12"]["inputs"]["noise_seed"], json!(55));
    }
}

#[test]
fn fixed_u64_above_2_pow_53_is_exact_and_isolated() {
    let db = Database::in_memory().unwrap();
    let (_, profile) = setup(&db, true, ABOVE_2_POW_53_TEXT);
    let result = db.prepare_universal(&profile, &Value::Null).unwrap();
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();

    let seed = &request["prompt"]["11"]["inputs"]["seed"];
    assert_eq!(seed.as_u64(), Some(ABOVE_2_POW_53));
    // Exact decimal spelling in the serialized request, never an f64 rounding.
    assert_eq!(seed.to_string(), ABOVE_2_POW_53_TEXT);
    assert!(result["prepared"]["requestJson"]
        .as_str()
        .unwrap()
        .contains("\"seed\":9007199254740993"));
    assert_eq!(result["resolution"]["masterSeed"], ABOVE_2_POW_53_TEXT);
    // A fixed master seed still must not unify independent seeds.
    assert_eq!(request["prompt"]["6"]["inputs"]["seed"], json!(1));
    assert_eq!(request["prompt"]["7"]["inputs"]["seed"], json!(99));
    assert_eq!(request["prompt"]["12"]["inputs"]["noise_seed"], json!(55));
}

#[test]
fn sentinel_without_rgthree_seed_is_refused_not_broadcast() {
    for sentinel in ["-1", "-2", "-3"] {
        let db = Database::in_memory().unwrap();
        let (_, profile) = setup(&db, false, sentinel);
        let error = db
            .prepare_universal(&profile, &Value::Null)
            .expect_err("a sentinel without a Seed (rgthree) node must be refused");
        assert!(
            error.to_string().contains("sans node Seed"),
            "unexpected error for {sentinel}: {error}"
        );
        assert!(db.jobs().unwrap().is_empty(), "no draft job may be created");
    }
}

#[test]
fn fixed_seed_without_rgthree_seed_uses_seed_fallback() {
    let db = Database::in_memory().unwrap();
    let (_, profile) = setup(&db, false, ABOVE_2_POW_53_TEXT);
    let request = prepared_prompt(&db, &profile);
    assert_eq!(
        request["prompt"]["6"]["inputs"]["seed"].as_u64(),
        Some(ABOVE_2_POW_53)
    );
    assert_eq!(
        request["prompt"]["7"]["inputs"]["seed"].as_u64(),
        Some(ABOVE_2_POW_53)
    );
    assert_eq!(
        request["prompt"]["12"]["inputs"]["noise_seed"].as_u64(),
        Some(ABOVE_2_POW_53)
    );
}

#[test]
fn fixed_seed_survives_profile_export_import_across_sessions() {
    let db = Database::in_memory().unwrap();
    let (workflow, profile) = setup(&db, true, ABOVE_2_POW_53_TEXT);
    let exported = db.export_prepare_profile(&profile).unwrap();
    assert_eq!(exported["config"]["masterSeed"], ABOVE_2_POW_53_TEXT);
    let imported = db
        .import_prepare_profile(&exported.to_string(), &workflow)
        .unwrap();
    assert_eq!(
        db.prepare_profile(imported["id"].as_str().unwrap())
            .unwrap()["config"]["masterSeed"],
        ABOVE_2_POW_53_TEXT
    );
    // Re-applying the persisted seed (Continue/Restart) is exact and isolated.
    let request = prepared_prompt(&db, imported["id"].as_str().unwrap());
    assert_eq!(
        request["prompt"]["11"]["inputs"]["seed"].as_u64(),
        Some(ABOVE_2_POW_53)
    );
    assert_eq!(request["prompt"]["6"]["inputs"]["seed"], json!(1));
    assert_eq!(request["prompt"]["7"]["inputs"]["seed"], json!(99));
}
