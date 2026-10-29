use crate::{Database, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub id: String,
    pub name: String,
    pub identity: Value,
    pub image_ids: Vec<String>,
    pub archived: bool,
    pub revision: u32,
}

impl Database {
    pub fn characters(&self) -> Result<Vec<Character>> {
        self.all_characters(false)
    }
    pub fn all_characters(&self, include_archived: bool) -> Result<Vec<Character>> {
        let mut query=self.connection.prepare("SELECT id,name,identity_json,archived,revision FROM characters WHERE archived=0 OR ? ORDER BY name COLLATE NOCASE,id")?;
        let rows = query.query_map([include_archived], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, u32>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, name, text, archived, revision) = row?;
            let mut refs=self.connection.prepare("SELECT DISTINCT asset_id FROM asset_characters WHERE character_id=? ORDER BY asset_id")?;
            let image_ids = refs
                .query_map([&id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            result.push(Character {
                id,
                name,
                identity: serde_json::from_str(&text)?,
                image_ids,
                archived,
                revision,
            });
        }
        Ok(result)
    }
    pub fn save_character(
        &self,
        id: Option<String>,
        name: String,
        identity: Value,
    ) -> Result<String> {
        if name.trim().is_empty()
            || name.len() > 300
            || !identity.is_object()
            || identity.to_string().len() > 200_000
        {
            return Err("Nom ou caractéristiques invalides".into());
        }
        let id = match id {
            Some(id) => {
                let changed=self.connection.execute("UPDATE characters SET name=?2,identity_json=?3,revision=revision+1 WHERE id=?1 AND archived=0",params![id,name.trim(),identity.to_string()])?;
                if changed != 1 {
                    return Err("Personnage introuvable".into());
                }
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.connection.execute(
                    "INSERT INTO characters(id,name,identity_json) VALUES(?1,?2,?3)",
                    params![id, name.trim(), identity.to_string()],
                )?;
                id
            }
        };
        Ok(id)
    }
    pub fn delete_character(&self, id: &str) -> Result<()> {
        self.set_character_archived(id, true)
    }
    pub fn set_character_archived(&self, id: &str, archived: bool) -> Result<()> {
        if self.connection.execute(
            "UPDATE characters SET archived=?2,revision=revision+1 WHERE id=?1",
            params![id, archived],
        )? != 1
        {
            return Err("Personnage introuvable".into());
        }
        Ok(())
    }
    pub fn associate(&self, character: &str, asset: &str, add: bool) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let before = self.reference_state(character)?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM characters WHERE id=? AND archived=0)",
            [character],
            |r| r.get(0),
        )?;
        if !active {
            return Err("Personnage introuvable".into());
        }
        if add {
            self.connection.execute("INSERT INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,'reference','human') ON CONFLICT DO NOTHING",params![asset,character])?;
        } else {
            self.connection.execute(
                "DELETE FROM asset_characters WHERE asset_id=?1 AND character_id=?2",
                params![asset, character],
            )?;
        }
        self.record_reference_change(character, if add { "associate" } else { "unlink" }, before)?;
        tx.commit()?;
        Ok(())
    }
    pub fn preferences(&self) -> Result<Value> {
        let mut result = serde_json::Map::new();
        let mut q = self
            .connection
            .prepare("SELECT key,value_json FROM preferences")?;
        for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (k, v) = row?;
            result.insert(k, serde_json::from_str(&v)?);
        }
        Ok(Value::Object(result))
    }
    pub fn set_preference(&self, key: &str, value: Value) -> Result<()> {
        if ![
            "galleryFilters",
            "selectedTab",
            "language",
            "language_setup_completed",
            "fileNamingLanguage",
            "libraryRoot",
            "captureRoot",
            "generationDraft",
            "generationWorkspace",
            "castingWorkspace",
            "generationPreparations",
            "selectedSayaRun",
            "characterEditor",
            "gallerySelection",
            "generationRoot",
            "loraRoot",
            "loraFavorites",
            "loraStacks",
            "loraActiveStack",
            "generationLoraSlots",
            "generationLoraInbox",
            "modelFolderPaths",
            "modelOrganizeJournal",
            "promptEngineRoot",
            "galleryView",
            "presetBase",
            "comfyOutputDir",
        ]
        .contains(&key)
            || value.to_string().len() > 1_000_000
        {
            return Err("Préférence invalide".into());
        }
        const LANGUAGES: [&str; 14] = ["fr","en","ja","ko","zh-CN","zh-TW","pt-BR","es","ru","tr","ar","vi","th","id"];
        if key == "language" && !value.as_str().is_some_and(|code| LANGUAGES.contains(&code)) {
            return Err("Langue inconnue".into());
        }
        // Empty string = follow the interface language.
        if key == "fileNamingLanguage" && !value.as_str().is_some_and(|code| code.is_empty() || LANGUAGES.contains(&code)) {
            return Err("Langue inconnue".into());
        }
        if key == "language_setup_completed" && !value.is_boolean() {
            return Err("Confirmation de langue invalide".into());
        }
        if key == "captureRoot" {
            // Delivery creates and later deletes files under this folder: never a system path.
            let text = value.as_str().ok_or("Dossier de captures invalide")?;
            if !text.trim().is_empty() {
                crate::library_sources::checked_capture_root(text)?;
            }
        }
        if key == "galleryView" && !matches!(value.as_str(), Some("stream" | "pages")) {
            return Err("Vue de galerie inconnue".into());
        }
        // Empty string = automatic detection; otherwise an absolute, existing, readable folder.
        if key == "comfyOutputDir" {
            let text = value.as_str().ok_or("Dossier output de ComfyUI invalide")?;
            if !text.is_empty() && !crate::generation_browser::usable_output_dir(text) {
                return Err("Dossier output de ComfyUI invalide : chemin absolu d'un dossier existant et lisible requis".into());
            }
            crate::generation_browser::set_preferred_output_dir(text);
        }
        self.connection.execute("INSERT INTO preferences VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![key,value.to_string()])?;
        self.mirror_settings(key);
        Ok(())
    }
    pub(crate) fn import_character(
        &self,
        source: &str,
        source_hash: &str,
        key: &str,
        record: &Value,
    ) -> Result<String> {
        let old:Option<String>=self.connection.query_row("SELECT character_id FROM import_records WHERE source_path=?1 AND source_key=?2 AND character_id IS NOT NULL ORDER BY imported_at LIMIT 1",params![source,key],|r|r.get(0)).optional()?;
        if let Some(id) = old {
            return Ok(id);
        }
        let name = record
            .get("name")
            .or(record.get("label"))
            .and_then(Value::as_str)
            .unwrap_or(key);
        let mut identity = record.clone();
        if !identity.is_object() {
            identity = json!({});
        }
        identity["sourceId"] = json!(key);
        let id = self.save_character(None, name.into(), identity)?;
        self.connection.execute("INSERT INTO import_records(id,source_path,source_sha256,source_key,character_id) VALUES(?1,?2,?3,?4,?5)",params![uuid::Uuid::new_v4().to_string(),source,source_hash,key,id])?;
        Ok(id)
    }
}

pub(crate) fn validate_variant(name: &str, blocks: &Value, constraints: &Value) -> Result<()> {
    if name.trim().is_empty()
        || name.len() > 300
        || !constraints.is_object()
        || constraints.to_string().len() > 200_000
    {
        return Err("Variante invalide".into());
    }
    let blocks = blocks
        .as_object()
        .ok_or("Les sections de prompt doivent être un objet")?;
    if blocks.iter().any(|(k, v)| {
        !["QUALITY", "P1", "P2", "ACTION", "BG", "NEG"].contains(&k.as_str())
            || !v.is_string()
            || v.as_str().is_some_and(|s| s.len() > 200_000)
    }) {
        return Err(
            "Sections autorisées : QUALITY, P1, P2, ACTION, BG, NEG ; valeurs texte uniquement"
                .into(),
        );
    }
    Ok(())
}
impl Database {
    pub fn variants(&self, character: &str) -> Result<Vec<Value>> {
        let mut q=self.connection.prepare("SELECT id,name,prompt_blocks_json,constraints_json,revision FROM character_variants WHERE character_id=? ORDER BY name,id")?;
        let rows = q
            .query_map([character], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, u32>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|(id,name,blocks,constraints,revision)| Ok(json!({"id":id,"name":name,"promptBlocks":serde_json::from_str::<Value>(&blocks)?,"constraints":serde_json::from_str::<Value>(&constraints)?,"revision":revision}))).collect()
    }
    pub fn save_variant(
        &self,
        character: &str,
        id: Option<String>,
        name: &str,
        blocks: Value,
        constraints: Value,
    ) -> Result<String> {
        self.save_variant_at_revision(character, id, name, blocks, constraints, None)
    }
    pub(crate) fn save_variant_at_revision(
        &self,
        character: &str,
        id: Option<String>,
        name: &str,
        blocks: Value,
        constraints: Value,
        expected_revision: Option<u32>,
    ) -> Result<String> {
        validate_variant(name, &blocks, &constraints)?;
        let tx = self.connection.unchecked_transaction()?;
        let id = if let Some(id) = id {
            if tx.execute("UPDATE character_variants SET name=?3,prompt_blocks_json=?4,constraints_json=?5,revision=revision+1 WHERE id=?1 AND character_id=?2",params![id,character,name,blocks.to_string(),constraints.to_string()])?!=1 { return Err("Variante introuvable".into()); }
            id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute("INSERT INTO character_variants(id,character_id,name,prompt_blocks_json,constraints_json) VALUES(?1,?2,?3,?4,?5)",params![id,character,name,blocks.to_string(),constraints.to_string()])?;
            id
        };
        if tx.execute(
            "UPDATE characters SET revision=revision+1 WHERE id=?1 AND archived=0 AND (?2 IS NULL OR revision=?2)",
            params![character,expected_revision],
        )? != 1
        {
            return Err("Personnage modifié, introuvable ou archivé ; préparez à nouveau la variante".into());
        }
        tx.commit()?;
        Ok(id)
    }
    pub fn character_history(&self, character: &str) -> Result<Vec<Value>> {
        let mut q=self.connection.prepare("SELECT revision,name,identity_json,archived,variants_json,saved_at FROM character_history WHERE character_id=? ORDER BY revision DESC")?;
        let rows = q
            .query_map([character], |r| {
                Ok((
                    r.get::<_, u32>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|(revision,name,identity,archived,variants,saved_at)| Ok(json!({"revision":revision,"name":name,"identity":serde_json::from_str::<Value>(&identity)?,"archived":archived,"variants":serde_json::from_str::<Value>(&variants)?,"savedAt":saved_at}))).collect()
    }
    pub fn restore_character(&self, character: &str, revision: u32) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let (name,identity,variants):(String,String,String)=tx.query_row("SELECT name,identity_json,variants_json FROM character_history WHERE character_id=?1 AND revision=?2",params![character,revision],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let variants: Vec<Value> = serde_json::from_str(&variants)?;
        tx.execute(
            "DELETE FROM character_variants WHERE character_id=?",
            [character],
        )?;
        for v in variants {
            tx.execute("INSERT INTO character_variants(id,character_id,name,prompt_blocks_json,constraints_json,revision) VALUES(?1,?2,?3,?4,?5,?6)",params![v["id"].as_str(),character,v["name"].as_str(),v["promptBlocks"].to_string(),v["constraints"].to_string(),v["revision"].as_i64()])?;
        }
        tx.execute(
            "UPDATE characters SET name=?2,identity_json=?3,revision=revision+1 WHERE id=?1",
            params![character, name, identity],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn export_character(&self, id: &str) -> Result<Value> {
        let (name, identity): (String, String) = self.connection.query_row(
            "SELECT name,identity_json FROM characters WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let value = json!({"format":"saya-character","version":2,"name":name,"identity":serde_json::from_str::<Value>(&identity)?,"variants":self.variants(id)?,"references":self.export_references(id)?});
        if value.to_string().len() > 2_000_000
            || value["variants"].as_array().is_some_and(|v| v.len() > 100)
            || value["references"]
                .as_array()
                .is_some_and(|r| r.len() > 10_000)
        {
            return Err("Fiche trop volumineuse pour le format portable (2 Mo, 100 variantes, 10000 références)".into());
        }
        Ok(value)
    }
    pub fn import_character_document(&self, text: &str) -> Result<String> {
        if text.len() > 2_000_000 {
            return Err("Fiche supérieure à 2 Mo".into());
        }
        let value: Value = serde_json::from_str(text)?;
        if value["format"] != "saya-character" || !matches!(value["version"].as_u64(), Some(1 | 2))
        {
            return Err("Format de fiche incompatible".into());
        }
        let name = value["name"].as_str().ok_or("Nom absent")?;
        let variants = value["variants"].as_array().ok_or("Variantes absentes")?;
        if variants.len() > 100 {
            return Err("Maximum 100 variantes par fiche".into());
        }
        for v in variants {
            validate_variant(
                v["name"].as_str().ok_or("Nom de variante absent")?,
                &v["promptBlocks"],
                &v["constraints"],
            )?;
        }
        let legacy_hashes = if value["version"] == 1 {
            let hashes = value["referenceHashes"]
                .as_array()
                .ok_or("Références absentes")?;
            if hashes.len() > 10_000
                || hashes
                    .iter()
                    .any(|v| !v.as_str().is_some_and(crate::references::valid_hash))
            {
                return Err("Hashes de référence invalides".into());
            }
            Some(hashes)
        } else {
            None
        };
        let tx = self.connection.unchecked_transaction()?;
        let id = self.save_character(None, name.into(), value["identity"].clone())?;
        let mut variant_ids = std::collections::HashMap::new();
        for v in variants {
            let new_id = uuid::Uuid::new_v4().to_string();
            if value["version"] == 2 {
                let old_id = v["id"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                    .ok_or("Identifiant de variante invalide")?;
                if variant_ids
                    .insert(old_id.to_string(), new_id.clone())
                    .is_some()
                {
                    return Err("Identifiant de variante dupliqué".into());
                }
            }
            tx.execute("INSERT INTO character_variants(id,character_id,name,prompt_blocks_json,constraints_json) VALUES(?1,?2,?3,?4,?5)",params![new_id,id,v["name"].as_str(),v["promptBlocks"].to_string(),v["constraints"].to_string()])?;
        }
        if let Some(hashes) = legacy_hashes {
            for hash in hashes {
                tx.execute("INSERT OR IGNORE INTO asset_characters(asset_id,character_id,role,evidence) SELECT id,?1,'reference','human' FROM assets WHERE content_sha256=?2",params![id,hash.as_str().map(str::to_ascii_lowercase)])?;
            }
        } else {
            self.import_references(&id, &value["references"], &variant_ids)?;
        }
        tx.execute(
            "UPDATE characters SET revision=revision+1 WHERE id=?",
            [&id],
        )?;
        self.record_reference_change(&id, "import", Vec::new())?;
        tx.commit()?;
        Ok(id)
    }
}

/// Local rule-based suggestions. Never applies a suggestion or rewrites existing tags.
pub fn suggest_character(constraints: &Value, seed: &str) -> Result<Value> {
    crate::contracts::Seed::parse(seed)?;
    let object = constraints
        .as_object()
        .ok_or("Contraintes attendues sous forme d'objet")?;
    if object
        .keys()
        .any(|k| !["world", "role", "species", "palette"].contains(&k.as_str()))
    {
        return Err("Contrainte inconnue ; aucune contrainte ne sera ignorée".into());
    }
    let get = |key: &str, allowed: &[&str]| -> Result<String> {
        let value = constraints[key]
            .as_str()
            .ok_or_else(|| format!("Contrainte absente : {key}"))?;
        if !allowed.contains(&value) {
            return Err(format!("Contrainte invalide : {key}").into());
        }
        Ok(value.into())
    };
    let world = get("world", &["fantasy", "modern", "science-fiction"])?;
    let role = get("role", &["scholar", "guardian", "explorer"])?;
    let species = get("species", &["any", "human", "elf", "android"])?;
    let palette = get("palette", &["any", "silver", "dark", "warm"])?;
    let permitted = match world.as_str() {
        "fantasy" => vec!["human", "elf"],
        "modern" => vec!["human"],
        _ => vec!["human", "android"],
    };
    let candidates: Vec<_> = permitted
        .into_iter()
        .filter(|s| species == "any" || *s == species)
        .collect();
    if candidates.is_empty() {
        return Err("Espèce incompatible avec cet univers ; ajuster une contrainte".into());
    }
    let hash = crate::gallery::digest(format!("{constraints}:{seed}").as_bytes());
    let choice = u64::from_str_radix(&hash[..16], 16)?;
    let species = candidates[(choice % candidates.len() as u64) as usize];
    let wardrobe = wardrobe(&world, &role);
    let palette = if palette == "any" {
        ["silver", "dark", "warm"][((choice / 7) % 3) as usize]
    } else {
        &palette
    };
    let appearance = match palette {
        "silver" => "silver hair, blue eyes",
        "dark" => "black hair, brown eyes",
        _ => "auburn hair, green eyes",
    };
    let species_tags = match species {
        "elf" => "elf, pointy ears",
        "android" => "android, mechanical joints",
        _ => "human",
    };
    let names = ["Ari", "Mira", "Ren", "Sora", "Noa", "Lior"];
    Ok(
        json!({"name":names[((choice/13)%names.len() as u64) as usize],"identity":{"identity_prompt":format!("adult, solo, {species_tags}, {appearance}, {wardrobe}"),"description":format!("{role} / {world} / {species}"),"loras":[],"generator":{"version":1,"seed":seed,"constraints":constraints}},"constraints":constraints,"explanation":format!("Univers {world} : espèce {species} compatible, tenue définie par le rôle {role}, palette {palette}. Aucune LoRA imposée.")}),
    )
}

pub(crate) fn wardrobe(world: &str, role: &str) -> &'static str {
    match (world, role) {
        ("fantasy", "scholar") => "robe, embroidered trim, satchel",
        ("fantasy", "guardian") => "plate armor, tabard, gauntlets",
        ("fantasy", _) => "travel cloak, leather boots, belt pouch",
        ("modern", "scholar") => "cardigan, collared shirt, glasses",
        ("modern", "guardian") => "protective uniform, utility belt, boots",
        ("modern", _) => "hiking jacket, cargo pants, backpack",
        (_, "scholar") => "lab coat, bodysuit, data tablet",
        (_, "guardian") => "powered armor, reinforced boots, visor",
        _ => "spacesuit, helmet under arm, utility harness",
    }
}
