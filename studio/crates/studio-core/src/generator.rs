use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratorInput {
    pub seed: String,
    #[serde(default)]
    pub required: BTreeMap<String, String>,
    #[serde(default)]
    pub forbidden: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub locked: BTreeMap<String, String>,
    #[serde(default)]
    pub checkpoint_family: String,
}

#[derive(Serialize)]
struct Attribute {
    id: &'static str,
    label: &'static str,
    permanent: bool,
    values: &'static [&'static str],
}
const ATTRIBUTES: &[Attribute] = &[
    Attribute {
        id: "name",
        label: "Nom",
        permanent: true,
        values: &["Ari", "Mira", "Ren", "Sora", "Noa", "Lior"],
    },
    Attribute {
        id: "world",
        label: "Univers",
        permanent: false,
        values: &["fantasy", "modern", "science-fiction"],
    },
    Attribute {
        id: "role",
        label: "Rôle",
        permanent: false,
        values: &["scholar", "guardian", "explorer"],
    },
    Attribute {
        id: "species",
        label: "Espèce",
        permanent: true,
        values: &["human", "elf", "android"],
    },
    Attribute {
        id: "hairColor",
        label: "Cheveux",
        permanent: true,
        values: &["black", "silver", "auburn", "blonde"],
    },
    Attribute {
        id: "eyeColor",
        label: "Yeux",
        permanent: true,
        values: &["blue", "brown", "green", "amber"],
    },
    Attribute {
        id: "hairStyle",
        label: "Coiffure",
        permanent: false,
        values: &["short", "long", "braid"],
    },
];
pub fn catalog() -> Value {
    json!({"version":2,"attributes":ATTRIBUTES,"rules":["fantasy: human ou elf ; modern: human ; science-fiction: human ou android","Tenue définie par univers et rôle","Une seule couleur de cheveux, couleur des yeux et coiffure ; braid implique long hair"]})
}
fn compatible(world: &str, species: &str) -> bool {
    species == "human"
        || (world == "fantasy" && species == "elf")
        || (world == "science-fiction" && species == "android")
}
fn choose<'a>(seed: &str, attribute: &str, candidates: &[&'a str]) -> Result<&'a str> {
    if candidates.is_empty() {
        return Err(format!("Contraintes impossibles pour {attribute} ; retirez une obligation, une interdiction ou un verrou").into());
    }
    let hash = crate::gallery::digest(format!("saya-generator-v2:{seed}:{attribute}").as_bytes());
    Ok(candidates[(u64::from_str_radix(&hash[..16], 16)? % candidates.len() as u64) as usize])
}
pub fn generate(input: &GeneratorInput) -> Result<Value> {
    crate::contracts::Seed::parse(&input.seed)?;
    if input.checkpoint_family.len() > 200 {
        return Err("Famille checkpoint trop longue".into());
    }
    for (key, values) in input
        .required
        .iter()
        .map(|(k, v)| (k, vec![v]))
        .chain(input.locked.iter().map(|(k, v)| (k, vec![v])))
        .chain(input.forbidden.iter().map(|(k, v)| (k, v.iter().collect())))
    {
        let field = ATTRIBUTES
            .iter()
            .find(|a| a.id == key)
            .ok_or_else(|| format!("Attribut inconnu : {key}"))?;
        if values.len() > field.values.len()
            || values.iter().any(|v| !field.values.contains(&v.as_str()))
        {
            return Err(format!("Valeur de contrainte invalide pour {key}").into());
        }
    }
    let mut domains = BTreeMap::new();
    for field in ATTRIBUTES {
        let candidates = field
            .values
            .iter()
            .copied()
            .filter(|value| {
                input.required.get(field.id).is_none_or(|v| v == value)
                    && input.locked.get(field.id).is_none_or(|v| v == value)
                    && input
                        .forbidden
                        .get(field.id)
                        .is_none_or(|v| !v.iter().any(|v| v == value))
            })
            .collect::<Vec<_>>();
        choose(&input.seed, field.id, &candidates)?;
        domains.insert(field.id, candidates);
    }
    // Prune the joint domain before choosing: a locked elf must not fail because a random world was modern.
    let worlds = domains["world"]
        .iter()
        .copied()
        .filter(|w| domains["species"].iter().any(|s| compatible(w, s)))
        .collect::<Vec<_>>();
    if worlds.is_empty() {
        return Err(
            "Univers et espèce incompatibles ; modifiez leur obligation, interdiction ou verrou"
                .into(),
        );
    }
    let world = choose(&input.seed, "world", &worlds)?;
    domains.insert("world", worlds);
    domains
        .get_mut("species")
        .ok_or("Catalogue incomplet")?
        .retain(|s| compatible(world, s));
    let mut attributes = BTreeMap::new();
    let mut trace = Vec::new();
    for field in ATTRIBUTES {
        let selected = if field.id == "world" {
            world
        } else {
            choose(&input.seed, field.id, &domains[field.id])?
        };
        attributes.insert(field.id, selected);
        let reason = if input.locked.contains_key(field.id) {
            "locked"
        } else if input.required.contains_key(field.id) {
            "required"
        } else {
            "seeded"
        };
        trace.push(json!({"attribute":field.id,"selected":selected,"reason":reason,"candidates":domains[field.id],"forbidden":input.forbidden.get(field.id)}));
    }
    let species = match attributes["species"] {
        "elf" => "elf, pointy ears",
        "android" => "android, mechanical joints",
        _ => "human",
    };
    let hairstyle = match attributes["hairStyle"] {
        "short" => "short hair",
        "long" => "long hair",
        _ => "long hair, braid",
    };
    let wardrobe = crate::characters::wardrobe(world, attributes["role"]);
    let prompt = format!(
        "adult, solo, {species}, {} hair, {} eyes, {hairstyle}, {wardrobe}",
        attributes["hairColor"], attributes["eyeColor"]
    );
    let prompts = json!({"QUALITY":"","P1":prompt,"P2":"","ACTION":"","BG":"","NEG":""});
    let description = format!(
        "{} / {world} / {}",
        attributes["role"], attributes["species"]
    );
    let generator = json!({"version":2,"seed":input.seed,"checkpointFamily":input.checkpoint_family,"attributes":attributes,"required":input.required,"forbidden":input.forbidden,"locked":input.locked,"trace":trace,"prompts":prompts});
    Ok(
        json!({"seed":input.seed,"name":attributes["name"],"attributes":attributes,"trace":trace,"prompts":prompts,"identity":{"identity_prompt":prompt,"description":description,"loras":[],"generator":generator},"explanation":format!("{description} : espèce compatible avec l'univers, tenue déterminée par le rôle, coiffure sans contradiction. Famille checkpoint déclarative ; compatibilité du workflow à vérifier séparément.")}),
    )
}

impl crate::Database {
    pub fn prepare_generated_variant(
        &self,
        character: &str,
        input: &GeneratorInput,
    ) -> Result<Value> {
        let (revision, text): (u32, String) = self.connection.query_row(
            "SELECT revision,identity_json FROM characters WHERE id=? AND archived=0",
            [character],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let identity: Value = serde_json::from_str(&text)?;
        let source = &identity["generator"];
        if source["version"] != 2 {
            return Err("Variante automatique disponible uniquement pour une identité du générateur v2 ; utilisez l'éditeur manuel".into());
        }
        let original: GeneratorInput = serde_json::from_value(
            json!({"seed":source["seed"],"checkpointFamily":source["checkpointFamily"],"required":source["required"],"forbidden":source["forbidden"],"locked":source["locked"]}),
        )?;
        let replay = generate(&original)?;
        if replay["identity"]["generator"] != *source
            || replay["prompts"]["P1"] != identity["identity_prompt"]
        {
            return Err("Identité modifiée manuellement : utilisez l'éditeur de variantes pour préserver vos tags exacts".into());
        }
        let mut constrained = input.clone();
        for field in ATTRIBUTES.iter().filter(|a| a.permanent) {
            let value = source["attributes"][field.id]
                .as_str()
                .ok_or("Identité générée incomplète")?;
            if constrained.locked.get(field.id).is_some_and(|v| v != value) {
                return Err(
                    format!("Le trait permanent {} doit rester {value}", field.label).into(),
                );
            }
            constrained.locked.insert(field.id.into(), value.into());
        }
        let result = generate(&constrained)?;
        Ok(
            json!({"characterId":character,"revision":revision,"basePrompt":identity["identity_prompt"],"result":result}),
        )
    }
    pub fn save_generated_variant(
        &self,
        character: &str,
        name: &str,
        input: &GeneratorInput,
        revision: u32,
        confirmed: bool,
    ) -> Result<String> {
        if !confirmed {
            return Err("Confirmation explicite requise pour créer la variante".into());
        }
        let prepared = self.prepare_generated_variant(character, input)?;
        if prepared["revision"].as_u64() != Some(revision as u64) {
            return Err(
                "Fiche modifiée depuis la préparation ; préparez à nouveau la variante".into(),
            );
        }
        self.save_variant_at_revision(character,None,name,prepared["result"]["prompts"].clone(),json!({"generator":prepared["result"]["identity"]["generator"],"baseCharacterId":character,"baseRevision":revision,"basePrompt":prepared["basePrompt"]}),Some(revision))
    }
}
