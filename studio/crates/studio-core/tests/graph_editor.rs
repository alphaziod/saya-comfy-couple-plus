use serde_json::{json, Value};
use studio_core::graph_editor::{
    Command, Connection, EditorDocument, EditorGraph, EditorGroup, EditorNode, ExposedPort,
    InputPort, LinkEnd, LinkEndInput, OutputPort, Point, SubgraphContainer, MAX_HISTORY,
};
use studio_core::workflow::validate_graph;

fn schema() -> Value {
    json!({
        "CheckpointLoaderSimple": {
            "input": {"required": {"ckpt_name": ["STRING"]}},
            "output": ["MODEL", "CLIP", "VAE"],
            "output_name": ["MODEL", "CLIP", "VAE"]
        },
        "CLIPTextEncode": {
            "input": {"required": {"text": ["STRING"], "clip": ["CLIP"]}},
            "output": ["CONDITIONING"]
        },
        "EmptyLatentImage": {
            "input": {"required": {"width": ["INT"], "height": ["INT"], "batch": ["INT"]}},
            "output": ["LATENT"]
        },
        "KSampler": {
            "input": {"required": {
                "seed": ["INT"], "steps": ["INT"], "cfg": ["FLOAT"],
                "sampler_name": [["euler", "heun", "dpm2"], {}],
                "scheduler": [["normal", "karras"], {}],
                "denoise": ["FLOAT"],
                "model": ["MODEL"], "positive": ["CONDITIONING"],
                "negative": ["CONDITIONING"], "latent_image": ["LATENT"]
            }},
            "output": ["LATENT"]
        },
        "VAEDecode": {
            "input": {"required": {"samples": ["LATENT"], "vae": ["VAE"]}},
            "output": ["IMAGE"]
        },
        "SaveImage": {
            "input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING"]}},
            "output": [],
            "output_node": true
        }
    })
}

fn executed_solo() -> Value {
    json!({
        "ckpt": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "model.safetensors"}},
        "pos": {"class_type": "CLIPTextEncode", "inputs": {"text": "saya", "clip": ["ckpt", 1]}},
        "neg": {"class_type": "CLIPTextEncode", "inputs": {"text": "flou", "clip": ["ckpt", 1]}},
        "latent": {"class_type": "EmptyLatentImage", "inputs": {"width": 1024, "height": 1024, "batch": 1}},
        "ksampler": {"class_type": "KSampler", "inputs": {
            "seed": 18446744073709551615u64, "steps": 20, "cfg": 7.0,
            "sampler_name": "euler", "scheduler": "normal", "denoise": 1.0,
            "model": ["ckpt", 0], "positive": ["pos", 0],
            "negative": ["neg", 0], "latent_image": ["latent", 0]
        }},
        "decode": {"class_type": "VAEDecode", "inputs": {"samples": ["ksampler", 0], "vae": ["ckpt", 2]}},
        "save": {"class_type": "SaveImage", "inputs": {"images": ["decode", 0], "filename_prefix": "saya"}}
    })
}

fn node(
    id: &str,
    class: &str,
    inputs: &[(&str, &str, bool)],
    outputs: &[(&str, &str)],
) -> EditorNode {
    EditorNode {
        id: id.to_owned(),
        class_type: class.to_owned(),
        title: String::new(),
        color: None,
        position: None,
        size: None,
        inputs: inputs
            .iter()
            .map(|(name, kind, optional)| InputPort {
                name: (*name).to_owned(),
                port_type: (*kind).to_owned(),
                optional: *optional,
            })
            .collect(),
        outputs: outputs
            .iter()
            .map(|(name, kind)| OutputPort {
                name: (*name).to_owned(),
                port_type: (*kind).to_owned(),
            })
            .collect(),
        params: Default::default(),
    }
}

fn connect(graph: &EditorGraph, from: &str, output: usize, to: &str, input: &str) -> Command {
    Command::Connect {
        connection: Connection {
            id: graph.new_id(),
            from: LinkEnd {
                node: from.to_owned(),
                output,
            },
            to: LinkEndInput {
                node: to.to_owned(),
                input: input.to_owned(),
            },
        },
    }
}

fn node_id(graph: &EditorGraph, class: &str) -> String {
    graph
        .document()
        .nodes
        .iter()
        .find(|n| n.class_type == class)
        .unwrap()
        .id
        .clone()
}

#[test]
fn executed_round_trip_preserves_structure_types_and_u64_seed() {
    let graph = EditorGraph::from_executed(&executed_solo(), &schema(), "SOLO réel").unwrap();
    let document = graph.document();
    assert_eq!(document.nodes.len(), 7);
    assert_eq!(document.connections.len(), 9);
    assert!(document
        .nodes
        .iter()
        .all(|n| n.position.is_none() && n.size.is_none()));
    let ksampler = document
        .nodes
        .iter()
        .find(|n| n.class_type == "KSampler")
        .unwrap();
    assert_eq!(ksampler.params["seed"], json!(18446744073709551615u64));
    let ckpt = document
        .nodes
        .iter()
        .find(|n| n.class_type == "CheckpointLoaderSimple")
        .unwrap();
    assert_eq!(
        ckpt.outputs
            .iter()
            .map(|p| p.port_type.as_str())
            .collect::<Vec<_>>(),
        vec!["MODEL", "CLIP", "VAE"]
    );
    let sampler_port = ksampler.input_port("sampler_name").unwrap();
    assert_eq!(sampler_port.port_type, "COMBO");
    assert!(!sampler_port.optional);
    // Export keeps the editor's stable identifiers: the comparison
    // is made against the fixture remapped to those identifiers.
    let executed = graph.to_executed().unwrap();
    assert_eq!(executed, expected_executed(&graph));
    assert!(validate_graph(&executed, &schema()).is_ok());
    let text = graph.serialize().unwrap();
    let reloaded = EditorGraph::deserialize(&text).unwrap();
    assert_eq!(reloaded.serialize().unwrap(), text);
    assert_eq!(reloaded.to_executed().unwrap(), executed);
    assert!(reloaded.missing_required_inputs().is_empty());
}

/// SOLO fixture remapped to the stable identifiers of the edited graph.
fn expected_executed(graph: &EditorGraph) -> Value {
    let find = |class: &str, text: Option<&str>| {
        graph
            .document()
            .nodes
            .iter()
            .find(|n| {
                n.class_type == class
                    && text.is_none_or(|t| n.params.get("text").and_then(Value::as_str) == Some(t))
            })
            .unwrap()
            .id
            .clone()
    };
    let ckpt = find("CheckpointLoaderSimple", None);
    let pos = find("CLIPTextEncode", Some("saya"));
    let neg = find("CLIPTextEncode", Some("flou"));
    let latent = find("EmptyLatentImage", None);
    let ksampler = find("KSampler", None);
    let decode = find("VAEDecode", None);
    let save = find("SaveImage", None);
    let mut out = serde_json::Map::new();
    out.insert(
        ckpt.clone(),
        json!({"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "model.safetensors"}}),
    );
    out.insert(
        pos.clone(),
        json!({"class_type": "CLIPTextEncode", "inputs": {"text": "saya", "clip": [&ckpt, 1]}}),
    );
    out.insert(
        neg.clone(),
        json!({"class_type": "CLIPTextEncode", "inputs": {"text": "flou", "clip": [&ckpt, 1]}}),
    );
    out.insert(
        latent.clone(),
        json!({"class_type": "EmptyLatentImage", "inputs": {"width": 1024, "height": 1024, "batch": 1}}),
    );
    out.insert(
        ksampler.clone(),
        json!({"class_type": "KSampler", "inputs": {
            "seed": 18446744073709551615u64, "steps": 20, "cfg": 7.0,
            "sampler_name": "euler", "scheduler": "normal", "denoise": 1.0,
            "model": [&ckpt, 0], "positive": [&pos, 0],
            "negative": [&neg, 0], "latent_image": [&latent, 0]
        }}),
    );
    out.insert(
        decode.clone(),
        json!({"class_type": "VAEDecode", "inputs": {"samples": [&ksampler, 0], "vae": [&ckpt, 2]}}),
    );
    out.insert(
        save,
        json!({"class_type": "SaveImage", "inputs": {"images": [&decode, 0], "filename_prefix": "saya"}}),
    );
    Value::Object(out)
}

#[test]
fn connection_rules_reject_duplicates_self_type_mismatch_cycles_and_dangling() {
    let mut graph = EditorGraph::new("règles de connexion").unwrap();
    let source = "source-node".to_owned();
    let target = "target-node".to_owned();
    let clip_target = "clip-target".to_owned();
    let loopback = "loopback-node".to_owned();
    let other_loop = "other-loop".to_owned();
    for (id, inputs, outputs) in [
        (source.clone(), vec![], vec![("out", "MODEL")]),
        (
            target.clone(),
            vec![("in", "MODEL", false)],
            vec![("out", "CLIP")],
        ),
        (clip_target.clone(), vec![("in", "CLIP", true)], vec![]),
        (
            loopback.clone(),
            vec![("in", "MODEL", false)],
            vec![("out", "MODEL")],
        ),
        (
            other_loop.clone(),
            vec![("in", "MODEL", false)],
            vec![("out", "MODEL")],
        ),
    ] {
        graph
            .apply(Command::AddNode {
                node: node(&id, "Test", &inputs, &outputs),
            })
            .unwrap();
    }
    let depth = graph.undo_depth();
    let valid = connect(&graph, &source, 0, &target, "in");
    graph.apply(valid).unwrap();
    let before = graph.serialize().unwrap();
    // Duplicate: same target input.
    let error = graph
        .apply(connect(&graph, &source, 0, &target, "in"))
        .unwrap_err();
    assert!(error.to_string().contains("déjà connectée"), "{error}");
    // Types incompatibles : MODEL vers CLIP.
    let error = graph
        .apply(connect(&graph, &source, 0, &clip_target, "in"))
        .unwrap_err();
    assert!(error.to_string().contains("incompatibles"), "{error}");
    // Index de sortie invalide.
    assert!(graph
        .apply(connect(&graph, &source, 3, &target, "in"))
        .is_err());
    // Nonexistent input.
    assert!(graph
        .apply(connect(&graph, &source, 0, &target, "aucune"))
        .is_err());
    // Auto-connexion.
    let error = graph
        .apply(connect(&graph, &loopback, 0, &loopback, "in"))
        .unwrap_err();
    assert!(error.to_string().contains("Auto-connexion"), "{error}");
    // No failure leaves a trace in the document or the history.
    assert_eq!(graph.serialize().unwrap(), before);
    assert_eq!(graph.undo_depth(), depth + 1);
    // Cycle: autre-boucle -> boucle then boucle -> autre-boucle.
    graph
        .apply(connect(&graph, &other_loop, 0, &loopback, "in"))
        .unwrap();
    let error = graph
        .apply(connect(&graph, &loopback, 0, &other_loop, "in"))
        .unwrap_err();
    assert!(error.to_string().contains("Cycle"), "{error}");
    assert_eq!(graph.undo_depth(), depth + 2);
    // Disconnect then reconnect with a new identifier.
    let connection = graph.document().connections[0].id.clone();
    graph.apply(Command::Disconnect { connection }).unwrap();
    assert_eq!(graph.document().connections.len(), 1);
    graph
        .apply(connect(&graph, &source, 0, &target, "in"))
        .unwrap();
    assert_eq!(graph.document().connections.len(), 2);
}

#[test]
fn node_ids_are_unique_and_never_recycled() {
    let mut graph = EditorGraph::new("identifiants").unwrap();
    let first = graph.new_id();
    graph
        .apply(Command::AddNode {
            node: node(&first, "Test", &[], &[("out", "MODEL")]),
        })
        .unwrap();
    graph
        .apply(Command::RemoveNode {
            node: first.clone(),
        })
        .unwrap();
    let second = graph.new_id();
    graph
        .apply(Command::AddNode {
            node: node(&second, "Test", &[], &[("out", "MODEL")]),
        })
        .unwrap();
    assert_ne!(first, second, "un identifiant retiré n'est jamais recyclé");
    let mut seen = std::collections::BTreeSet::new();
    let mut ids = Vec::new();
    for _ in 0..50 {
        let id = graph.new_id();
        ids.push(id.clone());
        assert!(seen.insert(id), "doublon d'identifiant généré");
    }
    // Connection identifiers follow the same uniqueness rule.
    let a = graph.new_id();
    graph
        .apply(Command::AddNode {
            node: node(&a, "Test", &[("in", "MODEL", false)], &[]),
        })
        .unwrap();
    let connection = Connection {
        id: graph.new_id(),
        from: LinkEnd {
            node: second.clone(),
            output: 0,
        },
        to: LinkEndInput {
            node: a.clone(),
            input: "in".to_owned(),
        },
    };
    let connection_id = connection.id.clone();
    graph.apply(Command::Connect { connection }).unwrap();
    assert!(graph
        .apply(Command::Connect {
            connection: Connection {
                id: connection_id,
                from: LinkEnd {
                    node: second,
                    output: 0
                },
                to: LinkEndInput {
                    node: a,
                    input: "in".to_owned()
                },
            },
        })
        .is_err());
}

#[test]
fn undo_redo_restores_exact_state_and_is_bounded() {
    let mut graph = EditorGraph::new("historique").unwrap();
    let initial = graph.serialize().unwrap();
    let alpha = graph.new_id();
    let beta = graph.new_id();
    graph
        .apply(Command::AddNode {
            node: node(
                &alpha,
                "Test",
                &[("steps", "INT", true), ("in", "MODEL", false)],
                &[],
            ),
        })
        .unwrap();
    graph
        .apply(Command::AddNode {
            node: node(&beta, "Test", &[], &[("out", "MODEL")]),
        })
        .unwrap();
    graph
        .apply(Command::MoveNode {
            node: alpha.clone(),
            position: Some(Point { x: 100.0, y: 50.0 }),
        })
        .unwrap();
    graph
        .apply(Command::SetParam {
            node: alpha.clone(),
            input: "steps".to_owned(),
            value: json!(42),
        })
        .unwrap();
    graph
        .apply(connect(&graph, &beta, 0, &alpha, "in"))
        .unwrap();
    let final_state = graph.serialize().unwrap();
    for _ in 0..5 {
        assert!(graph.undo().unwrap());
    }
    assert!(!graph.undo().unwrap());
    assert_eq!(graph.serialize().unwrap(), initial);
    assert_eq!(graph.redo_depth(), 5);
    for _ in 0..5 {
        assert!(graph.redo().unwrap());
    }
    assert!(!graph.redo().unwrap());
    assert_eq!(graph.serialize().unwrap(), final_state);
    assert!(graph.undo().unwrap());
    assert!(graph.undo().unwrap());
    assert_ne!(graph.serialize().unwrap(), final_state);
    assert!(graph.redo().unwrap());
    assert_ne!(graph.serialize().unwrap(), final_state);
    assert!(graph.redo().unwrap());
    assert_eq!(graph.serialize().unwrap(), final_state);
    // Rejected command: the history stays intact.
    let undo_depth = graph.undo_depth();
    assert!(graph
        .apply(Command::RemoveNode {
            node: "inconnu".to_owned()
        })
        .is_err());
    assert_eq!(graph.undo_depth(), undo_depth);
    // Bounded history: only the last MAX_HISTORY commands can be
    // undone.
    for i in 0..250 {
        graph
            .apply(Command::MoveNode {
                node: alpha.clone(),
                position: Some(Point {
                    x: f64::from(i),
                    y: 0.0,
                }),
            })
            .unwrap();
    }
    assert_eq!(graph.undo_depth(), MAX_HISTORY);
    let mut undone = 0;
    while graph.undo().unwrap() {
        undone += 1;
    }
    assert_eq!(undone, MAX_HISTORY);
    let alpha_node = graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == alpha)
        .unwrap();
    assert_eq!(alpha_node.position, Some(Point { x: 49.0, y: 0.0 }));
}

#[test]
fn json_v1_round_trip_is_strict_on_version_and_unknown_fields() {
    let mut graph = EditorGraph::new("sérialisation").unwrap();
    let a = graph.new_id();
    let b = graph.new_id();
    graph
        .apply(Command::AddNode {
            node: node(&a, "Test", &[], &[("out", "MODEL")]),
        })
        .unwrap();
    graph
        .apply(Command::AddNode {
            node: node(&b, "Test", &[("in", "MODEL", false)], &[]),
        })
        .unwrap();
    graph.apply(connect(&graph, &a, 0, &b, "in")).unwrap();
    let text = graph.serialize().unwrap();
    let document: EditorDocument = serde_json::from_str(&text).unwrap();
    assert_eq!(document.format_version, 1);
    assert_eq!(
        EditorGraph::deserialize(&text)
            .unwrap()
            .serialize()
            .unwrap(),
        text
    );

    let mut future: Value = serde_json::from_str(&text).unwrap();
    future["formatVersion"] = json!(2);
    assert!(EditorGraph::deserialize(&future.to_string()).is_err());
    let mut unknown = future.clone();
    unknown["formatVersion"] = json!(1);
    unknown["futureField"] = json!(true);
    assert!(EditorGraph::deserialize(&unknown.to_string()).is_err());
    // Dangling connection: node removed from the document but links kept.
    let mut dangling: Value = serde_json::from_str(&text).unwrap();
    dangling["nodes"] = json!([]);
    assert!(EditorGraph::deserialize(&dangling.to_string()).is_err());
    // Input already connected twice.
    let mut duplicate: Value = serde_json::from_str(&text).unwrap();
    let target = duplicate["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "in")
        })
        .unwrap()["id"]
        .clone();
    let extra = json!({
        "id": "connexion-supplémentaire",
        "from": {"node": duplicate["nodes"][0]["id"].clone(), "output": 0},
        "to": {"node": target, "input": "in"}
    });
    duplicate["connections"].as_array_mut().unwrap().push(extra);
    assert!(EditorGraph::deserialize(&duplicate.to_string()).is_err());
}

#[test]
fn subgraph_containers_are_opaque_and_block_api_export() {
    let mut graph = EditorGraph::from_executed(&executed_solo(), &schema(), "SOLO").unwrap();
    let ckpt = node_id(&graph, "CheckpointLoaderSimple");
    let container_id = graph.new_id();
    let nodes_before = serde_json::to_string(&graph.document().nodes).unwrap();
    graph
        .apply(Command::AddSubgraph {
            container: SubgraphContainer {
                id: container_id.clone(),
                title: "Chargeur".to_owned(),
                node_ids: vec![ckpt.clone()],
                inputs: vec![ExposedPort {
                    name: "ckpt_name".to_owned(),
                    port_type: "STRING".to_owned(),
                }],
                outputs: vec![
                    ExposedPort {
                        name: "MODEL".to_owned(),
                        port_type: "MODEL".to_owned(),
                    },
                    ExposedPort {
                        name: "CLIP".to_owned(),
                        port_type: "CLIP".to_owned(),
                    },
                    ExposedPort {
                        name: "VAE".to_owned(),
                        port_type: "VAE".to_owned(),
                    },
                ],
            },
        })
        .unwrap();
    assert_eq!(
        serde_json::to_string(&graph.document().nodes).unwrap(),
        nodes_before,
        "l'ajout du conteneur ne touche à aucun node"
    );
    let after_add = graph.serialize().unwrap();
    // Opaque box: no structural change to its members.
    assert!(graph
        .apply(Command::SetParam {
            node: ckpt.clone(),
            input: "ckpt_name".to_owned(),
            value: json!("autre.safetensors"),
        })
        .is_err());
    assert!(graph
        .apply(Command::MoveNode {
            node: ckpt.clone(),
            position: Some(Point { x: 1.0, y: 1.0 })
        })
        .is_err());
    assert!(graph
        .apply(Command::RemoveNode { node: ckpt.clone() })
        .is_err());
    let pos = node_id(&graph, "CLIPTextEncode");
    let connection = graph
        .document()
        .connections
        .iter()
        .find(|c| c.from.node == ckpt && c.to.node == pos)
        .unwrap()
        .id
        .clone();
    assert!(graph.apply(Command::Disconnect { connection }).is_err());
    // A group cannot capture a member.
    assert!(graph
        .apply(Command::AddGroup {
            group: EditorGroup {
                id: graph.new_id(),
                title: "Groupe".to_owned(),
                node_ids: vec![ckpt.clone()],
                bounds: None,
                color: None,
            },
        })
        .is_err());
    // Moving the whole box, exactly reversible.
    graph
        .apply(Command::MoveSubgraph {
            container: container_id.clone(),
            delta: Point { x: 120.0, y: -40.0 },
        })
        .unwrap();
    let moved = graph.serialize().unwrap();
    assert!(graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == ckpt)
        .unwrap()
        .position
        .is_some());
    assert!(graph.undo().unwrap());
    assert_eq!(graph.serialize().unwrap(), after_add);
    assert!(graph.redo().unwrap());
    assert_eq!(graph.serialize().unwrap(), moved);
    // No arbitrary conversion: API export is refused while the
    // container exists, then the members come out unchanged.
    assert!(graph.to_executed().is_err());
    graph
        .apply(Command::RemoveSubgraph {
            container: container_id,
        })
        .unwrap();
    assert_eq!(graph.to_executed().unwrap(), expected_executed(&graph));
    assert!(graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == ckpt)
        .unwrap()
        .position
        .is_some());
}

#[test]
fn groups_move_assign_and_dissolve() {
    let mut graph = EditorGraph::from_executed(&executed_solo(), &schema(), "SOLO").unwrap();
    let latent = node_id(&graph, "EmptyLatentImage");
    let ksampler = node_id(&graph, "KSampler");
    let group_id = graph.new_id();
    graph
        .apply(Command::AddGroup {
            group: EditorGroup {
                id: group_id.clone(),
                title: "Échantillonnage".to_owned(),
                node_ids: vec![latent.clone(), ksampler.clone()],
                bounds: None,
                color: None,
            },
        })
        .unwrap();
    // A node belongs to one group only.
    assert!(graph
        .apply(Command::AddGroup {
            group: EditorGroup {
                id: graph.new_id(),
                title: "Autre".to_owned(),
                node_ids: vec![ksampler.clone()],
                bounds: None,
                color: None,
            },
        })
        .is_err());
    graph
        .apply(Command::MoveGroup {
            group: group_id.clone(),
            delta: Point { x: 25.0, y: -5.0 },
        })
        .unwrap();
    for id in [&latent, &ksampler] {
        assert_eq!(
            graph
                .document()
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .unwrap()
                .position,
            Some(Point { x: 25.0, y: -5.0 })
        );
    }
    let moved = graph.serialize().unwrap();
    assert!(graph.undo().unwrap());
    assert!(graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == latent)
        .unwrap()
        .position
        .is_none());
    assert!(graph.redo().unwrap());
    assert_eq!(graph.serialize().unwrap(), moved);
    // Dissolve then reassign; export ignores groups.
    graph
        .apply(Command::UnassignGroup {
            node: latent.clone(),
        })
        .unwrap();
    assert_eq!(
        graph
            .document()
            .groups
            .iter()
            .find(|g| g.id == group_id)
            .unwrap()
            .node_ids
            .len(),
        1
    );
    graph
        .apply(Command::DissolveGroup { group: group_id })
        .unwrap();
    assert!(graph.document().groups.is_empty());
    assert!(graph
        .apply(Command::AssignGroup {
            group: "absent".to_owned(),
            node: latent
        })
        .is_err());
}

#[test]
fn missing_required_inputs_are_reported_and_param_remains_dormant_under_links() {
    let mut graph = EditorGraph::from_executed(&executed_solo(), &schema(), "SOLO").unwrap();
    let ksampler = node_id(&graph, "KSampler");
    let connection = graph
        .document()
        .connections
        .iter()
        .find(|c| c.to.node == ksampler && c.to.input == "model")
        .unwrap()
        .id
        .clone();
    graph.apply(Command::Disconnect { connection }).unwrap();
    assert!(graph
        .missing_required_inputs()
        .contains(&(ksampler.clone(), "model".to_owned())));
    // Parameter under a connected input: dormant, the link stays the executed
    // form; once disconnected, the parameter takes effect again.
    graph
        .apply(Command::Connect {
            connection: Connection {
                id: graph.new_id(),
                from: LinkEnd {
                    node: node_id(&graph, "CheckpointLoaderSimple"),
                    output: 0,
                },
                to: LinkEndInput {
                    node: ksampler.clone(),
                    input: "model".to_owned(),
                },
            },
        })
        .unwrap();
    graph
        .apply(Command::SetParam {
            node: ksampler.clone(),
            input: "model".to_owned(),
            value: json!("dormant"),
        })
        .unwrap();
    assert!(graph.missing_required_inputs().is_empty());
    assert_eq!(graph.to_executed().unwrap(), expected_executed(&graph));
    graph
        .apply(Command::SetParam {
            node: ksampler.clone(),
            input: "model".to_owned(),
            value: Value::Null,
        })
        .unwrap();
    assert!(!graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == ksampler)
        .unwrap()
        .params
        .contains_key("model"));
}

#[test]
fn import_rejects_ui_graphs_unknown_classes_and_incomplete_executed_graphs() {
    // UI export refused, as for the existing engine.
    let ui = json!({"nodes": [], "links": [], "version": 0.4});
    assert!(EditorGraph::from_executed(&ui, &schema(), "UI").is_err());
    // Class missing from the provided schema.
    let unknown_class = json!({"x": {"class_type": "Inconnu", "inputs": {}}});
    let error = EditorGraph::from_executed(&unknown_class, &schema(), "x")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Classe absente"), "{error}");
    // Required input missing from the executed graph.
    let incomplete = json!({"pos": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["c", 1]}},
        "c": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "m"}}});
    let error = EditorGraph::from_executed(&incomplete, &schema(), "x")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Entrée requise absente"), "{error}");
    // Malformed link.
    let malformed = json!({"pos": {"class_type": "CLIPTextEncode", "inputs": {"text": "a", "clip": ["c"]}},
        "c": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "m"}}});
    let error = EditorGraph::from_executed(&malformed, &schema(), "x")
        .unwrap_err()
        .to_string();
    assert!(error.contains("Lien invalide"), "{error}");
    // Nom de document invalide.
    assert!(EditorGraph::from_executed(&executed_solo(), &schema(), " ").is_err());
    assert!(EditorGraph::new("").is_err());
    // Empty executed graph refused on export.
    assert!(EditorGraph::new("vide").unwrap().to_executed().is_err());
}

#[test]
fn undo_after_removing_neighbor_of_frozen_member_restores_exactly() {
    let mut graph = EditorGraph::from_executed(&executed_solo(), &schema(), "SOLO").unwrap();
    let member = node_id(&graph, "CLIPTextEncode");
    let neighbor = graph
        .document()
        .connections
        .iter()
        .find(|c| c.to.node == member)
        .map(|c| c.from.node.clone())
        .unwrap();
    let container_id = graph.new_id();
    graph
        .apply(Command::AddSubgraph {
            container: SubgraphContainer {
                id: container_id,
                title: "Membre".to_owned(),
                node_ids: vec![member.clone()],
                inputs: vec![],
                outputs: vec![],
            },
        })
        .unwrap();
    let before = serde_json::from_str::<Value>(&graph.serialize().unwrap()).unwrap();
    graph
        .apply(Command::RemoveNode {
            node: neighbor.clone(),
        })
        .unwrap();
    assert!(graph.undo().unwrap());
    let after = serde_json::from_str::<Value>(&graph.serialize().unwrap()).unwrap();
    // Content restored exactly (array order may differ:
    // AddNode reinserts at the end — general behaviour of RemoveNode undo).
    let canon = |doc: &Value| {
        let mut doc = doc.clone();
        let nodes = doc["nodes"].as_array().unwrap().clone();
        let mut nodes: Vec<Value> = nodes.into_iter().collect();
        nodes.sort_by_key(|n| n["id"].as_str().unwrap().to_owned());
        doc["nodes"] = Value::Array(nodes);
        let conns = doc["connections"].as_array().unwrap().clone();
        let mut conns: Vec<Value> = conns.into_iter().collect();
        conns.sort_by_key(|c| c["id"].as_str().unwrap().to_owned());
        doc["connections"] = Value::Array(conns);
        doc
    };
    assert_eq!(canon(&before), canon(&after));
    // The link to the frozen member is back (G1 regression).
    assert!(graph
        .document()
        .connections
        .iter()
        .any(|c| c.from.node == neighbor && c.to.node == member));
}

#[test]
fn node_and_group_names_colors_are_reversible_and_persisted() {
    let mut graph = EditorGraph::from_executed(&executed_solo(), &schema(), "SOLO").unwrap();
    let ksampler = node_id(&graph, "KSampler");
    graph
        .apply(Command::SetNodeStyle {
            node: ksampler.clone(),
            title: "Sampler principal".into(),
            color: Some("#112233".into()),
        })
        .unwrap();
    let styled = graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == ksampler)
        .unwrap()
        .clone();
    assert_eq!(styled.title, "Sampler principal");
    assert_eq!(styled.color.as_deref(), Some("#112233"));
    assert!(graph
        .apply(Command::SetNodeStyle {
            node: ksampler.clone(),
            title: "x".into(),
            color: Some("red".into()),
        })
        .is_err());
    graph.undo().unwrap();
    let restored = graph
        .document()
        .nodes
        .iter()
        .find(|n| n.id == ksampler)
        .unwrap()
        .clone();
    assert!(restored.title.is_empty() && restored.color.is_none());
    graph.redo().unwrap();
    assert_eq!(
        graph
            .document()
            .nodes
            .iter()
            .find(|n| n.id == ksampler)
            .unwrap()
            .title,
        "Sampler principal"
    );
    let group = EditorGroup {
        id: graph.new_id(),
        title: "Groupe".into(),
        node_ids: vec![ksampler.clone()],
        bounds: None,
        color: None,
    };
    let group_id = group.id.clone();
    graph.apply(Command::AddGroup { group }).unwrap();
    graph
        .apply(Command::SetGroupStyle {
            group: group_id.clone(),
            title: "Groupe coloré".into(),
            color: Some("#abcdef".into()),
        })
        .unwrap();
    let document = graph.document();
    let saved = document
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .unwrap()
        .clone();
    assert_eq!(saved.title, "Groupe coloré");
    assert_eq!(saved.color.as_deref(), Some("#abcdef"));
    let text = serde_json::to_string(&document).unwrap();
    let reloaded: EditorDocument = serde_json::from_str(&text).unwrap();
    let restored_group = reloaded.groups.iter().find(|g| g.id == group_id).unwrap();
    assert_eq!(restored_group.color.as_deref(), Some("#abcdef"));
    assert_eq!(restored_group.title, "Groupe coloré");
}
