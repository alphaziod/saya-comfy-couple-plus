use serde_json::{json, Value};
use studio_core::{gallery::Filters, generation_metadata::inspect, Database};

fn graph() -> Value {
    json!({"prompt":{
     "mode":{"class_type":"PrimitiveString","inputs":{"value":"solo"}},
     "solo":{"class_type":"StringCompare","inputs":{"string_a":["mode",0],"string_b":"solo","mode":"Equal","case_sensitive":true}},
     "p1":{"class_type":"PrimitiveStringMultiline","inputs":{"value":"  exact, (tag:1.2)\nété 🌸\n"},"_meta":{"title":"Prompt · Person 1"}},
     "anatomy":{"class_type":"PrimitiveStringMultiline","inputs":{"value":"anatomy  "},"_meta":{"title":"P1 ANATOMY"}},
     "join":{"class_type":"StringConcatenate","inputs":{"string_a":["anatomy",0],"string_b":["p1",0],"delimiter":",\n"}},
     "c1":{"class_type":"CLIPTextEncode","inputs":{"text":["join",0]},"_meta":{"title":"P1 encoder"}},
     "p2":{"class_type":"PrimitiveStringMultiline","inputs":{"value":"P2 private inactive"},"_meta":{"title":"Prompt · Person 2"}},
     "c2":{"class_type":"CLIPTextEncode","inputs":{"text":["p2",0]},"_meta":{"title":"P2 encoder"}},
     "neg":{"class_type":"CLIPTextEncode","inputs":{"text":""}},
     "couple":{"class_type":"SayaMultiCouple","inputs":{"solo":["solo",0],"pos_1":["c1",0],"pos_2":["c2",0],"neg_1":["neg",0]}},
     "seed":{"class_type":"Seed (rgthree)","inputs":{"seed":18446744073709551615u64}},
     "sample":{"class_type":"KSampler","inputs":{"positive":["couple",2],"negative":["neg",0],"seed":["seed",0],"steps":27,"cfg":5.5}},
     "save":{"class_type":"SaveImage","inputs":{"images":["sample",0]}}
    }})
}

#[test]
fn real_saya_shapes_resolve_solo_gates_sections_and_exact_u64() {
    let metadata = graph();
    let result = inspect(&metadata, None);
    assert_eq!(result["mode"], "solo");
    let sections = result["sections"].as_array().unwrap();
    assert!(!sections
        .iter()
        .any(|s| s["role"].as_str().unwrap().starts_with("P2")));
    assert!(sections
        .iter()
        .any(|s| s["role"] == "P1 ANATOMY" && s["content"] == "anatomy  "));
    assert!(sections
        .iter()
        .any(|s| s["content"] == "  exact, (tag:1.2)\nété 🌸\n"));
    assert!(sections
        .iter()
        .any(|s| s["role"] == "NEG" && s["status"] == "empty"));
    assert!(sections
        .iter()
        .any(|s| s["content"] == "anatomy  ,\n  exact, (tag:1.2)\nété 🌸\n"));
    let params = result["parameters"].as_array().unwrap();
    assert!(params
        .iter()
        .filter(|p| p["node"] == "sample")
        .flat_map(|p| p["values"].as_array().unwrap())
        .any(|p| p["name"] == "seed" && p["value"] == "18446744073709551615"));
    assert!(result["rawJson"]
        .as_str()
        .unwrap()
        .contains("18446744073709551615"));
    let mut couple = metadata;
    couple["prompt"]["mode"]["inputs"]["value"] = json!("couple");
    let result = inspect(&couple, None);
    assert_eq!(result["mode"], "couple");
    assert!(result["sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["role"] == "P2"));
}

#[test]
fn unresolved_dynamic_nodes_and_cycles_never_invent_prompts_or_mode() {
    let mut text = graph();
    text["prompt"]["mode"]["class_type"] = json!("UnknownDynamicString");
    let result = inspect(&text, None);
    assert_eq!(result["mode"], "indeterminate");
    text["prompt"]["p1"]["inputs"]["value"] = json!(["p1", 0]);
    let result = inspect(&text, None);
    assert!(result["sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["node"] == "p1" && s["status"] == "unparseable" && s["content"].is_null()));
    assert_eq!(
        inspect(&json!({"prompt":"broken","unknown":" preserve  \n"}), None)["sections"][0]
            ["status"],
        "absent"
    );
}

#[test]
fn selected_switch_branch_only_and_lora_weights_are_reported() {
    let text = json!({"prompt":{
     "a":{"class_type":"CLIPTextEncode","inputs":{"text":"active"},"_meta":{"title":"P1"}},
     "b":{"class_type":"CLIPTextEncode","inputs":{"text":"inactive"},"_meta":{"title":"P2"}},
     "pick":{"class_type":"SayaLazyBooleanSelect","inputs":{"switch":true,"on_true":["a",0],"on_false":["b",0]}},
     "lora":{"class_type":"Lora Loader (LoraManager)","inputs":{"loras":{"__value__":[{"name":"example","active":true,"strength":0.7},{"name":"off","active":false}]}}},
     "sample":{"class_type":"KSampler","inputs":{"positive":["pick",0],"model":["lora",0],"seed":18446744073709551615u64}},
     "save":{"class_type":"SaveImage","inputs":{"images":["sample",0]}}
    }});
    let result = inspect(&text, None);
    assert!(!result["sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["content"] == "inactive"));
    assert_eq!(result["models"].as_array().unwrap().len(), 1);
    assert_eq!(result["models"][0]["strengthModel"], "0.7");
}

#[test]
fn metadata_filter_rejects_plain_images_and_preserves_raw_seed() {
    let dir = std::env::temp_dir().join(format!("saya-v15-meta-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    image::RgbImage::new(2, 2)
        .save(dir.join("plain.png"))
        .unwrap();
    let file = std::fs::File::create(dir.join("generation.png")).unwrap();
    let mut encoder = png::Encoder::new(file, 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_itxt_chunk("prompt".into(), graph()["prompt"].to_string())
        .unwrap();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[0; 12])
        .unwrap();
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let page = db
        .gallery(&Filters {
            metadata_only: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, 1);
    let info = db.generation_metadata(&page.items[0].id).unwrap();
    assert_eq!(info["mode"], "solo");
    assert!(info["originalMetadataJson"]
        .as_str()
        .unwrap()
        .contains("18446744073709551615"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dasiwa_parameters_keep_declared_prompts_and_seed_as_text() {
    let result = inspect(
        &json!({"parameters":"  tag, (weight:1.2)\nsecond line\nNegative prompt: bad  \n\nSteps: 27, Sampler: euler, CFG scale: 5.5, Seed: 18446744073709551615, Lora hashes: {\"one\": \"hash,a\"}"}),
        None,
    );
    let sections = result["sections"].as_array().unwrap();
    assert!(sections
        .iter()
        .any(|s| s["role"] == "POS" && s["content"] == "  tag, (weight:1.2)\nsecond line"));
    assert!(sections
        .iter()
        .any(|s| s["role"] == "NEG" && s["content"] == "bad  \n"));
    let values = result["parameters"][0]["values"].as_array().unwrap();
    assert!(values
        .iter()
        .any(|v| v["name"] == "Seed" && v["value"] == "18446744073709551615"));
    assert!(values
        .iter()
        .any(|v| v["name"] == "Lora hashes" && v["value"] == "{\"one\": \"hash,a\"}"));
}
