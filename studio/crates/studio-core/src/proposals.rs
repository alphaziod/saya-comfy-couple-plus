use crate::{Database, Result};
use rusqlite::params;
use serde_json::{json, Value};
impl Database {
    pub fn save_proposal(&self, root: &str, request: &Value, response: &Value) -> Result<String> {
        let original = request["prompt"].as_str().ok_or("Prompt absent")?;
        let result = &response["result"];
        if result["original_prompt"] != original
            || !result["full_prompt"].is_string()
            || result["status"] != "proposed_unvalidated"
            || result["structural_validation"]["valid"] != true
            || !response["sourceSha256"]
                .as_str()
                .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Proposition sans preuve de conservation des tags".into());
        }
        if request.to_string().len() > 1_000_000 || response.to_string().len() > 8_000_000 {
            return Err("Proposition trop volumineuse".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute("INSERT INTO prompt_proposals(id,engine_root,request_json,response_json) VALUES(?1,?2,?3,?4)",params![id,root,request.to_string(),response.to_string()])?;
        Ok(id)
    }
    pub fn proposals(&self) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare(
            "SELECT id,decision,created_at FROM prompt_proposals ORDER BY rowid DESC LIMIT 50",
        )?;
        let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"decision":r.get::<_,String>(1)?,"createdAt":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn proposal(&self, id: &str) -> Result<Value> {
        let (root,request,response,decision,created,decided)=self.connection.query_row("SELECT engine_root,request_json,response_json,decision,created_at,decided_at FROM prompt_proposals WHERE id=?",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?)))?;
        Ok(
            json!({"id":id,"engineRoot":root,"request":serde_json::from_str::<Value>(&request)?,"response":serde_json::from_str::<Value>(&response)?,"decision":decision,"createdAt":created,"decidedAt":decided}),
        )
    }
    pub fn decide_proposal(
        &self,
        id: &str,
        decision: &str,
        confirmed: bool,
        current: &str,
    ) -> Result<String> {
        if !confirmed || !["accept", "reject"].contains(&decision) {
            return Err("Décision humaine explicite requise".into());
        }
        let response: String = self.connection.query_row(
            "SELECT response_json FROM prompt_proposals WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        let result: Value = serde_json::from_str(&response)?;
        if decision == "accept" && result["result"]["original_prompt"] != current {
            return Err(
                "Le prompt a changé depuis la proposition ; recalculer avant application".into(),
            );
        }
        let text = result["result"]["full_prompt"]
            .as_str()
            .ok_or("Proposition invalide")?;
        if self.connection.execute("UPDATE prompt_proposals SET decision=?2,decided_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND decision='pending'",params![id,decision])?!=1 {
            return Err("Décision déjà enregistrée ou proposition absente".into());
        }
        Ok(if decision == "accept" {
            text.into()
        } else {
            current.into()
        })
    }
}
