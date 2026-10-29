use crate::Result;
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const LIMIT: usize = 16_000_000;
fn read_bounded(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take((LIMIT + 1) as u64).read_to_end(&mut data)?;
    Ok(data)
}
/// Calls only the trusted local engine's pure prompt/graph functions, never its MCP server.
pub fn call(root: &Path, tool: &str, args: &Value) -> Result<Value> {
    if !["analyze", "optimize", "validate", "inspect"].contains(&tool) {
        return Err("Outil local non autorisé".into());
    }
    let root = root.canonicalize()?;
    if !root.join("src/saya_prompt_engine/prompts.py").is_file() {
        return Err("Source Saya Prompt Engine introuvable".into());
    }
    let input = serde_json::to_vec(args)?;
    if input.len() > LIMIT {
        return Err("Requête supérieure à 16 Mo".into());
    }
    let mut child = Command::new("python3")
        .arg("-I")
        .arg("-c")
        .arg(format!(
            "{}\n{}",
            include_str!("../../../scripts/subgraph_inputs.py"),
            include_str!("../../../scripts/prompt-engine-bridge.py")
        ))
        .arg(root)
        .arg(tool)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("stdin absent")?;
    let stdout = child.stdout.take().ok_or("stdout absent")?;
    let stderr = child.stderr.take().ok_or("stderr absent")?;
    let writer = thread::spawn(move || stdin.write_all(&input));
    let output = thread::spawn(move || read_bounded(stdout));
    let errors = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < Duration::from_secs(20) => {
                thread::sleep(Duration::from_millis(25))
            }
            other => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "Moteur local interrompu (délai 20 s ou erreur processus) : {other:?}"
                ));
            }
        }
    };
    let written = writer.join().map_err(|_| "Écriture moteur interrompue")?;
    let output = output.join().map_err(|_| "Lecture moteur interrompue")??;
    let errors = errors.join().map_err(|_| "Lecture erreur interrompue")??;
    let status = status?;
    if !status.success() {
        return Err(format!(
            "Échec Saya Prompt Engine : {}",
            String::from_utf8_lossy(&errors[..errors.len().min(4000)])
        )
        .into());
    }
    written?;
    if output.len() > LIMIT {
        return Err("Réponse moteur supérieure à 16 Mo".into());
    }
    let result: Value = serde_json::from_slice(&output)?;
    if result.get("result").is_none() || !result["sourceSha256"].is_string() {
        return Err("Réponse moteur invalide".into());
    }
    Ok(result)
}
