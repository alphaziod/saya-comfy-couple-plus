use serde_json::json;
use std::path::Path;
use studio_core::engine;

#[test]
fn rejects_unknown_tools_and_missing_engine() {
    assert!(engine::call(Path::new("/missing"), "execute", &json!({})).is_err());
    assert!(engine::call(
        Path::new("/missing"),
        "analyze",
        &json!({"prompt":"P1: exact"})
    )
    .is_err());
}
#[test]
#[ignore = "Requires local Saya Prompt Engine source; set SAYA_PROMPT_ENGINE_ROOT"]
fn local_engine_preserves_tags_and_inspects_subgraphs_without_conversion() {
    let root = std::env::var("SAYA_PROMPT_ENGINE_ROOT").expect("SAYA_PROMPT_ENGINE_ROOT");
    let root = Path::new(&root);
    let original="QUALITY: detailed\nP1: (silver hair, blue eyes:1.2), exact\\, tag\nP2: separate\nACTION: standing\nBG: outdoors\nNEG: blur";
    let proposal=engine::call(root,"optimize",&json!({"prompt":original,"target":"P1","loras":[{"name":"fixture","documented_triggers":[{"tag":"silver ribbon","source":"user documented fixture"}]}]})).unwrap();
    assert_eq!(proposal["result"]["original_prompt"], original);
    assert_eq!(proposal["result"]["structural_validation"]["valid"], true);
    assert!(proposal["result"]["full_prompt"]
        .as_str()
        .unwrap()
        .contains("silver ribbon"));
    assert_eq!(proposal["result"]["status"], "proposed_unvalidated");
    let removed = engine::call(
        root,
        "validate",
        &json!({"original":original,"proposed":"P1: different"}),
    )
    .unwrap();
    assert_eq!(removed["result"]["valid"], false);
    let graph = json!({"nodes":[{"id":1,"type":"sub"}],"links":[],"definitions":{"subgraphs":[{"id":"sub","nodes":[{"id":2,"type":"CLIPTextEncode","widgets_values":["exact tags"]}],"links":[]}]}});
    let inspected = engine::call(root, "inspect", &json!({"graph":graph})).unwrap();
    assert_eq!(inspected["result"]["format"], "ui");
    assert_eq!(inspected["result"]["exposedInputsVersion"], 1);
    assert!(inspected["result"]["graphs"].as_array().unwrap().len() >= 2);
    assert!(inspected["sourceSha256"].as_str().unwrap().len() == 64);
}
