use serde_json::{json, Value};
use studio_core::contracts::Seed;
use studio_core::workflow_pipeline::{
    InputBinding, IntermediateState, PassKind, PassStatus, PortSpec, PortType, ResumeConditions,
    SourceRef, Step, StepId, WorkflowPipeline, WorkflowRef, PIPELINE_FORMAT, PIPELINE_VERSION,
};

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn port(name: &str, port_type: &str) -> Value {
    json!({"name": name, "portType": port_type})
}

fn binding(port: &str, node: &str, input: &str, source_step: &str, source_port: &str) -> Value {
    json!({"port": port, "nodeId": node, "input": input,
           "source": {"step": source_step, "port": source_port}})
}

fn linear_manifest() -> Value {
    // Steps deliberately listed out of dependency order: the DAG, not the list, decides.
    json!({
        "format": PIPELINE_FORMAT,
        "version": PIPELINE_VERSION,
        "id": "6f1e2d3c-4b5a-4678-9abc-def012345678",
        "name": "Trois passes Saya",
        "description": "Base, raffinement latent, finalisation image",
        "tags": ["couple", "three-pass"],
        "workflows": [
            {"id": "wf-base", "name": "Base SDXL", "sourceSha256": HASH},
            {"id": "wf-refine", "name": "Raffinement latent"},
            {"id": "wf-polish", "name": "Finalisation image"}
        ],
        "steps": [
            {
                "id": "polish", "workflowId": "wf-polish", "kind": "image",
                "name": "Finalisation",
                "inputs": [port("latent", "LATENT")],
                "outputs": [port("image", "IMAGE")],
                "bindings": [binding("latent", "88", "latent_image", "refine", "latent")],
                "status": "pending"
            },
            {
                "id": "refine", "workflowId": "wf-refine", "kind": "latent",
                "inputs": [port("latent", "LATENT")],
                "outputs": [port("latent", "LATENT")],
                "bindings": [binding("latent", "12", "samples", "base", "latent")],
                "seed": "42",
                "status": "failed",
                "state": {"path": "passes/refine/samples.safetensors", "portType": "LATENT"},
                "resume": {
                    "requiresState": true,
                    "requiresUpstreamCompleted": true,
                    "fromStatuses": ["failed", "submitted"],
                    "note": "Relancer après vérification du latent"
                }
            },
            {
                "id": "base", "workflowId": "wf-base", "kind": "image",
                "name": "Passe de base",
                "inputs": [],
                "outputs": [port("latent", "LATENT"), port("image", "IMAGE")],
                "bindings": [],
                "seed": "18446744073709551615",
                "parameters": {"P1": "saya, solo", "NEG": "blurry"},
                "status": "pending",
                "state": {"path": "passes/base/boundary.png", "portType": "IMAGE"}
            }
        ]
    })
}

fn parse(value: &Value) -> WorkflowPipeline {
    WorkflowPipeline::parse(&value.to_string()).unwrap()
}

fn rejection(value: Value) -> String {
    WorkflowPipeline::parse(&value.to_string())
        .unwrap_err()
        .to_string()
}

#[test]
fn linear_three_pass_pipeline_validates_and_orders_steps_from_the_dag() {
    let pipeline = parse(&linear_manifest());
    assert_eq!(
        pipeline.execution_order().unwrap(),
        ["base", "refine", "polish"]
    );
    assert_eq!(
        pipeline.execution_order().unwrap(),
        pipeline.execution_order().unwrap()
    );
    let base = &pipeline.steps[2];
    assert_eq!(base.id.as_str(), "base");
    assert_eq!(base.kind, PassKind::Image);
    let base_seed: String = base.seed.clone().unwrap().into();
    assert_eq!(base_seed, "18446744073709551615");
    assert_eq!(base.parameters["P1"], "saya, solo");
    assert_eq!(pipeline.steps[0].status, PassStatus::Pending);
    assert_eq!(pipeline.steps[1].status, PassStatus::Failed);
    let refine = &pipeline.steps[1];
    assert_eq!(refine.bindings[0].node_id, "12");
    assert_eq!(refine.bindings[0].input, "samples");
    assert_eq!(refine.bindings[0].source.step.as_str(), "base");
    assert_eq!(refine.bindings[0].source.port, "latent");
    assert_eq!(
        refine.resume.as_ref().unwrap().from_statuses,
        [PassStatus::Failed, PassStatus::Submitted]
    );
}

#[test]
fn branched_pipeline_orders_diamond_deterministically() {
    let manifest = json!({
        "format": PIPELINE_FORMAT,
        "version": PIPELINE_VERSION,
        "id": "pipeline-branched",
        "name": "Losange",
        "workflows": [
            {"id": "wf-a"}, {"id": "wf-b"}, {"id": "wf-c"}, {"id": "wf-d"}
        ],
        "steps": [
            {"id": "d", "workflowId": "wf-d", "kind": "image",
             "inputs": [port("a", "IMAGE"), port("b", "IMAGE")],
             "outputs": [port("image", "IMAGE")],
             "bindings": [binding("a", "9", "images", "b", "image"),
                          binding("b", "9", "images", "c", "image")]},
            {"id": "c", "workflowId": "wf-c", "kind": "image",
             "inputs": [port("latent", "LATENT")],
             "outputs": [port("image", "IMAGE")],
             "bindings": [binding("latent", "3", "samples", "a", "latent")]},
            {"id": "b", "workflowId": "wf-b", "kind": "latent",
             "inputs": [port("latent", "LATENT")],
             "outputs": [port("image", "IMAGE")],
             "bindings": [binding("latent", "3", "samples", "a", "latent")]},
            {"id": "a", "workflowId": "wf-a", "kind": "latent",
             "outputs": [port("latent", "LATENT")]}
        ]
    });
    let pipeline = parse(&manifest);
    let order = pipeline.execution_order().unwrap();
    assert_eq!(order.first().unwrap(), "a");
    assert_eq!(order.last().unwrap(), "d");
    assert_eq!(order.len(), 4);
    assert_eq!(order, pipeline.execution_order().unwrap());
}

#[test]
fn manifest_round_trips_exactly_and_preserves_the_u64_seed_text() {
    let first = parse(&linear_manifest());
    let text = first.to_json().unwrap();
    assert!(text.contains("\"seed\":\"18446744073709551615\""));
    assert!(!text.contains("18446744073709551615.0"));
    let second = WorkflowPipeline::parse(&text).unwrap();
    assert_eq!(first, second);
    assert_eq!(second.to_json().unwrap(), text);
}

#[test]
fn pipeline_built_in_rust_matches_the_json_format() {
    let pipeline = WorkflowPipeline {
        format: PIPELINE_FORMAT.into(),
        version: PIPELINE_VERSION,
        id: "pipeline-rust".into(),
        name: "Deux passes construites en Rust".into(),
        description: None,
        tags: vec![],
        workflows: vec![WorkflowRef {
            id: "wf-base".into(),
            name: None,
            source_sha256: None,
        }],
        steps: vec![
            Step {
                id: StepId::parse("base").unwrap(),
                workflow_id: "wf-base".into(),
                kind: PassKind::Image,
                name: Some("Base".into()),
                inputs: vec![],
                outputs: vec![PortSpec {
                    name: "latent".into(),
                    port_type: PortType::parse("LATENT").unwrap(),
                }],
                bindings: vec![],
                seed: Seed::parse("18446744073709551615").ok(),
                parameters: Default::default(),
                status: PassStatus::Pending,
                state: None,
                resume: None,
            },
            Step {
                id: StepId::parse("refine").unwrap(),
                workflow_id: "wf-base".into(),
                kind: PassKind::Latent,
                name: None,
                inputs: vec![PortSpec {
                    name: "latent".into(),
                    port_type: PortType::parse("LATENT").unwrap(),
                }],
                outputs: vec![],
                bindings: vec![InputBinding {
                    port: "latent".into(),
                    node_id: "12".into(),
                    input: "samples".into(),
                    source: SourceRef {
                        step: StepId::parse("base").unwrap(),
                        port: "latent".into(),
                    },
                }],
                seed: None,
                parameters: Default::default(),
                status: PassStatus::Prepared,
                state: Some(IntermediateState {
                    path: "passes/refine/samples.safetensors".into(),
                    port_type: PortType::parse("LATENT").unwrap(),
                }),
                resume: Some(ResumeConditions {
                    requires_state: true,
                    requires_upstream_completed: true,
                    from_statuses: vec![PassStatus::Failed],
                    note: None,
                }),
            },
        ],
    };
    pipeline.validate().unwrap();
    assert_eq!(pipeline.execution_order().unwrap(), ["base", "refine"]);
    let parsed = WorkflowPipeline::parse(&pipeline.to_json().unwrap()).unwrap();
    assert_eq!(parsed, pipeline);
}

#[test]
fn incompatible_port_types_are_rejected_explicitly() {
    let mut manifest = linear_manifest();
    manifest["steps"][0]["bindings"] =
        json!([binding("latent", "88", "latent_image", "base", "image")]);
    let error = rejection(manifest);
    assert!(error.contains("incompatibles"), "{error}");
    assert!(
        error.contains("LATENT") && error.contains("IMAGE"),
        "{error}"
    );
    // Wildcard ports stay compatible, mirroring the engine's own validate_graph.
    let mut wildcard = linear_manifest();
    wildcard["steps"][0]["inputs"] = json!([port("latent", "*")]);
    wildcard["steps"][0]["bindings"] =
        json!([binding("latent", "88", "latent_image", "base", "image")]);
    parse(&wildcard);
}

#[test]
fn unknown_step_port_and_workflow_references_are_rejected() {
    let mut unknown_step = linear_manifest();
    unknown_step["steps"][1]["bindings"] =
        json!([binding("latent", "12", "samples", "absent", "latent")]);
    assert!(rejection(unknown_step).contains("étape source inconnue"));

    let mut unknown_port = linear_manifest();
    unknown_port["steps"][1]["bindings"] =
        json!([binding("latent", "12", "samples", "base", "inconnu")]);
    assert!(rejection(unknown_port).contains("aucun port de sortie"));

    let mut unknown_workflow = linear_manifest();
    unknown_workflow["steps"][0]["workflowId"] = json!("wf-absent");
    assert!(rejection(unknown_workflow).contains("absent de la liste déclarée"));
}

#[test]
fn cycles_are_rejected_with_clear_diagnostics() {
    let mut cycle = linear_manifest();
    cycle["steps"][2]["inputs"] = json!([port("image", "IMAGE")]);
    cycle["steps"][2]["bindings"] = json!([binding("image", "1", "images", "polish", "image")]);
    let error = rejection(cycle);
    assert!(error.contains("Cycle détecté"), "{error}");
    assert!(
        error.contains("base") && error.contains("polish"),
        "{error}"
    );

    let mut self_loop = linear_manifest();
    self_loop["steps"][1]["bindings"] =
        json!([binding("latent", "12", "samples", "refine", "latent")]);
    let error = rejection(self_loop);
    assert!(
        error.contains("Cycle détecté") && error.contains("refine"),
        "{error}"
    );
}

#[test]
fn duplicate_step_ids_are_rejected() {
    let mut manifest = linear_manifest();
    let refine = manifest["steps"][1].clone();
    manifest["steps"].as_array_mut().unwrap().push(refine);
    let error = rejection(manifest);
    assert!(error.contains("déjà utilisé"), "{error}");
}

#[test]
fn unbound_duplicated_and_undeclared_input_ports_are_rejected() {
    let mut unbound = linear_manifest();
    unbound["steps"][1]["bindings"] = json!([]);
    assert!(rejection(unbound).contains("non alimenté"));

    let mut duplicated = linear_manifest();
    duplicated["steps"][1]["bindings"] = json!([
        binding("latent", "12", "samples", "base", "latent"),
        binding("latent", "12", "samples", "base", "latent")
    ]);
    assert!(rejection(duplicated).contains("lié deux fois"));

    let mut undeclared = linear_manifest();
    undeclared["steps"][1]["bindings"] = json!([
        binding("latent", "12", "samples", "base", "latent"),
        binding("extra", "13", "samples", "base", "latent")
    ]);
    assert!(rejection(undeclared).contains("port d'entrée non déclaré"));
}

#[test]
fn wrong_format_version_and_invalid_seeds_are_rejected() {
    let mut wrong_version = linear_manifest();
    wrong_version["version"] = json!(3);
    assert!(rejection(wrong_version).contains("Format de pipeline inconnu"));

    let mut wrong_format = linear_manifest();
    wrong_format["format"] = json!("autre-format");
    assert!(rejection(wrong_format).contains("Format de pipeline inconnu"));

    for seed in ["007", "abc", "18446744073709551616", "-1"] {
        let mut manifest = linear_manifest();
        manifest["steps"][1]["seed"] = json!(seed);
        let error = rejection(manifest);
        assert!(error.contains("invalide"), "{seed} : {error}");
    }

    let mut empty = linear_manifest();
    empty["steps"] = json!([]);
    assert!(rejection(empty).contains("au moins une étape"));
}

#[test]
fn resume_state_and_intermediate_paths_round_trip_and_fail_safely() {
    let pipeline = parse(&linear_manifest());
    let text = pipeline.to_json().unwrap();
    let reopened = WorkflowPipeline::parse(&text).unwrap();
    assert_eq!(reopened, pipeline);
    let refine = &reopened.steps[1];
    let resume = refine.resume.as_ref().unwrap();
    assert!(resume.requires_state && resume.requires_upstream_completed);
    assert_eq!(
        resume.note.as_deref(),
        Some("Relancer après vérification du latent")
    );
    assert_eq!(
        refine.state.as_ref().unwrap().path,
        "passes/refine/samples.safetensors"
    );

    let mut mismatched_state = linear_manifest();
    mismatched_state["steps"][1]["state"]["portType"] = json!("IMAGE");
    assert!(rejection(mismatched_state).contains("incohérent"));

    let mut absolute = linear_manifest();
    absolute["steps"][1]["state"]["path"] = json!("/etc/saya/latent.bin");
    assert!(rejection(absolute).contains("absolu refusé"));

    let mut escape = linear_manifest();
    escape["steps"][1]["state"]["path"] = json!("passes/../../escape.bin");
    assert!(rejection(escape).contains("hors du dossier pipeline refusé"));

    let mut bad_hash = linear_manifest();
    bad_hash["workflows"][0]["sourceSha256"] = json!("ZZZZ");
    assert!(rejection(bad_hash).contains("Empreinte source invalide"));
}

#[test]
fn handoff_manifest_example_stays_parseable() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/v11-handoff/workflow-fusion.md");
    let doc = std::fs::read_to_string(&path).unwrap();
    let start = doc.find("```json").expect("exemple JSON absent du handoff") + "```json".len();
    let end = doc[start..].find("```").expect("bloc JSON non fermé") + start;
    let pipeline = WorkflowPipeline::parse(doc[start..end].trim()).unwrap();
    assert_eq!(
        pipeline.execution_order().unwrap(),
        ["base", "refine", "polish"]
    );
    let refine = pipeline
        .steps
        .iter()
        .find(|step| step.id.as_str() == "refine")
        .unwrap();
    assert_eq!(refine.status, PassStatus::Failed);
    let resume = refine.resume.as_ref().unwrap();
    assert!(resume.requires_state && resume.requires_upstream_completed);
    assert_eq!(refine.bindings[0].source.step.as_str(), "base");
}

#[test]
fn unknown_fields_are_rejected_at_every_level() {
    let mut root = linear_manifest();
    root["champ_inconnu"] = json!(1);
    assert!(rejection(root).contains("unknown field"));

    let mut step = linear_manifest();
    step["steps"][0]["champ_inconnu"] = json!(1);
    assert!(rejection(step).contains("unknown field"));
}

#[test]
fn v2_preserves_complete_prompts_without_changing_v1() {
    let mut value = linear_manifest();
    let text = "  (weighted:1.25), été 🌸\nsecond line\r\n\tend  ";
    value["steps"][0]["parameters"]["P1"] = json!(text);
    assert!(WorkflowPipeline::parse(&value.to_string()).is_err());
    value["version"] = json!(2);
    value["steps"][0]["parameters"]["NEG"] = json!("");
    let pipeline = WorkflowPipeline::parse(&value.to_string()).unwrap();
    assert_eq!(pipeline.steps[0].parameters["P1"], text);
    let again = WorkflowPipeline::parse(&pipeline.to_json().unwrap()).unwrap();
    assert_eq!(again, pipeline);
    value["steps"][0]["parameters"]["P1"] = json!("null\u{0000}byte");
    assert!(WorkflowPipeline::parse(&value.to_string()).is_err());
    value["steps"][0]["parameters"]["P1"] = json!(text);
    value["name"] = json!("name\nunsafe");
    assert!(WorkflowPipeline::parse(&value.to_string()).is_err());
}
