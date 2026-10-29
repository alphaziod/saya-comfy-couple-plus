//! Persisted interactive editor sessions.
//!
//! The source workflow is never rewritten: this module stores the editor
//! document plus the preserved UI/API source snapshot needed to resume. A
//! session survives restarts until it is explicitly deleted.
use crate::{storage::Result, Database};
use rusqlite::params;
use serde_json::{json, Value};

const MAX_ID: usize = 100;
const MAX_NAME: usize = 300;
const MAX_DOCUMENT: usize = 32 * 1024 * 1024;
const MAX_SOURCE: usize = 64 * 1024 * 1024;

fn check_text(label: &str, value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max {
        return Err(format!("{label} invalide").into());
    }
    Ok(())
}

impl Database {
    #[allow(clippy::too_many_arguments)]
    pub fn save_editor_session(
        &self,
        id: &str,
        name: &str,
        workflow: &str,
        source_format: &str,
        document_json: &str,
        ui_source_json: Option<&str>,
        original_api_json: Option<&str>,
        original_ids_json: &str,
    ) -> Result<()> {
        check_text("Identifiant de session", id, MAX_ID)?;
        check_text("Nom de session", name, MAX_NAME)?;
        if !matches!(source_format, "comfy_ui" | "comfy_api") {
            return Err("Format de session inconnu".into());
        }
        if document_json.len() > MAX_DOCUMENT || document_json.is_empty() {
            return Err("Document de session invalide".into());
        }
        for source in [ui_source_json, original_api_json].into_iter().flatten() {
            if source.len() > MAX_SOURCE {
                return Err("Source de session trop volumineuse".into());
            }
        }
        if original_ids_json.len() > MAX_DOCUMENT {
            return Err("Identifiants d'origine trop volumineux".into());
        }
        self.connection.execute(
            "INSERT INTO editor_sessions(id,name,workflow,source_format,document_json,ui_source_json,original_api_json,original_ids_json,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(id) DO UPDATE SET name=?2,workflow=?3,source_format=?4,document_json=?5,ui_source_json=?6,original_api_json=?7,original_ids_json=?8,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![id,name,workflow,source_format,document_json,ui_source_json,original_api_json,original_ids_json],
        )?;
        Ok(())
    }

    pub fn editor_sessions(&self) -> Result<Vec<Value>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,workflow,source_format,updated_at FROM editor_sessions ORDER BY updated_at DESC LIMIT 50",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "workflow": row.get::<_, String>(2)?,
                "sourceFormat": row.get::<_, String>(3)?,
                "updatedAt": row.get::<_, String>(4)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn editor_session(&self, id: &str) -> Result<Value> {
        check_text("Identifiant de session", id, MAX_ID)?;
        self.connection
            .query_row(
                "SELECT id,name,workflow,source_format,document_json,ui_source_json,original_api_json,original_ids_json,updated_at FROM editor_sessions WHERE id=?1",
                params![id],
                |row| {
                    Ok(json!({
                        "id": row.get::<_, String>(0)?,
                        "name": row.get::<_, String>(1)?,
                        "workflow": row.get::<_, String>(2)?,
                        "sourceFormat": row.get::<_, String>(3)?,
                        "document": row.get::<_, String>(4)?,
                        "uiSource": row.get::<_, Option<String>>(5)?,
                        "originalApi": row.get::<_, Option<String>>(6)?,
                        "originalIds": row.get::<_, String>(7)?,
                        "updatedAt": row.get::<_, String>(8)?,
                    }))
                },
            )
            .map_err(|e| format!("Session d'édition introuvable : {e}").into())
    }

    pub fn delete_editor_session(&self, id: &str) -> Result<()> {
        check_text("Identifiant de session", id, MAX_ID)?;
        self.connection
            .execute("DELETE FROM editor_sessions WHERE id=?1", params![id])?;
        Ok(())
    }
}
