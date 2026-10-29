//! Character gallery: read-only cards and sheets built from existing tables.
//! No score is invented: "top" ordering uses only persisted user signals
//! (favorites, then review state, then recency) and is deterministic.
use crate::{Database, Result};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};

/// The explicit, revisible ordering for "best" images.
/// 1. favorites first, 2. review: accepted, unreviewed, rejected, 3. most recent, 4. id.
pub(crate) const TOP_IMAGES_ORDER: &str = "a.favorite DESC, CASE a.review WHEN 'accepted' THEN 0 WHEN 'unreviewed' THEN 1 ELSE 2 END, length(a.modified_at) DESC, a.modified_at DESC, a.id DESC";

/// Characters currently shown as assets in this sheet/card context must
/// belong to an enabled library, matching gallery visibility rules.
const VISIBLE_ASSET: &str = "a.availability='present' AND EXISTS(SELECT 1 FROM libraries l WHERE l.id=a.library_id AND l.enabled=1)";

fn identity_text(identity: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = identity.get(*key).and_then(Value::as_str) {
            return text.to_string();
        }
    }
    String::new()
}

impl Database {
    /// Keep the recorded portrait even when missing; replacement is an explicit user choice.
    fn character_portrait(&self, character: &str) -> Result<Option<String>> {
        let mut q = self.connection.prepare(
            "SELECT a.id FROM assets a JOIN libraries l ON l.id=a.library_id WHERE EXISTS(SELECT 1 FROM asset_characters ac WHERE ac.asset_id=a.id AND ac.character_id=?1) ORDER BY CASE WHEN EXISTS(SELECT 1 FROM character_references r WHERE r.character_id=?1 AND r.asset_id=a.id AND r.variant_id='' AND r.is_primary=1) THEN 0 WHEN EXISTS(SELECT 1 FROM character_references r WHERE r.character_id=?1 AND r.asset_id=a.id) THEN 1 ELSE 2 END, (SELECT coalesce(min(r.position),999999) FROM character_references r WHERE r.character_id=?1 AND r.asset_id=a.id), a.relative_path, a.id LIMIT 1"
        )?;
        let portrait = q
            .query_row([character], |r| r.get::<_, String>(0))
            .optional()?;
        Ok(portrait)
    }

    fn linked_image_count(&self, character: &str) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT count(DISTINCT asset_id) FROM asset_characters WHERE character_id=?1",
            [character],
            |r| r.get(0),
        )?)
    }

    fn visible_image_count(&self, character: &str) -> Result<i64> {
        Ok(self.connection.query_row(
            &format!("SELECT count(*) FROM assets a WHERE {VISIBLE_ASSET} AND a.id IN (SELECT asset_id FROM asset_characters WHERE character_id=?1)"),
            [character], |r| r.get(0),
        )?)
    }

    /// Card list for the character gallery. `mainVariant` is only set when the
    /// sheet declares exactly one variant; no ranking between variants exists.
    pub fn character_cards(&self) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare(
            "SELECT c.id,c.name,(SELECT count(*) FROM character_variants v WHERE v.character_id=c.id) FROM characters c WHERE c.archived=0 ORDER BY c.name COLLATE NOCASE,c.id",
        )?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut cards = Vec::new();
        for (id, name, variant_count) in rows {
            let main_variant: Option<String> = if variant_count == 1 {
                Some(self.connection.query_row(
                    "SELECT name FROM character_variants WHERE character_id=? LIMIT 1",
                    [&id],
                    |r| r.get(0),
                )?)
            } else {
                None
            };
            let portrait = self.character_portrait(&id)?;
            let status = portrait
                .as_deref()
                .map(|id| self.asset_file_status(id))
                .transpose()?;
            cards.push(json!({
                "id":id,
                "name":name,
                "portrait":portrait,"portraitStatus":status,
                "imageCount":self.linked_image_count(&id)?,
                "visibleImageCount":self.visible_image_count(&id)?,
                "variantCount":variant_count,
                "mainVariant":main_variant,
            }));
        }
        Ok(cards)
    }

    pub fn portrait_candidates(&self, character: &str, offset: u32) -> Result<Value> {
        let mut query=self.connection.prepare("SELECT a.id,a.relative_path FROM assets a WHERE EXISTS(SELECT 1 FROM asset_characters ac WHERE ac.character_id=?1 AND ac.asset_id=a.id) ORDER BY a.relative_path,a.id LIMIT 48 OFFSET ?2")?;
        let ids = query
            .query_map(rusqlite::params![character, offset], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut items = Vec::new();
        for (id, path) in ids {
            items.push(json!({"id":id,"path":path,"status":self.asset_file_status(&id)?}));
        }
        Ok(json!({"items":items,"total":self.linked_image_count(character)?}))
    }
    /// Top images use only existing user signals; the maximum is 10.
    pub fn character_top_images(&self, character: &str) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare(&format!(
            "SELECT a.id,a.relative_path,a.width,a.height,a.favorite,a.review,a.modified_at FROM assets a WHERE {VISIBLE_ASSET} AND EXISTS(SELECT 1 FROM asset_characters ac WHERE ac.asset_id=a.id AND ac.character_id=?1) ORDER BY {TOP_IMAGES_ORDER} LIMIT 10"
        ))?;
        let rows = q.query_map([character], |r| {
            Ok(json!({
                "id":r.get::<_,String>(0)?,
                "path":r.get::<_,String>(1)?,
                "width":r.get::<_,Option<u32>>(2)?,
                "height":r.get::<_,Option<u32>>(3)?,
                "favorite":r.get::<_,bool>(4)?,
                "review":r.get::<_,String>(5)?,
                "modifiedAt":r.get::<_,Option<String>>(6)?,
            }))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Full read-only character sheet. P1/anatomy prompts come from the
    /// identity JSON persisted by the character editor; BG prompts come from
    /// variant prompt blocks. No migration needed: both fields already persist.
    pub fn character_sheet(&self, id: &str) -> Result<Value> {
        let (name, identity_json, archived, revision): (String, String, bool, u32) = self
            .connection
            .query_row(
                "SELECT name,identity_json,archived,revision FROM characters WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or("Personnage introuvable")?;
        let identity: Value = serde_json::from_str(&identity_json)?;
        let mut q = self
            .connection
            .prepare("SELECT v.id,v.name,json_extract(v.prompt_blocks_json,'$.BG') FROM character_variants v WHERE v.character_id=?1 AND json_extract(v.prompt_blocks_json,'$.BG') IS NOT NULL ORDER BY v.name,v.id")?;
        let backgrounds = q
            .query_map([id], |r| {
                Ok(json!({
                    "variantId":r.get::<_,String>(0)?,
                    "variantName":r.get::<_,String>(1)?,
                    "bg":r.get::<_,String>(2)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let portrait = self.character_portrait(id)?;
        let status = portrait
            .as_deref()
            .map(|id| self.asset_file_status(id))
            .transpose()?;
        Ok(json!({
            "id":id,
            "name":name,
            "archived":archived,
            "revision":revision,
            "portrait":portrait,"portraitStatus":status,
            "description":identity_text(&identity, &["description"]),
            "traits":identity_text(&identity, &["studioTraits"]),
            "identityPrompt":identity_text(&identity, &["identity_prompt","identityPrompt"]),
            "anatomyPrompt":identity_text(&identity, &["anatomy_prompt","anatomyPrompt"]),
            "backgrounds":backgrounds,
            "variants":self.variants(id)?,
            "topImages":self.character_top_images(id)?,
            "references":self.references(id)?,
            "imageCount":self.linked_image_count(id)?,
            "visibleImageCount":self.visible_image_count(id)?,
        }))
    }
}
