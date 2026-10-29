use crate::{gallery::file_digest, Database, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrashFile {
    pub id: String,
    pub path: String,
    pub bytes: String,
    pub modified: String,
    pub identity: String,
    pub sha256: String,
}

fn selection(ids: &[String]) -> Result<()> {
    if ids.is_empty() || ids.len() > 1000 {
        return Err("Sélection requise : 1 à 1000 images par opération".into());
    }
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("Sélection dupliquée".into());
    }
    Ok(())
}

fn regular_without_symlinks(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("Chemin absolu requis".into());
    }
    for ancestor in path.ancestors() {
        if std::fs::symlink_metadata(ancestor)?
            .file_type()
            .is_symlink()
        {
            return Err("Lien symbolique refusé pour la corbeille".into());
        }
    }
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err("Seuls des fichiers ordinaires peuvent être mis à la corbeille".into());
    }
    Ok(())
}

impl Database {
    pub fn set_catalog_removed(&self, ids: &[String], removed: bool) -> Result<()> {
        selection(ids)?;
        let _lock = self.index_lock()?;
        let tx = self.connection.unchecked_transaction()?;
        for id in ids {
            if tx.execute("UPDATE assets SET catalog_removed=?2 WHERE absolute_path=(SELECT absolute_path FROM assets WHERE id=?1)",params![id,removed])? == 0 {return Err("Image inconnue ; aucune modification appliquée".into());}
        }
        tx.commit()?;
        Ok(())
    }
    pub fn asset_file_status(&self, id: &str) -> Result<Value> {
        let (path,root):(String,String)=self.connection.query_row("SELECT a.absolute_path,l.root_path FROM assets a JOIN libraries l ON l.id=a.library_id WHERE a.id=?",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        // An unavailable source must never be turned into a missing-file diagnosis.
        if let Err(error) = crate::library_sources::checked_directory(Path::new(&root)) {
            return Ok(
                json!({"state":"source_inaccessible","reason":error.to_string(),"path":path}),
            );
        }
        let (state, reason) = match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => match self
                .asset_path(id)
                .and_then(|p| std::fs::File::open(p).map_err(Into::into))
            {
                Ok(_) => ("present", None),
                Err(e) => ("unreadable", Some(e.to_string())),
            },
            Ok(_) => (
                "unreadable",
                Some("Le chemin n’est plus un fichier ordinaire".into()),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ("missing", Some(e.to_string())),
            Err(e) => ("unreadable", Some(e.to_string())),
        };
        Ok(json!({"state":state,"reason":reason,"path":path}))
    }
    pub fn refresh_asset_presence(&self, id: &str) -> Result<Value> {
        let _lock = self.index_lock()?;
        let status = self.asset_file_status(id)?;
        if status["state"] == "source_inaccessible" {
            return Err(status["reason"]
                .as_str()
                .unwrap_or("Source inaccessible")
                .into());
        }
        self.connection.execute(
            "UPDATE assets SET availability=?2 WHERE absolute_path=?1",
            params![status["path"].as_str(), status["state"].as_str()],
        )?;
        Ok(status)
    }
    pub fn prepare_asset_trash(&self, ids: &[String]) -> Result<Vec<TrashFile>> {
        selection(ids)?;
        ids.iter().map(|id| {
            let path: String=self.connection.query_row("SELECT absolute_path FROM assets WHERE id=?",[id],|r|r.get(0))?;
            let p=Path::new(&path);
            regular_without_symlinks(p)?;
            if self.asset_path(id)?!=p {return Err("La cible résolue diffère de la sélection".into());}
            if let Some(db)=self.connection.path().filter(|p|!p.is_empty()) {
                let root=Path::new(db).canonicalize()?.parent().ok_or("Base sans dossier")?.to_owned();
                if ["pass-states","checkpoints","backups"].iter().any(|name|p.starts_with(root.join(name))) {return Err("État de passe ou sauvegarde protégé ; retrait du catalogue seulement".into());}
            }
            let refs:i64=self.connection.query_row("SELECT count(*) FROM character_references WHERE asset_id IN (SELECT id FROM assets WHERE absolute_path=?1)",[&path],|r|r.get(0))?;
            if refs>0 {return Err(format!("Image utilisée par {refs} référence(s) de personnage : retirer ces références explicitement avant la corbeille").into());}
            let stat=p.metadata()?;
            #[cfg(unix)]
            let identity={use std::os::unix::fs::MetadataExt;format!("{}:{}",stat.dev(),stat.ino())};
            #[cfg(not(unix))]
            let identity=format!("{:?}",stat.created()?);
            let sha256=file_digest(p)?;
            Ok(TrashFile{id:id.clone(),path,bytes:stat.len().to_string(),modified:format!("{:?}",stat.modified()?),identity,sha256})
        }).collect()
    }
    pub fn trash_assets(&self, plan: &[TrashFile], confirmed: bool) -> Result<Value> {
        if !confirmed {
            return Err("Confirmation dédiée de mise à la corbeille requise".into());
        }
        let _lock = self.index_lock()?;
        let ids = plan.iter().map(|f| f.id.clone()).collect::<Vec<_>>();
        if self.prepare_asset_trash(&ids)? != plan {
            return Err("La sélection a changé depuis la confirmation ; préparer à nouveau".into());
        }
        let mut trashed = Vec::new();
        let mut errors = Vec::new();
        for file in plan {
            let result = (|| -> Result<()> {
                if self.prepare_asset_trash(std::slice::from_ref(&file.id))? != [file.clone()] {
                    return Err("Fichier modifié après confirmation".into());
                }
                let output = std::process::Command::new("gio")
                    .arg("trash")
                    .arg("--")
                    .arg(&file.path)
                    .output()?;
                if !output.status.success() {
                    return Err(format!(
                        "Corbeille indisponible : {}",
                        String::from_utf8_lossy(&output.stderr)
                    )
                    .into());
                }
                self.connection.execute(
                    "UPDATE assets SET availability='missing' WHERE absolute_path=?",
                    [&file.path],
                )?;
                Ok(())
            })();
            match result {
                Ok(_) => trashed.push(file.id.clone()),
                Err(e) => {
                    errors.push(json!({"id":file.id,"path":file.path,"error":e.to_string()}));
                    break;
                }
            }
        }
        Ok(json!({"trashed":trashed,"errors":errors,"requested":plan.len()}))
    }
    pub fn asset_parent_directory(&self, id: &str) -> Result<PathBuf> {
        let path: String = self.connection.query_row(
            "SELECT absolute_path FROM assets WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        crate::library_sources::checked_directory(
            Path::new(&path).parent().ok_or("Dossier absent")?,
        )
    }
}
