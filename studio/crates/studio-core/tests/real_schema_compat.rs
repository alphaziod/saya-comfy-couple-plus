//! Regression tests for ComfyUI 0.34/custom-node schemas encountered by the
//! real Saya Couple preflight. No engine is called.
use serde_json::json;

#[test]
fn v3_combo_matchtype_customcombo_and_power_puter_are_accepted_explicitly() {
    let info = json!({
        "CustomCombo":{"input":{"required":{"choice":["COMBO",{"multiselect":false,"options":[]}]}},"output":["STRING","INT"]},
        "Power Puter (rgthree)":{"input":{"required":{},"optional":{}},"output":["*"]},
        "CLIPTextEncode":{"input":{"required":{"text":["STRING"],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["ck.safetensors"]]}},"output":["MODEL","CLIP","VAE"]}
    });
    let graph = json!({
        "1":{"class_type":"CustomCombo","inputs":{"choice":"Model 2","index":1,"option1":"Model 1","option2":"Model 2"}},
        "2":{"class_type":"Power Puter (rgthree)","inputs":{"a":"x","code":"return a","outputs":{"outputs":["*"]}}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":["1",0],"clip":["4",1]}},
        "4":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck.safetensors"}}
    });
    studio_core::workflow::validate_graph(&graph, &info).unwrap();
}

#[test]
fn lora_manager_selected_field_and_combo_contract_are_supported() {
    let info = json!({
        "Lora Loader (LoraManager)":{"input":{"required":{"model":["MODEL"],"text":["AUTOCOMPLETE_TEXT_LORAS",{}],"loras":["LORAS",{}]}},"output":["MODEL","CLIP","STRING","STRING"]},
        "CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["ck.safetensors"]]}},"output":["MODEL","CLIP","VAE"]}
    });
    let graph = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck.safetensors"}},
        "2":{"class_type":"Lora Loader (LoraManager)","inputs":{"model":["1",0],"text":"<lora:x:1:1>","loras":{"__value__":[{"name":"x","strength":"1","clipStrength":"1","active":true,"expanded":false,"locked":false,"selected":true}]}}}
    });
    studio_core::workflow::validate_graph(&graph, &info).unwrap();
    let mut bad = graph.clone();
    bad["2"]["inputs"]["loras"]["__value__"][0]["selected"] = json!("yes");
    assert!(studio_core::workflow::validate_graph(&bad, &info).is_err());
}
