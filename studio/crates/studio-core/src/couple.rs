use crate::{workflow::Edit, Database, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoupleInput {
    #[serde(default)]
    pub characters: Vec<crate::workflow::JobCharacter>,
    pub p1: String,
    #[serde(default)]
    pub p2: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub bg: String,
    pub quality: Option<String>,
    pub negative: Option<String>,
    pub mask: Option<SplitEdit>,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitEdit {
    pub node_id: String,
    pub direction: String,
    pub split: u32,
    pub blur: f64,
    pub swap: bool,
}
impl Database {
    pub fn couple_inspection(&self, id: &str) -> Result<Value> {
        let text: String = self.connection.query_row(
            "SELECT document_json FROM workflow_revisions WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        let graph: Value = serde_json::from_str(&text)?;
        let fields = self.workflow(id)?.fields;
        let has_p1 = fields
            .iter()
            .any(|f| f.enabled && f.section.as_deref() == Some("P1"));
        let has_p2 = fields
            .iter()
            .any(|f| f.enabled && f.section.as_deref() == Some("P2"));
        let profile = match (has_p1, has_p2) {
            (true, true) => "couple",
            (true, false) => "solo",
            _ => "unconfigured",
        };
        let mut masks = Vec::new();
        let mut links = Vec::new();
        let mut warnings = Vec::new();
        if !has_p1 {
            warnings.push(
                "Mapping P1 absent : sélectionner une entrée texte distincte dans Workflows."
                    .into(),
            );
        }
        for (node_id, node) in graph.as_object().ok_or("Graphe invalide")? {
            if node["class_type"] == "SayaSplitMask" {
                let enabled = ["direction", "split", "blur", "swap"].iter().all(|key| {
                    fields
                        .iter()
                        .any(|f| f.node_id == *node_id && f.input_name == *key && f.enabled)
                });
                masks.push(json!({"nodeId":node_id,"inputs":node["inputs"],"enabled":enabled}));
                if !enabled {
                    warnings.push(format!(
                        "Masque {node_id} : activer ses quatre entrées primitives pour l'adapter."
                    ));
                }
            }
            for (key, value) in node["inputs"].as_object().into_iter().flatten() {
                if let Some(link) = value.as_array() {
                    let resolved = link.len() == 2
                        && link[1].is_u64()
                        && link[0]
                            .as_str()
                            .is_some_and(|source| graph.get(source).is_some());
                    links.push(
                        json!({"nodeId":node_id,"input":key,"source":value,"resolved":resolved}),
                    );
                    if !resolved {
                        warnings.push(format!("Entrée liée non résolue : {node_id}.{key}"));
                    }
                }
            }
        }
        Ok(
            json!({"roles":fields.iter().filter(|f|f.enabled && f.section.is_some()).collect::<Vec<_>>(),
            "promptProfile":profile,"masks":masks,"links":links,"warnings":warnings,
            "scope":"Graphe API uniquement. Les types et slots sont validés avec le moteur lors de la préparation. Aucun subgraph UI n'est converti."}),
        )
    }

    pub fn couple_edits(&self, id: &str, input: &CoupleInput) -> Result<Vec<Edit>> {
        let fields = self.workflow(id)?.fields;
        let has_p2 = fields
            .iter()
            .any(|f| f.enabled && f.section.as_deref() == Some("P2"));
        if input.p1.trim().is_empty() || (has_p2 && input.p2.trim().is_empty()) {
            return Err("Chaque identité mappée exige un prompt non vide".into());
        }
        if !has_p2 && !input.p2.is_empty() {
            return Err("P2 fourni sans mapping P2 ; aucun changement implicite de mode".into());
        }
        let mut edits = Vec::new();
        for (role, value) in [
            ("P1", Some(&input.p1)),
            ("P2", has_p2.then_some(&input.p2)),
            ("ACTION", Some(&input.action)),
            ("BG", Some(&input.bg)),
            ("QUALITY", input.quality.as_ref()),
            ("NEG", input.negative.as_ref()),
        ] {
            if let Some(value) = value {
                if matches!(role, "ACTION" | "BG")
                    && value.is_empty()
                    && !fields
                        .iter()
                        .any(|f| f.enabled && f.section.as_deref() == Some(role))
                {
                    continue;
                }
                if value.len() > 200_000 {
                    return Err("Champ trop long".into());
                }
                let f = fields
                    .iter()
                    .find(|f| {
                        f.enabled && f.value_type == "string" && f.section.as_deref() == Some(role)
                    })
                    .ok_or_else(|| format!("Mapping texte {role} absent"))?;
                edits.push(Edit {
                    section: Some(role.into()),
                    node_id: f.node_id.clone(),
                    input_name: f.input_name.clone(),
                    value: value.clone(),
                });
            }
        }
        if let Some(mask) = &input.mask {
            if !["vertical", "horizontal"].contains(&mask.direction.as_str())
                || mask.split > 100
                || !mask.blur.is_finite()
                || mask.blur < 0.0
            {
                return Err("Géométrie SayaSplitMask invalide".into());
            }
            for (key, value) in [
                ("direction", mask.direction.clone()),
                ("split", mask.split.to_string()),
                ("blur", mask.blur.to_string()),
                ("swap", mask.swap.to_string()),
            ] {
                let f = fields
                    .iter()
                    .find(|f| {
                        f.node_id == mask.node_id
                            && f.input_name == key
                            && f.class_type == "SayaSplitMask"
                            && f.enabled
                            && f.section.is_none()
                    })
                    .ok_or("Entrée SayaSplitMask absente, liée ou non autorisée")?;
                edits.push(Edit {
                    section: None,
                    node_id: f.node_id.clone(),
                    input_name: key.into(),
                    value,
                });
            }
        }
        Ok(edits)
    }
}
