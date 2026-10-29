use crate::{Database, Result};
use rusqlite::params;
use serde_json::{json, Value};
impl Database {
    fn writable_character(&self, id: &str) -> Result<()> {
        if !self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM characters WHERE id=? AND archived=0)",
            [id],
            |r| r.get::<_, bool>(0),
        )? {
            return Err("Personnage introuvable ou archivé".into());
        }
        Ok(())
    }
    pub fn references(&self, character: &str) -> Result<Vec<Value>> {
        let mut query=self.connection.prepare("SELECT r.asset_id,nullif(r.variant_id,''),coalesce(v.name,r.variant_name),r.position,r.is_primary,a.availability,CASE WHEN r.variant_id!='' AND v.id IS NULL THEN 1 ELSE 0 END FROM character_references r JOIN assets a ON a.id=r.asset_id LEFT JOIN character_variants v ON v.id=r.variant_id AND v.character_id=r.character_id WHERE r.character_id=? ORDER BY r.variant_id,r.position")?;
        let rows=query.query_map([character],|r|Ok(json!({"assetId":r.get::<_,String>(0)?,"variantId":r.get::<_,Option<String>>(1)?,"variantName":r.get::<_,Option<String>>(2)?,"position":r.get::<_,i64>(3)?,"primary":r.get::<_,bool>(4)?,"availability":r.get::<_,String>(5)?,"historicalVariant":r.get::<_,bool>(6)?})))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    pub fn add_reference(&self, character: &str, variant: Option<&str>, asset: &str) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        self.writable_character(character)?;
        let before = self.reference_state(character)?;
        let name: Option<String> = variant
            .map(|id| {
                tx.query_row(
                    "SELECT name FROM character_variants WHERE id=?1 AND character_id=?2",
                    params![id, character],
                    |r| r.get(0),
                )
            })
            .transpose()?;
        let scope = variant.unwrap_or("");
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM character_references WHERE character_id=?1 AND variant_id=?2",
            params![character, scope],
            |r| r.get(0),
        )?;
        if count >= 1000 {
            return Err("Maximum 1000 références par portée".into());
        }
        tx.execute("INSERT INTO character_references(character_id,variant_id,variant_name,asset_id,position) SELECT ?1,?2,?3,?4,coalesce(max(position)+1,0) FROM character_references WHERE character_id=?1 AND variant_id=?2 ON CONFLICT(character_id,variant_id,asset_id) DO NOTHING",params![character,scope,name,asset])?;
        tx.execute("INSERT INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,'reference','human') ON CONFLICT DO NOTHING",params![asset,character])?;
        self.record_reference_change(character, "add", before)?;
        tx.commit()?;
        Ok(())
    }
    pub fn remove_reference(
        &self,
        character: &str,
        variant: Option<&str>,
        asset: &str,
    ) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        self.writable_character(character)?;
        let before = self.reference_state(character)?;
        tx.execute("DELETE FROM character_references WHERE character_id=?1 AND variant_id=?2 AND asset_id=?3",params![character,variant.unwrap_or(""),asset])?;
        tx.execute("DELETE FROM asset_characters WHERE character_id=?1 AND asset_id=?2 AND role='reference' AND NOT EXISTS(SELECT 1 FROM character_references WHERE character_id=?1 AND asset_id=?2)",params![character,asset])?;
        self.record_reference_change(character, "remove", before)?;
        tx.commit()?;
        Ok(())
    }
    pub fn primary_reference(
        &self,
        character: &str,
        variant: Option<&str>,
        asset: &str,
    ) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        self.writable_character(character)?;
        let before = self.reference_state(character)?;
        let scope = variant.unwrap_or("");
        tx.execute(
            "UPDATE character_references SET is_primary=0 WHERE character_id=?1 AND variant_id=?2",
            params![character, scope],
        )?;
        if tx.execute("UPDATE character_references SET is_primary=1 WHERE character_id=?1 AND variant_id=?2 AND asset_id=?3",params![character,scope,asset])?!=1 {return Err("Référence introuvable dans cette portée".into());}
        self.record_reference_change(character, "primary", before)?;
        tx.commit()?;
        Ok(())
    }
    pub fn choose_portrait(&self, character: &str, asset: &str) -> Result<()> {
        if self.asset_file_status(asset)?["state"] != "present" {
            return Err("Le fichier choisi n’est pas disponible".into());
        }
        let tx = self.connection.unchecked_transaction()?;
        self.writable_character(character)?;
        if !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM asset_characters WHERE character_id=?1 AND asset_id=?2)",
            params![character, asset],
            |r| r.get::<_, bool>(0),
        )? {
            return Err("Choisissez une image déjà associée au personnage".into());
        }
        let before = self.reference_state(character)?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM character_references WHERE character_id=? AND variant_id=''",
            [character],
            |r| r.get(0),
        )?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM character_references WHERE character_id=?1 AND variant_id='' AND asset_id=?2)",params![character,asset],|r|r.get(0))?;
        if count >= 1000 && !exists {
            return Err("Maximum 1000 références par portée".into());
        }
        tx.execute("INSERT INTO character_references(character_id,variant_id,asset_id,position) SELECT ?1,'',?2,coalesce(max(position)+1,0) FROM character_references WHERE character_id=?1 AND variant_id='' ON CONFLICT DO NOTHING",params![character,asset])?;
        tx.execute("INSERT INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,'reference','human') ON CONFLICT DO NOTHING",params![asset,character])?;
        tx.execute("UPDATE character_references SET is_primary=(asset_id=?2) WHERE character_id=?1 AND variant_id=''",params![character,asset])?;
        self.record_reference_change(character, "primary", before)?;
        tx.commit()?;
        Ok(())
    }
    pub fn reorder_references(
        &self,
        character: &str,
        variant: Option<&str>,
        assets: &[String],
    ) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        self.writable_character(character)?;
        let before = self.reference_state(character)?;
        if assets.len() > 1000 {
            return Err("Maximum 1000 références par portée".into());
        }
        let refs = self
            .references(character)?
            .into_iter()
            .filter(|r| r["variantId"].as_str() == variant)
            .collect::<Vec<_>>();
        let actual = refs
            .iter()
            .filter_map(|r| r["assetId"].as_str())
            .collect::<std::collections::HashSet<_>>();
        let requested = assets
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        if requested.len() != assets.len() || actual != requested {
            return Err(
                "L'ordre doit contenir chaque référence de la portée exactement une fois".into(),
            );
        }
        let scope = variant.unwrap_or("");
        tx.execute(
            "DELETE FROM character_references WHERE character_id=?1 AND variant_id=?2",
            params![character, scope],
        )?;
        for (index, asset) in assets.iter().enumerate() {
            let old = refs
                .iter()
                .find(|r| r["assetId"].as_str() == Some(asset))
                .ok_or("Référence absente")?;
            tx.execute("INSERT INTO character_references(character_id,variant_id,variant_name,asset_id,position,is_primary) VALUES(?1,?2,?3,?4,?5,?6)",params![character,scope,old["variantName"].as_str(),asset,index as i64,old["primary"].as_bool().unwrap_or(false)])?;
        }
        self.record_reference_change(character, "order", before)?;
        tx.commit()?;
        Ok(())
    }
}

pub(crate) fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableReference {
    asset_key: String,
    hash: String,
    variant_id: Option<String>,
    variant_name: Option<String>,
    historical_variant: bool,
    position: u32,
    primary: bool,
}

impl Database {
    pub(crate) fn export_references(&self, character: &str) -> Result<Value> {
        let mut references = Vec::new();
        for r in self.references(character)? {
            let asset = r["assetId"].as_str().ok_or("Référence invalide")?;
            let hash: Option<String> = self.connection.query_row(
                "SELECT content_sha256 FROM assets WHERE id=?",
                [asset],
                |r| r.get(0),
            )?;
            let hash = hash
                .filter(|h| valid_hash(h))
                .ok_or("Référence sans hash : indexez son image avant l'export")?;
            references.push(PortableReference {
                asset_key: asset.into(),
                hash,
                variant_id: r["variantId"].as_str().map(str::to_string),
                variant_name: r["variantName"].as_str().map(str::to_string),
                historical_variant: r["historicalVariant"].as_bool().ok_or("Portée invalide")?,
                position: r["position"]
                    .as_u64()
                    .and_then(|p| p.try_into().ok())
                    .ok_or("Position invalide")?,
                primary: r["primary"].as_bool().ok_or("Rôle invalide")?,
            });
        }
        Ok(serde_json::to_value(references)?)
    }

    // The caller owns the transaction, including creation of the new character and variants.
    pub(crate) fn import_references(
        &self,
        character: &str,
        value: &Value,
        active_variants: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        use std::collections::{HashMap, HashSet};
        let references: Vec<PortableReference> = serde_json::from_value(value.clone())?;
        if references.len() > 10_000 {
            return Err("Maximum 10000 références par fiche".into());
        }
        let mut variants = active_variants.clone();
        let mut historical_names = HashMap::new();
        let mut assets: HashMap<String, (String, String)> = HashMap::new();
        let mut used_assets = HashSet::new();
        let mut positions = HashSet::new();
        let mut scope_assets = HashSet::new();
        let mut primary_scopes = HashSet::new();
        let mut counts = HashMap::new();
        for r in references {
            if r.asset_key.is_empty() || r.asset_key.len() > 128 || !valid_hash(&r.hash) {
                return Err("Clé ou hash de référence invalide".into());
            }
            let scope = match r.variant_id.as_deref() {
                None => {
                    if r.historical_variant || r.variant_name.is_some() {
                        return Err("Métadonnées de portée principale incohérentes".into());
                    }
                    String::new()
                }
                Some(old) => {
                    if old.is_empty() || old.len() > 128 {
                        return Err("Portée de variante invalide".into());
                    }
                    let name = r
                        .variant_name
                        .as_deref()
                        .filter(|n| !n.trim().is_empty() && n.len() <= 300)
                        .ok_or("Nom de variante absent")?;
                    if r.historical_variant {
                        if active_variants.contains_key(old) {
                            return Err("Une variante active ne peut être historique".into());
                        }
                        if historical_names
                            .insert(old.to_string(), name.to_string())
                            .is_some_and(|previous| previous != name)
                        {
                            return Err("Noms historiques contradictoires".into());
                        }
                        variants
                            .entry(old.to_string())
                            .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                            .clone()
                    } else {
                        let new = active_variants
                            .get(old)
                            .ok_or("Référence attribuée à une variante absente")?;
                        let actual: String = self.connection.query_row(
                            "SELECT name FROM character_variants WHERE id=?",
                            [new],
                            |r| r.get(0),
                        )?;
                        if name != actual {
                            return Err("Nom de variante incohérent".into());
                        }
                        new.clone()
                    }
                }
            };
            let count = counts.entry(scope.clone()).or_insert(0);
            *count += 1;
            if *count > 1000
                || !positions.insert((scope.clone(), r.position))
                || !scope_assets.insert((scope.clone(), r.asset_key.clone()))
                || (r.primary && !primary_scopes.insert(scope.clone()))
            {
                return Err("Références dupliquées, ordre ou principale incompatibles".into());
            }
            let hash = r.hash.to_ascii_lowercase();
            let asset = if let Some((previous_hash, asset)) = assets.get(&r.asset_key) {
                if previous_hash != &hash {
                    return Err("Une clé d'image désigne plusieurs hashes".into());
                }
                asset.clone()
            } else {
                let mut q = self
                    .connection
                    .prepare("SELECT id FROM assets WHERE content_sha256=? ORDER BY id")?;
                let candidates = q
                    .query_map([&hash], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let asset=candidates.into_iter().find(|a|!used_assets.contains(a)).ok_or_else(||format!("Référence {hash} absente ou copies indexées insuffisantes ; indexez les images avant l'import"))?;
                used_assets.insert(asset.clone());
                assets.insert(r.asset_key, (hash, asset.clone()));
                asset
            };
            self.connection.execute("INSERT INTO character_references(character_id,variant_id,variant_name,asset_id,position,is_primary) VALUES(?1,?2,?3,?4,?5,?6)", params![character,scope,r.variant_name,asset,r.position,r.primary])?;
            self.connection.execute("INSERT INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,'reference','human') ON CONFLICT DO NOTHING",params![asset,character])?;
        }
        Ok(())
    }
}

impl Database {
    pub(crate) fn reference_state(&self, character: &str) -> Result<Vec<Value>> {
        Ok(self
            .references(character)?
            .into_iter()
            .map(|mut r| {
                if let Some(object) = r.as_object_mut() {
                    object.remove("availability");
                    object.remove("historicalVariant");
                }
                r
            })
            .collect())
    }
    pub(crate) fn record_reference_change(
        &self,
        character: &str,
        operation: &str,
        before: Vec<Value>,
    ) -> Result<()> {
        use std::collections::BTreeMap;
        let keyed = |rows: Vec<Value>| {
            rows.into_iter()
                .map(|r| {
                    (
                        (
                            r["variantId"].as_str().unwrap_or("").to_string(),
                            r["assetId"].as_str().unwrap_or("").to_string(),
                        ),
                        r,
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        let before = keyed(before);
        let after = keyed(self.reference_state(character)?);
        if before == after {
            return Ok(());
        }
        let changed_before = before
            .iter()
            .filter(|(key, value)| after.get(*key) != Some(*value))
            .map(|(_, r)| r)
            .collect::<Vec<_>>();
        let changed_after = after
            .iter()
            .filter(|(key, value)| before.get(*key) != Some(*value))
            .map(|(_, r)| r)
            .collect::<Vec<_>>();
        self.connection.execute("INSERT INTO character_reference_history(character_id,operation,changes_json) VALUES(?1,?2,?3)",params![character,operation,json!({"before":changed_before,"after":changed_after}).to_string()])?;
        Ok(())
    }
    pub fn reference_history(&self, character: &str, before: Option<i64>) -> Result<Vec<Value>> {
        let mut query=self.connection.prepare("SELECT id,operation,changes_json,saved_at FROM character_reference_history WHERE character_id=?1 AND (?2 IS NULL OR id<?2) ORDER BY id DESC LIMIT 20")?;
        let rows = query
            .query_map(params![character, before], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|(id,operation,changes,saved)|Ok(json!({"id":id,"operation":operation,"changes":serde_json::from_str::<Value>(&changes)?,"savedAt":saved}))).collect()
    }
}
