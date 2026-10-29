//! Explicit IMAGE boundaries; reuses the existing pipeline runner.
use crate::{
    gallery::digest, workflow::validate_graph, workflow_pipeline::WorkflowPipeline, Database,
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Boundary {
    pub node: String,
    pub slot: usize,
}
fn output_type<'a>(graph: &Value, info: &'a Value, b: &Boundary) -> Result<&'a str> {
    let class = graph[&b.node]["class_type"]
        .as_str()
        .ok_or("Node de frontière absent")?;
    info[class]["output"]
        .get(b.slot)
        .and_then(Value::as_str)
        .ok_or_else(|| "Sortie de frontière inconnue".into())
}
fn link(value: &Value) -> Option<Boundary> {
    let a = value.as_array()?;
    if a.len() != 2 {
        return None;
    }
    Some(Boundary {
        node: a[0].as_str()?.into(),
        slot: a[1].as_u64()? as usize,
    })
}
fn collect(
    graph: &Value,
    node: &str,
    prior: &[Boundary],
    included: &mut BTreeSet<String>,
    used: &mut BTreeSet<usize>,
) -> Result<()> {
    if !included.insert(node.into()) {
        return Ok(());
    }
    let inputs = graph[node]["inputs"]
        .as_object()
        .ok_or("Node de dépendance absent")?;
    for value in inputs.values() {
        if let Some(b) = link(value) {
            if let Some(i) = prior.iter().position(|p| p == &b) {
                used.insert(i);
            } else if prior.iter().any(|p| p.node == b.node) {
                return Err(format!(
                    "Frontière {} : sortie {} supplémentaire requise, transport non défini",
                    b.node, b.slot
                )
                .into());
            } else {
                collect(graph, &b.node, prior, included, used)?;
            }
        }
    }
    Ok(())
}
fn fresh(graph: &Value, base: &str) -> String {
    let mut id = base.to_owned();
    while graph.get(&id).is_some() {
        id.push('_');
    }
    id
}
fn bound_schema(info: &Value, graph: &Value, loaders: &[String]) -> Value {
    let mut schema = info.clone();
    if let Some(choices) = schema
        .pointer_mut("/LoadImage/input/required/image/0")
        .and_then(Value::as_array_mut)
    {
        for id in loaders {
            let image = graph[id]["inputs"]["image"].clone();
            if !choices.contains(&image) {
                choices.push(image);
            }
        }
    }
    schema
}
struct Plan {
    graphs: Vec<Value>,
    steps: Vec<Value>,
    included: BTreeSet<String>,
}
fn build(
    graph: &Value,
    info: &Value,
    cuts: &[Boundary],
    output: &str,
    source: &str,
    sha: &str,
) -> Result<Plan> {
    validate_graph(graph, info)?;
    if cuts.len() > 31 {
        return Err("Maximum 32 passes".into());
    }
    if graph[output]["class_type"] != "SaveImage" {
        return Err("Choisir explicitement la sortie finale SaveImage".into());
    }
    let mut seen = BTreeSet::new();
    for b in cuts {
        if !seen.insert((b.node.clone(), b.slot)) {
            return Err("Frontière dupliquée".into());
        }
        let kind = output_type(graph, info, b)?;
        if !matches!(kind, "IMAGE" | "LATENT") {
            return Err(format!("Frontière {}[{}] de type {kind} : transport non pris en charge (IMAGE/LATENT uniquement, aucune conversion)",b.node,b.slot).into());
        }
    }
    let mut graphs = Vec::new();
    let mut steps = Vec::new();
    let mut all = BTreeSet::new();
    for stage in 0..=cuts.len() {
        let target = cuts.get(stage).map_or(output, |b| b.node.as_str());
        let mut included = BTreeSet::new();
        let mut used = BTreeSet::new();
        collect(graph, target, &cuts[..stage], &mut included, &mut used)?;
        if stage > 0 && !used.contains(&(stage - 1)) {
            return Err(format!("Passe {} : la frontière précédente n'alimente pas cette passe ; ordre ou découpe ambigu",stage+1).into());
        }
        if cuts
            .iter()
            .skip(stage + 1)
            .any(|c| included.contains(&c.node))
        {
            return Err("Frontières dans un ordre incompatible avec les dépendances".into());
        }
        // Reconstruct shared loaders/conditioning locally, but never repeat an image/latent-producing branch invisibly.
        for repeated in included.intersection(&all) {
            let class = graph[repeated]["class_type"].as_str().unwrap();
            if info[class]["output"].as_array().is_some_and(|o| {
                o.iter()
                    .any(|t| matches!(t.as_str(), Some("IMAGE" | "LATENT" | "MASK")))
            }) {
                return Err(format!("Dépendance {} ({class}) recalculerait une image/latent déjà produit ; frontière incomplète",repeated).into());
            }
        }
        let mut copy: Map<String, Value> = included
            .iter()
            .map(|id| (id.clone(), graph[id].clone()))
            .collect();
        let mut loaders = BTreeMap::new();
        for previous in &used {
            let id = fresh(graph, &format!("saya_input_{}", previous + 1));
            loaders.insert(*previous, id.clone());
            let kind = output_type(graph, info, &cuts[*previous])?;
            if kind == "LATENT" {
                copy.insert(id, json!({"class_type":"LoadLatent","inputs":{"latent":"saya-pipeline-awaiting-latent.latent"}}));
            } else {
                copy.insert(id,json!({"class_type":"LoadImage","inputs":{"image":"saya-pipeline-awaiting-upload.png"}}));
            }
        }
        for node in copy.values_mut() {
            for value in node["inputs"]
                .as_object_mut()
                .ok_or("Entrées absentes")?
                .values_mut()
            {
                if let Some(b) = link(value) {
                    if let Some(i) = cuts[..stage].iter().position(|p| p == &b) {
                        *value = json!([loaders[&i], 0]);
                    }
                }
            }
        }
        let save = if stage < cuts.len() {
            let id = fresh(graph, "saya_output");
            let kind = output_type(graph, info, &cuts[stage])?;
            if kind == "LATENT" {
                copy.insert(id.clone(),json!({"class_type":"SaveLatent","inputs":{"samples":[cuts[stage].node,cuts[stage].slot],"filename_prefix":format!("saya/pass-{}",stage+1)}}));
            } else {
                copy.insert(id.clone(),json!({"class_type":"SaveImage","inputs":{"images":[cuts[stage].node,cuts[stage].slot],"filename_prefix":format!("saya/pass-{}",stage+1)}}));
            }
            id
        } else {
            output.into()
        };
        if copy
            .values()
            .filter(|n| info[n["class_type"].as_str().unwrap_or("")]["output_node"] == true)
            .count()
            != 1
        {
            return Err("Une seule sortie exécutable par passe est requise ; dépendance à un autre output node refusée".into());
        }
        copy.get_mut(&save).unwrap()["_meta"]["sayaSplit"] = json!({"sourceWorkflow":source,"sourceSha256":sha,"cuts":cuts,"finalOutput":output,"pass":stage+1});
        let copy = Value::Object(copy);
        let loader_ids: Vec<String> = loaders.values().cloned().collect();
        let has_latent = used
            .iter()
            .any(|i| output_type(graph, info, &cuts[*i]).ok() == Some("LATENT"))
            || (stage < cuts.len() && output_type(graph, info, &cuts[stage])? == "LATENT");
        if !has_latent {
            validate_graph(&copy, &bound_schema(info, &copy, &loader_ids))?;
        } else {
            for node in copy.as_object().into_iter().flatten().map(|(_, n)| n) {
                let class = node["class_type"].as_str().unwrap_or_default();
                if info.get(class).is_none() {
                    return Err(format!("Classe {class} absente de object_info").into());
                }
            }
        }
        let bindings: Vec<Value> = used
            .iter()
            .map(|i| {
                let kind = output_type(graph, info, &cuts[*i]).unwrap_or("IMAGE");
                json!({
                    "port": format!("{}_{}", kind.to_ascii_lowercase(), i + 1),
                    "nodeId": loaders[i],
                    "input": if kind == "LATENT" { "latent" } else { "image" },
                    "source": {"step": format!("pass{}", i + 1), "port": kind.to_ascii_lowercase()}
                })
            })
            .collect();
        let inputs: Vec<Value> = used
            .iter()
            .map(|i| {
                let kind = output_type(graph, info, &cuts[*i]).unwrap_or("IMAGE");
                json!({"name": format!("{}_{}", kind.to_ascii_lowercase(), i + 1), "portType": kind})
            })
            .collect();
        let out_kind = if stage < cuts.len() {
            output_type(graph, info, &cuts[stage])?
        } else {
            "IMAGE"
        };
        steps.push(json!({"id":format!("pass{}",stage+1),"workflowId":format!("pending-{}",stage+1),"kind":out_kind.to_ascii_lowercase(),"inputs":inputs,"outputs":[{"name":out_kind.to_ascii_lowercase(),"portType":out_kind}],"bindings":bindings}));
        all.extend(included);
        graphs.push(copy);
    }
    let bytes: usize = graphs.iter().map(|g| g.to_string().len()).sum();
    if bytes > 64 * 1024 * 1024 {
        return Err("Décomposition supérieure à 64 Mo".into());
    }
    Ok(Plan {
        graphs,
        steps,
        included: all,
    })
}
fn reachable(reference: &Value, output: &str) -> BTreeSet<String> {
    fn walk(graph: &Value, node: &str, seen: &mut BTreeSet<String>) {
        if !seen.insert(node.to_owned()) {
            return;
        }
        let Some(node_value) = graph.get(node) else {
            return;
        };
        for value in node_value["inputs"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, v)| v)
        {
            if let Some(link) = link(value) {
                walk(graph, &link.node, seen);
            }
        }
    }
    let mut seen = BTreeSet::new();
    walk(reference, output, &mut seen);
    seen
}

fn fidelity(reference: &Value, graphs: &[Value], cuts: &[Boundary], output: &str) -> Result<Value> {
    let cut_nodes: BTreeSet<String> = cuts.iter().map(|c| c.node.clone()).collect();
    let mut union = BTreeSet::new();
    let mut mutated = Vec::new();
    for (stage, graph) in graphs.iter().enumerate() {
        for (id, node) in graph.as_object().into_iter().flatten() {
            let class = node["class_type"].as_str().unwrap_or_default();
            if id.starts_with("saya_input_") || id == "saya_output" {
                if !matches!(
                    class,
                    "LoadImage" | "SaveImage" | "LoadLatent" | "SaveLatent"
                ) {
                    return Err(format!(
                        "Passe {} : node de frontière {id} de classe inattendue {class}",
                        stage + 1
                    )
                    .into());
                }
                continue;
            }
            union.insert(id.clone());
            let original = reference.get(id).ok_or_else(|| {
                format!(
                    "Passe {} : node {id} absent de l'API de référence",
                    stage + 1
                )
            })?;
            if original["class_type"] != node["class_type"] {
                return Err(format!("Passe {} : classe modifiée pour {id}", stage + 1).into());
            }
            let original_inputs = original["inputs"]
                .as_object()
                .ok_or("Entrées de référence absentes")?;
            let node_inputs = node["inputs"]
                .as_object()
                .ok_or("Entrées de passe absentes")?;
            if original_inputs.len() != node_inputs.len() {
                mutated.push(json!({"pass":stage+1,"node":id,"reason":"nombre d'entrées"}));
                continue;
            }
            for (key, value) in original_inputs {
                let actual = node_inputs
                    .get(key)
                    .ok_or_else(|| format!("Passe {} : entrée {id}.{key} perdue", stage + 1))?;
                if let Some(original_link) = link(value) {
                    let Some(actual_link) = link(actual) else {
                        mutated.push(json!({"pass":stage+1,"node":id,"input":key,"reason":"lien"}));
                        continue;
                    };
                    if original_link == actual_link {
                        continue;
                    }
                    let allowed = actual_link.node.starts_with("saya_input_")
                        && cut_nodes.contains(&original_link.node);
                    if !allowed {
                        mutated.push(json!({"pass":stage+1,"node":id,"input":key,"expected":value,"actual":actual}));
                    }
                } else if value != actual {
                    mutated.push(json!({"pass":stage+1,"node":id,"input":key,"expected":value,"actual":actual}));
                }
            }
            let meta = |n: &Value| n.pointer("/_meta").cloned().unwrap_or(Value::Null);
            let mut expected_meta = meta(original);
            let mut actual_meta = meta(node);
            if let Some(map) = expected_meta.as_object_mut() {
                map.remove("sayaSplit");
            }
            if let Some(map) = actual_meta.as_object_mut() {
                map.remove("sayaSplit");
            }
            if expected_meta.as_object().is_some_and(|m| m.is_empty()) {
                expected_meta = Value::Null;
            }
            if actual_meta.as_object().is_some_and(|m| m.is_empty()) {
                actual_meta = Value::Null;
            }
            if expected_meta != actual_meta {
                mutated.push(json!({"pass":stage+1,"node":id,"reason":"métadonnées"}));
            }
            if let Some(extra) = node_inputs
                .keys()
                .find(|k| !original_inputs.contains_key(*k))
            {
                mutated.push(
                    json!({"pass":stage+1,"node":id,"input":extra,"reason":"entrée ajoutée"}),
                );
            }
        }
    }
    let missing: Vec<String> = reachable(reference, output)
        .into_iter()
        .filter(|id| !union.contains(id))
        .collect();
    if !missing.is_empty() || !mutated.is_empty() {
        return Err(format!(
            "Découpage non fidèle : {} node(s) manquant(s) {:?}, {} mutation(s) : {}",
            missing.len(),
            missing,
            mutated.len(),
            serde_json::to_string(&mutated).unwrap_or_default()
        )
        .into());
    }
    if !union.contains(output) {
        return Err("Découpage non fidèle : sortie finale absente".into());
    }
    Ok(
        json!({"nodes": union.len(), "missing": missing, "mutated": mutated, "outputsPreserved": true}),
    )
}

impl Database {
    /// Convert a UI workflow with the validated graphToPrompt converter, then
    /// split the full API reference. Nothing is written and the UI source is
    /// never modified; the complete reference API is returned for inspection.
    pub fn split_ui_preview(
        &self,
        source: &str,
        cuts: &[Boundary],
        output: &str,
        info: &Value,
    ) -> Result<Value> {
        let converted = crate::workflow_ui_api::convert_ui(source, Some(info))?;
        if converted["report"]["complete"] != true {
            return Err(format!(
                "Conversion UI→API incomplète : {} entrée(s) non résolue(s)",
                converted["report"]["unresolved"]
                    .as_array()
                    .map_or(0, Vec::len)
            )
            .into());
        }
        let api = converted["api"].clone();
        validate_graph(&api, info)?;
        let sha = digest(api.to_string().as_bytes());
        let plan = build(&api, info, cuts, output, "ui", &sha)?;
        let audit = fidelity(&api, &plan.graphs, cuts, output)?;
        let passes: Vec<Value> = plan
            .graphs
            .iter()
            .enumerate()
            .map(|(i, g)| json!({"step":plan.steps[i],"apiJson":g.to_string(),"nodeCount":g.as_object().unwrap().len()}))
            .collect();
        Ok(json!({
            "referenceApi": api,
            "apiSha256": sha,
            "passes": passes,
            "cuts": cuts,
            "output": output,
            "fidelity": audit,
            "report": converted["report"],
        }))
    }

    pub fn split_analysis(&self, workflow: &str, info: &Value) -> Result<Value> {
        let (_, text) = self.workflow_document(workflow)?;
        let graph: Value = serde_json::from_str(&text)?;
        validate_graph(&graph, info)?;
        let nodes:Vec<Value>=graph.as_object().unwrap().iter().map(|(id,n)|{
            let class=n["class_type"].as_str().unwrap();
            let ports:Vec<Value>=info[class]["output"].as_array().into_iter().flatten().enumerate().map(|(slot,t)|json!({"node":id,"slot":slot,"type":t,"supported":t=="IMAGE"||t=="LATENT"})).collect();
            let category=if class.contains("Sampler"){"sampler"}else if class.contains("Detailer"){"detailer"}else if class.contains("Scale")||class.contains("Upscale"){"upscale"}else if class.contains("VAE"){"vae"}else if class.contains("Loader"){"loader"}else{"node"};
            json!({"id":id,"class":class,"category":category,"outputs":ports,"finalOutput":class=="SaveImage","parameters":n["inputs"].to_string()})
        }).collect();
        Ok(json!({"workflow":workflow,"sourceSha256":digest(text.as_bytes()),"nodes":nodes}))
    }
    pub fn split_preview(
        &self,
        workflow: &str,
        cuts: &[Boundary],
        output: &str,
        info: &Value,
    ) -> Result<Value> {
        let (_, text) = self.workflow_document(workflow)?;
        let sha = digest(text.as_bytes());
        let graph: Value = serde_json::from_str(&text)?;
        let plan = build(&graph, info, cuts, output, workflow, &sha)?;
        let fidelity = fidelity(&graph, &plan.graphs, cuts, output)?;
        let passes:Vec<Value>=plan.graphs.iter().enumerate().map(|(i,g)|json!({"step":plan.steps[i],"apiJson":g.to_string(),"nodeCount":g.as_object().unwrap().len()})).collect();
        let excluded: Vec<&String> = graph
            .as_object()
            .unwrap()
            .keys()
            .filter(|id| !plan.included.contains(*id))
            .collect();
        let preview = json!({"workflow":workflow,"sourceSha256":sha,"cuts":cuts,"output":output,"passes":passes,"excludedNodes":excluded,"fidelity":fidelity,"transport":"IMAGE via PNG ou LATENT via fichier SaveLatent puis LoadLatent ; reprise logique, sans état interne du sampler. MODEL/CLIP/VAE/conditionings reconstruits dans chaque passe depuis leurs dépendances."});
        Ok(json!({"previewHash":digest(preview.to_string().as_bytes()),"plan":preview}))
    }
    #[allow(clippy::too_many_arguments)] // Explicit source, boundaries and confirmation snapshot are all required.
    pub fn split_save(
        &self,
        workflow: &str,
        cuts: &[Boundary],
        output: &str,
        name: &str,
        hash: &str,
        confirmed: bool,
        info: &Value,
    ) -> Result<Value> {
        if !confirmed {
            return Err("Confirmer la décomposition affichée".into());
        }
        let preview = self.split_preview(workflow, cuts, output, info)?;
        if preview["previewHash"] != hash {
            return Err("Décomposition modifiée : prévisualiser à nouveau".into());
        }
        let tx = self.connection.unchecked_transaction()?;
        let passes = preview["plan"]["passes"].as_array().unwrap();
        let mut steps = Vec::new();
        let mut workflows = Vec::new();
        for (i, p) in passes.iter().enumerate() {
            let text = p["apiJson"].as_str().unwrap();
            let id = self.import_workflow(&format!("{name} · {}", i + 1), text)?;
            workflows.push(json!({"id":id,"sourceSha256":digest(text.as_bytes())}));
            let mut step = p["step"].clone();
            step["workflowId"] = json!(id);
            steps.push(step);
        }
        let manifest = json!({"format":"saya-workflow-pipeline","version":2,"id":uuid::Uuid::new_v4().to_string(),"name":name,"description":format!("Source {} ; SHA-256 {}",workflow,preview["plan"]["sourceSha256"].as_str().unwrap()),"workflows":workflows,"steps":steps});
        let parsed = WorkflowPipeline::parse(&manifest.to_string())?;
        self.preflight_pipeline(&parsed, Some(info))?;
        let id = self.save_pipeline(&manifest.to_string())?;
        tx.commit()?;
        Ok(json!({"pipeline":id,"manifestJson":manifest.to_string(),"workflows":workflows}))
    }
}
/// Only declared IMAGE bindings may receive a future upload; validate all other values unchanged.
pub(crate) fn validate_with_uploads(
    graph: &Value,
    info: &Value,
    step: &crate::workflow_pipeline::Step,
) -> Result<()> {
    let loaders: Vec<String> = step.bindings.iter().map(|b| b.node_id.clone()).collect();
    validate_graph(graph, &bound_schema(info, graph, &loaders))
}
