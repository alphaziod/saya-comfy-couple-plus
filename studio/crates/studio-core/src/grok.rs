//! Explicit data import from observed Grok JSON exports. Never executes source code.
use crate::{gallery::digest, Database, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub key: String,
    pub name: String,
    pub source_id: String,
    pub identity: Value,
    pub blocks: Value,
    pub variants: Vec<Value>,
    pub warnings: Vec<String>,
    pub duplicate: bool,
    pub suggested: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Choice {
    pub path: String,
    pub sha256: String,
    pub key: String,
    pub mode: String,
    pub character: Option<String>,
}
fn text<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| v.get(*key).and_then(Value::as_str))
}

/// Anatomy words: a tag containing one belongs to the ANATOMY prompt, never to P1.
const ANATOMY_WORDS: [&str; 16] = [
    "penis",
    "testicle",
    "testicule",
    "scrotum",
    "glans",
    "pubic hair",
    "erection",
    "cute_penis",
    "bulge",
    "pussy",
    "clitoris",
    "labia",
    "vagina",
    "vulva",
    "cleft of venus",
    "precum",
];

/// `first` kept byte for byte, followed by `, second` when there is one.
pub(crate) fn join_prompt(first: &str, second: &str) -> String {
    let second = second.trim().trim_matches(',').trim();
    if second.is_empty() {
        return first.to_owned();
    }
    let head = first.trim_end().trim_end_matches(',');
    if head.trim().is_empty() {
        second.to_owned()
    } else {
        format!("{head}, {second}")
    }
}

/// Splits an identity prompt into (P1 without anatomy, completed anatomy). The anatomy tags
/// already present in `anatomy` are not repeated; the original order is kept. Without any anatomy tag
/// in the prompt, both texts are returned as is (byte for byte).
pub fn split_anatomy(prompt: &str, anatomy: &str) -> (String, String) {
    let mut identity = Vec::new();
    let mut moved = Vec::new();
    for tag in prompt.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let lower = tag.to_lowercase();
        if ANATOMY_WORDS.iter().any(|word| lower.contains(word)) {
            moved.push(tag.to_string());
        } else {
            identity.push(tag.to_string());
        }
    }
    if moved.is_empty() {
        // Nothing to move: texts kept identical (spaces, line breaks).
        return (prompt.to_string(), anatomy.to_string());
    }
    let mut anatomy_tags: Vec<String> = anatomy
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    for tag in moved {
        if !anatomy_tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
            anatomy_tags.push(tag);
        }
    }
    (identity.join(", "), anatomy_tags.join(", "))
}

pub fn parse_characters(path: &Path) -> Result<(Vec<Entry>, Vec<String>)> {
    let path = path.canonicalize()?;
    if !path.is_file() || path.extension().and_then(|x| x.to_str()) != Some("json") {
        return Err("Importer un export JSON Grok ; les scripts ne sont jamais exécutés".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Fichier supérieur à 16 Mio".into());
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let sha = digest(&bytes);
    let (format, rows) = if let Some(rows) = value["characters"].as_array() {
        ("characters", rows.clone())
    } else if let Some(rows) = value["valides"].as_array() {
        ("valides", rows.clone())
    } else if let Some(rows) = value["persos"].as_array() {
        ("persos", rows.clone())
    } else if let Some(rows) = value.as_array() {
        ("liste", rows.clone())
    } else {
        ("fiche", vec![value])
    };
    if rows.len() > 10000 {
        return Err("Maximum 10000 définitions par fichier".into());
    }
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut keys = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let Some(prompt) = text(row, &["identity_prompt", "p1"]) else {
            errors.push(format!(
                "{} · entrée {} : identity_prompt/p1 absent ; élément ignoré",
                path.display(),
                index + 1
            ));
            continue;
        };
        let Some(name) = text(row, &["name", "nom", "label"]) else {
            errors.push(format!(
                "Entrée {} : nom absent ; élément ignoré",
                index + 1
            ));
            continue;
        };
        let source_id = text(row, &["id", "slug", "bank_id"])
            .map(str::to_owned)
            .unwrap_or_else(|| format!("ligne-{}", index + 1));
        let key = format!("{format}:{source_id}");
        if !keys.insert(key.clone()) {
            return Err(format!(
                "Identifiant source dupliqué : {key} ; aucun regroupement automatique"
            )
            .into());
        }
        let anatomy_source = text(row, &["anatomy_prompt", "anatomy", "anatomie"]).unwrap_or("");
        // P1 = identity only: the anatomy tags left in the identity prompt (GROK bank)
        // move into the character's anatomy.
        let (prompt_p1, anatomy_split) = split_anatomy(prompt, anatomy_source);
        let prompt = prompt_p1.as_str();
        let anatomy = anatomy_split.as_str();
        let mut identity = row.clone();
        identity["identity_prompt"] = json!(prompt);
        identity["anatomy_prompt"] = json!(anatomy);
        identity["sourceId"] = json!(source_id);
        identity["grokSource"] = json!({"path":path,"sha256":sha,"key":key,"raw":row});
        // The generation P1 always carries the anatomy (penis, testicles, pubic hair).
        let mut blocks = json!({"P1":join_prompt(prompt, anatomy)});
        // A character is P1 (+ anatomy) and its backgrounds as variants, nothing else:
        // pose (action), quality and negative are chosen per generation and stay in
        // grokSource only.
        if let Some(v) = text(row, &["scene_bg", "bg"]) {
            blocks["BG"] = json!(v);
        }
        let mut variants = Vec::new();
        if let Some(options) = row["bg_options"].as_array() {
            for (i, bg) in options.iter().enumerate() {
                if let Some(content) = bg["scene_bg"].as_str() {
                    let mut b = blocks.clone();
                    b["BG"] = json!(content);
                    variants.push(json!({"name":text(bg,&["bg_name","bg_id"]).map(str::to_owned).unwrap_or_else(||format!("Décor {}",i+1)),"blocks":b,"source":bg}));
                }
            }
        }
        if let Some(options) = row["decors"].as_array() {
            for (i, bg) in options.iter().enumerate() {
                if let Some(content) = bg.as_str() {
                    let mut b = blocks.clone();
                    b["BG"] = json!(content);
                    variants.push(json!({"name":format!("Décor {}",i+1),"blocks":b,"source":bg}));
                }
            }
        }
        let mut warnings=vec!["Anatomie et paramètres source conservés dans la fiche/contraintes ; aucun mapping de workflow inventé.".into()];
        if text(row, &["image_source", "image"]).is_some() {
            warnings.push("Chemin de référence conservé en provenance ; association des images via la galerie après indexation.".into());
        }
        if row["bank_id"].is_string() {
            warnings.push(
                "bank_id conservé ; aucune fusion automatique entre variantes ou noms identiques."
                    .into(),
            );
        }
        if name.trim().is_empty() || name.len() > 300 || identity.to_string().len() > 200000 {
            errors.push(format!("{key} : fiche vide ou trop volumineuse"));
            continue;
        }
        entries.push(Entry {
            path: path.to_string_lossy().into(),
            sha256: sha.clone(),
            key,
            name: name.into(),
            source_id,
            identity,
            blocks,
            variants,
            warnings,
            duplicate: false,
            suggested: None,
        });
    }
    Ok((entries, errors))
}

impl Database {
    pub fn grok_preview(&self, paths: &[String]) -> Result<Value> {
        if paths.is_empty() || paths.len() > 64 {
            return Err("Choisissez de 1 à 64 fichiers".into());
        }
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut seen = BTreeSet::new();
        let mut bytes = 0;
        for path in paths {
            let path = Path::new(path);
            bytes += path.metadata()?.len();
            if bytes > 64 * 1024 * 1024 {
                return Err("Lot supérieur à 64 Mio".into());
            }
            match parse_characters(path) {
                Ok((rows, notes)) => {
                    warnings.extend(notes);
                    for mut e in rows {
                        if !seen.insert((e.path.clone(), e.key.clone())) {
                            continue;
                        }
                        e.duplicate=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM import_records WHERE source_path=?1 AND source_sha256=?2 AND source_key=?3)",params![e.path,e.sha256,e.key],|r|r.get(0))?;
                        e.suggested=self.connection.query_row("SELECT character_id FROM import_records WHERE source_path=?1 AND (source_key=?2 OR source_key=?3) AND character_id IS NOT NULL ORDER BY imported_at DESC LIMIT 1",params![e.path,e.key,e.source_id],|r|r.get(0)).optional()?;
                        entries.push(e);
                    }
                }
                Err(e) => warnings.push(format!("{} : {e}", path.display())),
            }
        }
        Ok(json!({"entries":entries,"warnings":warnings}))
    }
    pub fn grok_import(&self, choices: &[Choice], confirmed: bool) -> Result<Value> {
        if !confirmed {
            return Err("Confirmez la prévisualisation avant import".into());
        }
        if choices.is_empty() || choices.len() > 10000 {
            return Err("Sélection d’import invalide".into());
        }
        let unique: BTreeSet<_> = choices.iter().map(|c| &c.path).collect();
        if unique.len() > 64 {
            return Err("Maximum 64 fichiers par import".into());
        }
        let bytes = unique.iter().try_fold(0u64, |total, p| {
            std::fs::metadata(p).map(|s| total.saturating_add(s.len()))
        })?;
        if bytes > 64 * 1024 * 1024 {
            return Err("Lot supérieur à 64 Mio".into());
        }
        let mut files = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for choice in choices {
            if !seen.insert((&choice.path, &choice.key)) {
                return Err("Sélection dupliquée".into());
            }
            if !files.contains_key(&choice.path) {
                files.insert(
                    choice.path.clone(),
                    parse_characters(Path::new(&choice.path))?.0,
                );
            }
        }
        let tx = self.connection.unchecked_transaction()?;
        let mut imported = Vec::new();
        let mut skipped = 0;
        for c in choices {
            let entry = files[&c.path]
                .iter()
                .find(|e| e.key == c.key && e.sha256 == c.sha256)
                .ok_or("Source modifiée depuis l’aperçu ; recommencer l’analyse")?;
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM import_records WHERE source_path=?1 AND source_sha256=?2 AND source_key=?3)",params![entry.path,entry.sha256,entry.key],|r|r.get::<_,bool>(0))?{skipped+=1;continue;}
            let character = match c.mode.as_str() {
                "create" => {
                    self.save_character(None, entry.name.clone(), entry.identity.clone())?
                }
                "variant" => c.character.clone().ok_or("Personnage cible requis")?,
                _ => return Err("Action d’import inconnue".into()),
            };
            if !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM characters WHERE id=? AND archived=0)",
                [&character],
                |r| r.get::<_, bool>(0),
            )? {
                return Err("Personnage cible absent ou archivé".into());
            }
            let variants = std::iter::once(
                json!({"name":entry.name,"blocks":entry.blocks,"source":entry.identity}),
            )
            .chain(entry.variants.clone());
            for variant in variants {
                let name = variant["name"].as_str().ok_or("Nom de variante absent")?;
                let constraints = json!({"grokSource":{"path":entry.path,"sha256":entry.sha256,"key":entry.key,"raw":variant["source"]},"anatomy_prompt":entry.identity["anatomy_prompt"]});
                crate::characters::validate_variant(name, &variant["blocks"], &constraints)?;
                tx.execute("INSERT INTO character_variants(id,character_id,name,prompt_blocks_json,constraints_json) VALUES(?1,?2,?3,?4,?5)",params![uuid::Uuid::new_v4().to_string(),character,name,variant["blocks"].to_string(),constraints.to_string()])?;
            }
            tx.execute(
                "UPDATE characters SET revision=revision+1 WHERE id=?",
                [&character],
            )?;
            tx.execute("INSERT INTO import_records(id,source_path,source_sha256,source_key,character_id) VALUES(?1,?2,?3,?4,?5)",params![uuid::Uuid::new_v4().to_string(),entry.path,entry.sha256,entry.key,character])?;
            imported.push(json!({"key":entry.key,"characterId":character}));
        }
        tx.commit()?;
        Ok(json!({"imported":imported,"skipped":skipped}))
    }
}

#[cfg(test)]
mod split_anatomy_tests {
    use super::split_anatomy;

    #[test]
    fn moves_anatomy_tags_out_of_p1() {
        let (p1, anatomy) = split_anatomy(
            "1girl, solo, futanari, long hair, blue eyes, nude, small penis, erect penis, scrotum, small testicles, pubic hair",
            "",
        );
        assert_eq!(p1, "1girl, solo, futanari, long hair, blue eyes, nude");
        assert_eq!(
            anatomy,
            "small penis, erect penis, scrotum, small testicles, pubic hair"
        );
    }

    #[test]
    fn untouched_when_no_anatomy_in_prompt() {
        let (p1, anatomy) = split_anatomy("  exact, (tag:1.2)\nété\n", " anatomy  ");
        assert_eq!(p1, "  exact, (tag:1.2)\nété\n");
        assert_eq!(anatomy, " anatomy  ");
    }

    #[test]
    fn keeps_existing_anatomy_without_duplicates() {
        let (p1, anatomy) = split_anatomy(
            "1futanari, (red hair:1.2), penis visible",
            "small penis, penis visible",
        );
        assert_eq!(p1, "1futanari, (red hair:1.2)");
        assert_eq!(anatomy, "small penis, penis visible");
    }
}
