//! Slice V2 — action targeting for casting: READ-ONLY list of the engine's action bank
//! (`data/actions/**`) and launch of the targeted casting.
//!
//! Two layers, the usual rule: the "Target actions" window reads the bank FILES (never a
//! write, never an invented field — only the fields present in the JSON come out), and the
//! launch goes through the engine CLI
//! (`test_solo.py --casting N --actions a,b,c --graine G`, encoded by
//! `engine_runtime::run_engine` with `PYTHONHASHSEED=0`). The engine itself validates
//! the existence, validity and solo status of each action (moteur/tirage.py
//! `draw_casting_actions`): this module never reinterprets the bank.

use crate::engine_runtime::{run_engine, DryRunOutcome, CASTING_MAX, DRY_RUN_TIMEOUT_SECS};
use crate::Result;
use regex::Regex;
use serde_json::{json, Value};
use std::{path::Path, sync::OnceLock, time::Duration};

/// Sub-folder of the action bank, relative to the engine directory.
pub const ACTIONS_DATA: &str = "data/actions";

/// Cap on targeted actions per casting (the bank holds 337 names as of
/// 2026-10-26, `_flaccide` sisters included; the argv stays short).
pub const ACTIONS_MAX: usize = 500;

/// Boolean fields of the schema (`data/actions/schema.json`) copied as-is when
/// present: the window shows them as badges (valide / pas_solo / hors_test...).
const BOOLEAN_FIELDS: [&str; 12] = [
    "valide",
    "a_revoir",
    "flaccid",
    "half_erect",
    "hors_test",
    "attente_lora",
    "pas_solo",
    "priorite",
    "sans_anatomie",
    "sans_corps_bas",
    "sans_cute",
    "visage_libre",
];

/// A/B SLICE CONVERGENCE (slice D, 2026-10-26) — AVAILABLE ACTIONS RULE.
///
/// The `available` filter below is the EXACT port of the rule documented by
/// slice A in `generez/tests/tests_actions.py` ("RÈGLE DES ACTIONS DISPONIBLES",
/// function `regle_actions_disponibles`), itself checked against the REAL engine draw
/// (`noyau.eligible_names`, noyau.py:801 + `noyau.action_retired`, noyau.py:715 +
/// `donnees.lora_blocked`, donnees.py:1569): test A W1.5 proves the equivalence with
/// the engine on the real bank (151 available / 337 names, sha256 7382dce6…), and the
/// test `actions_converge_with_real_bank` (#[ignore], run against the real
/// engine) compares THIS port byte for byte against the same listing.
///
/// An action leaves the solo draw pool as soon as ONE of these conditions is true:
/// 1. flags: `valide` missing/false, `hors_random` true, `pas_solo` true
///    (`a_revoir`/`hors_test`/`attente_lora` remove NOTHING: the engine does not read them);
/// 2. blocked LoRA: name among the refused ones, or "naked ribbon" at strength ≥ 0.59;
/// 3. hard-coded removal (`noyau.action_retired`): x_dildo_chaise and x_dildo_chaise_*,
///    plus the 4 names of `_RETIREES`;
/// 4. CHARACTER filters: `persos_rx` must match the context (p1 + name, lowercase) and
///    `persos_exclure_rx` must NOT match it — base context "1futanari".
///
/// The listed bank is the ENGINE's (337 = 253 files + 84 `_flaccide` sisters
/// cloned in memory by `donnees.enrich_flaccid`, donnees.py:1478): the sisters are
/// real draw actions — targetable (`--actions`) and excludable (`--exclude`).
/// Consequence of Saya's feedback 1 (pass 3): an unavailable action drops out of both
/// windows (targeting AND exclusion) — it can never be drawn anyway.
const BASE_CONTEXT: &str = "1futanari";

/// Refused LoRAs (`donnees._REFUSED_LORA`, donnees.py:741) — pure literals, the
/// search is case-insensitive like the engine's `re.I`.
const REFUSED_LORAS: [&str; 9] = [
    "self_fisting",
    "horse_cock",
    "horsecock",
    "freaky_dick",
    "chikubi",
    "futabeads",
    "autofellatio",
    "autofacial",
    "self_suck",
];

/// Hard-coded removals (`noyau._RETIREES`, noyau.py:707): the actions file is not
/// rewritten, the core removes these names from the draw.
const RETIRED_RULE: [&str; 4] = [
    "x_dildo_masturbation",
    "x_dildo_masturbation_flaccide",
    "filet_x_dildo_masturbation",
    "filet_x_dildo_masturbation_flaccide",
];

/// Python truthiness of a JSON value: the engine reads flags through `dict.get` —
/// missing/null/false/0/"" = false, anything else = true.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `donnees.lora_blocked` (donnees.py:1569): refused LoRA, or naked ribbon at
/// strength ≥ 0.59 (the red bleeds onto the hair). An unreadable strength counts as 0.
fn lora_blocked(action: &Value) -> bool {
    let lora = action.get("lora");
    let nom = lora
        .and_then(|l| l.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    if REFUSED_LORAS.iter().any(|r| nom.contains(r)) {
        return true;
    }
    if nom.contains("naked ribbon") {
        let strength = lora
            .and_then(|l| l.get("strength"))
            .and_then(|s| match s {
                Value::Number(n) => n.as_f64(),
                Value::String(t) => t.trim().parse::<f64>().ok(),
                _ => None,
            })
            .unwrap_or(0.0);
        return strength >= 0.59;
    }
    false
}

/// `noyau.action_retired` (noyau.py:715): hard-coded removal in the core.
fn action_retired(nom: &str) -> bool {
    nom == "x_dildo_chaise" || nom.starts_with("x_dildo_chaise_") || RETIRED_RULE.contains(&nom)
}

/// `donnees.forces_erection` (donnees.py:843): `\berect penis\b|\berection\b` in
/// `add`, or a standalone "erect" in the name (case-insensitive). The second
/// Python pattern uses lookarounds `(?<![a-z])erect(?![a-z])` that the regex crate does
/// not support: same semantics written by hand (boundary = not an a-z letter).
fn forces_erection(nom: &str, action: &Value) -> bool {
    static ERECT_FORCE: OnceLock<Regex> = OnceLock::new();
    let erect_match = ERECT_FORCE
        .get_or_init(|| Regex::new(r"(?i)\berect penis\b|\berection\b").expect("regex figée"))
        .is_match(action.get("add").and_then(Value::as_str).unwrap_or(""));
    if erect_match {
        return true;
    }
    let lower = nom.to_lowercase();
    let bytes = lower.as_bytes();
    let mut cursor = 0;
    while let Some(position) = lower[cursor..].find("erect") {
        let p = cursor + position;
        let after = p + "erect".len();
        let bound_before = p == 0 || !bytes[p - 1].is_ascii_lowercase();
        let bound_after = after >= bytes.len() || !bytes[after].is_ascii_lowercase();
        if bound_before && bound_after {
            return true;
        }
        cursor = p + 1;
    }
    false
}

/// `donnees.solo_drawable` (donnees.py:1424): allowed in solo — precondition of the
/// flaccid cloning.
fn solo_drawable(action: &Value) -> bool {
    truthy(action.get("valide"))
        && !truthy(action.get("hors_random"))
        && !truthy(action.get("pas_solo"))
        && !lora_blocked(action)
}

/// `donnees.wants_flaccid_sister` (donnees.py:1463): this action must get its flaccid
/// variant.
fn wants_flaccid_sister(nom: &str, action: &Value) -> bool {
    if truthy(action.get("flaccid")) || nom.ends_with("_flaccide") {
        return false;
    }
    solo_drawable(action) && !forces_erection(nom, action)
}

/// `donnees._ban_slime` (donnees.py:1471): a slime does not draw the human flaccid
/// variant — "slime" joins `persos_exclure_rx` if it is not already there.
fn ban_slime(action: &mut Value) {
    let rx = action
        .get("persos_exclure_rx")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if rx.to_lowercase().contains("slime") {
        return;
    }
    if let Some(object) = action.as_object_mut() {
        object.insert(
            "persos_exclure_rx".into(),
            json!(if rx.is_empty() {
                "slime".to_owned()
            } else {
                format!("{rx}|slime")
            }),
        );
    }
}

/// A/B CONVERGENCE: the `available` filter, exact port of the draw rule (see the
/// documentation block above). `in_order` covers the engine constraint specific to
/// FILES (`valider_actions_familles` refuses an id absent from `_ordre.json`); the sisters
/// cloned in memory belong to the engine bank by construction and pass
/// `in_order = true`.
fn available(action: &Value, in_order: bool, nom: &str) -> bool {
    if !in_order
        || !truthy(action.get("valide"))
        || truthy(action.get("hors_random"))
        || truthy(action.get("pas_solo"))
        || lora_blocked(action)
        || action_retired(nom)
    {
        return false;
    }
    if let Some(rx) = action
        .get("persos_rx")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
    {
        let matched = Regex::new(rx).map(|re| re.is_match(BASE_CONTEXT));
        match matched {
            Ok(true) => {}
            _ => return false,
        }
    }
    // `action.get("persos_exclure_rx") or "$^"`: empty string = never matched.
    let exclude = action
        .get("persos_exclure_rx")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .unwrap_or("$^");
    match Regex::new(exclude).map(|re| re.is_match(BASE_CONTEXT)) {
        Ok(true) => false,
        _ => true,
    }
}

/// Port of `donnees.enrich_flaccid` (donnees.py:1478): the `_flaccide` sisters of the
/// engine bank, cloned IN MEMORY (never written). Fidelity to the engine:
/// - ABSENT sister: copy of the source + `flaccid:true, valide:true`, `priorite` and
///   `a_revoir` removed, `_ban_slime` applied;
/// - sister ALREADY PRESENT (file): `flaccid:true, valide:true, hors_random:false`
///   FORCED (even if the file says otherwise), `pas_solo:false` only if the
///   source does not have it, `_ban_slime` applied.
/// A source never becomes a candidate if it is not solo-drawable or if it forces
/// erection (`wants_flaccid_sister`, donnees.py:1463).
fn enrich_flaccid_sisters(bank: &mut Vec<(String, String, Value)>) {
    let sources: Vec<(String, String, Value)> = bank
        .iter()
        .filter(|(nom, _, action)| wants_flaccid_sister(nom, action))
        .map(|(nom, famille, action)| (nom.clone(), famille.clone(), action.clone()))
        .collect();
    for (nom, famille, source) in sources {
        let sister = format!("{nom}_flaccide");
        match bank
            .iter_mut()
            .find(|(existing, _, _)| *existing == sister)
        {
            Some((_, _, twin)) => {
                if let Some(object) = twin.as_object_mut() {
                    object.insert("flaccid".into(), json!(true));
                    object.insert("valide".into(), json!(true));
                    object.insert("hors_random".into(), json!(false));
                    if !truthy(source.get("pas_solo")) {
                        object.insert("pas_solo".into(), json!(false));
                    }
                }
                ban_slime(twin);
            }
            None => {
                let mut twin = source.clone();
                if let Some(object) = twin.as_object_mut() {
                    object.insert("flaccid".into(), json!(true));
                    object.insert("valide".into(), json!(true));
                    object.remove("priorite");
                    object.remove("a_revoir");
                }
                ban_slime(&mut twin);
                bank.push((sister, famille, twin));
            }
        }
    }
}

/// An action of the bank: the full name (key of the engine bank, e.g. `x_pinces_cheveux`),
/// its file family, its `add` tags, the LoRA it carries (`lora.name` of the schema —
/// "lora" family of the bank, e.g. `lora_naked_ribbon`) and the schema fields
/// present. No inferred field, no default value: absent from the JSON = absent from
/// the response. `available` carries the converged A/B rule (see the documentation block).
fn entry(nom: &str, famille: &str, action: &Value, in_order: bool) -> Value {
    let mut out = json!({
        "name": nom,
        "famille": famille,
        "disponible": available(action, in_order, nom),
    });
    if let Some(object) = action.as_object() {
        for field in ["add", "note"] {
            if let Some(value) = object.get(field).and_then(Value::as_str) {
                out[field] = json!(value);
            }
        }
        if let Some(lora) = object
            .get("lora")
            .and_then(|l| l.get("name"))
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
        {
            out["lora"] = json!(lora);
        }
        for field in BOOLEAN_FIELDS {
            if let Some(value) = object.get(field).and_then(Value::as_bool) {
                out[field] = json!(value);
            }
        }
    }
    out
}

/// Lists the engine's action bank, READ-ONLY: the historical order of
/// `_ordre.json` when it exists (the engine bank's), otherwise the read order of the
/// `*.actions.json` files. The bank ALSO carries the `_flaccide` sisters cloned in
/// memory by the engine (`enrich_flaccid_sisters`, port of `donnees.enrich_flaccid`) —
/// real draw actions, at the end of the list. A file family that does not
/// parse is a clean error (the engine bank refuses too — same source of
/// truth).
pub fn list_actions(dir: &Path) -> Result<Vec<Value>> {
    let order_path = dir.join("_ordre.json");
    let order: Option<Vec<String>> = if order_path.is_file() {
        Some(serde_json::from_slice(
            &std::fs::read(&order_path)
                .map_err(|e| format!("{} illisible : {e}", order_path.display()))?,
        )?)
    } else {
        None
    };
    let order_set: std::collections::HashSet<String> = order
        .as_ref()
        .map(|list| list.iter().cloned().collect())
        .unwrap_or_default();
    let mut by_name: Vec<(String, String, Value)> = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("Banque d'actions illisible dans {} : {e}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort_by_key(|entry| entry.file_name());
    for path in paths {
        let path = path.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json")
            || !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".actions.json"))
        {
            continue;
        }
        let payload: Value = serde_json::from_slice(
            &std::fs::read(&path).map_err(|e| format!("{} illisible : {e}", path.display()))?,
        )?;
        let famille = payload["famille"].as_str().unwrap_or_default().to_owned();
        let Some(actions) = payload["actions"].as_object() else {
            return Err(format!("{} : actions absentes", path.display()).into());
        };
        for (nom, action) in actions {
            if by_name.iter().any(|(existing, _, _)| existing == nom) {
                return Err(
                    format!("{}/{} : id dupliqué entre fichiers", path.display(), nom).into(),
                );
            }
            by_name.push((nom.clone(), famille.clone(), action.clone()));
        }
    }
    enrich_flaccid_sisters(&mut by_name);
    let in_bank = |nom: &str| nom.ends_with("_flaccide") || order_set.contains(nom);
    let Some(order) = order else {
        return Ok(by_name
            .into_iter()
            .map(|(nom, famille, action)| entry(&nom, &famille, &action, in_bank(&nom)))
            .collect());
    };
    // Historical order first (the engine bank's), then the ids outside the order —
    // including the cloned flaccid sisters — at the end.
    let mut out: Vec<Value> = Vec::with_capacity(by_name.len());
    for nom in &order {
        if let Some(position) = by_name.iter().position(|(existing, _, _)| existing == nom) {
            let (nom, famille, action) = by_name.remove(position);
            out.push(entry(&nom, &famille, &action, true));
        }
    }
    out.extend(by_name.into_iter().map(|(nom, famille, action)| {
        // Flaccid sisters: in the engine bank by construction. Any OTHER id outside the
        // order would be refused by the engine (`valider_actions_familles`): `available`
        // treats it as unavailable through the same constraint.
        entry(&nom, &famille, &action, in_bank(&nom))
    }));
    Ok(out)
}

/// Casting with targeted actions (slice V2): `python3 test_solo.py --casting N
/// [--famille X] --actions a,b,c --graine G` in the default engine directory,
/// `PYTHONHASHSEED=0`. The N OCs each carry ONE of the targeted actions, in a cycle
/// (moteur/tirage.py `draw_casting_actions`). No image: pass-1 catalogue in
/// `sortie/casting/catalogue.json`. Family `None` (or empty) asks for the normal draw.
pub fn casting_actions(
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    actions: &[String],
    exclusions: &[String],
) -> Result<DryRunOutcome> {
    casting_actions_in_with_timeout(
        &crate::engine_runtime::engine_dir(),
        graine,
        nombre,
        famille,
        actions,
        exclusions,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same casting in an explicit engine directory (tests, configuration).
pub fn casting_actions_in_with_timeout(
    dir: &Path,
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    actions: &[String],
    exclusions: &[String],
    timeout: Duration,
) -> Result<DryRunOutcome> {
    if nombre == 0 || nombre > CASTING_MAX {
        return Err(format!(
            "Casting : nombre d'OC hors limites ({nombre}, attendu 1..={CASTING_MAX})"
        )
        .into());
    }
    let mut seen: Vec<String> = Vec::new();
    for action in actions {
        let action = action.trim();
        if action.is_empty() {
            continue;
        }
        if action.len() > 100
            || !action
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!(
                "Casting : nom d'action ciblée invalide ({action}) — attendu un nom de la banque du moteur (lettres, chiffres, _)"
            )
            .into());
        }
        if seen.iter().any(|v| v == action) {
            return Err(format!("Casting : action ciblée en doublon ({action})").into());
        }
        seen.push(action.to_owned());
    }
    if seen.is_empty() {
        return Err("Casting : aucune action ciblée".into());
    }
    if seen.len() > ACTIONS_MAX {
        return Err(format!(
            "Casting : trop d'actions ciblées ({}, plafond {ACTIONS_MAX})",
            seen.len()
        )
        .into());
    }
    let famille = famille.map(str::trim).filter(|f| !f.is_empty());
    if let Some(famille) = famille {
        if !famille
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("Casting : famille invalide ({famille})").into());
        }
    }
    let mut args = vec!["--casting".into(), nombre.to_string()];
    if let Some(famille) = famille {
        args.push("--famille".into());
        args.push(famille.to_owned());
    }
    args.push("--actions".into());
    args.push(seen.join(","));
    args.push("--graine".into());
    args.push(graine.to_string());
    // W1 exclusion (Saya's feedback 6): `--exclude a,b,c` — EXCLUSION WINS over
    // targeting (excluded targeted actions drop out of the list on the engine side), never
    // produced. Refused before spawn if the engine ignores the flag (A/B convergence).
    crate::engine_runtime::push_exclusions(dir, &mut args, exclusions)?;
    run_engine(
        dir,
        &args,
        crate::engine_runtime::CASTING_CATALOGUE,
        timeout,
        graine,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, content: String) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn list_reads_files_in_historical_order() {
        let dir = std::env::temp_dir().join("saya-actions-bank-lister");
        let _ = std::fs::remove_dir_all(&dir);
        write_file(
            &dir.join("_ordre.json"),
            json!(["x_b", "x_a", "y_c"]).to_string(),
        );
        write_file(
            &dir.join("y.actions.json"),
            json!({"famille": "y", "actions": {
                "y_c": {"add": "tag c", "valide": true, "lora": {"name": "pole dancing_illustrious_V2.0", "strength": 0.6}}
            }}).to_string(),
        );
        write_file(
            &dir.join("x.actions.json"),
            json!({"famille": "x", "actions": {
                "x_a": {"add": "tag a", "valide": true, "pas_solo": true, "note": "a"},
                "x_b": {"add": "tag b", "valide": false, "hors_test": true}
            }})
            .to_string(),
        );
        // Read-only: the file bytes do not change.
        let before: Vec<_> = ["_ordre.json", "x.actions.json", "y.actions.json"]
            .iter()
            .map(|n| std::fs::read(dir.join(n)).unwrap())
            .collect();
        let list = list_actions(&dir).unwrap();
        let names: Vec<&str> = list.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            ["x_b", "x_a", "y_c", "y_c_flaccide"],
            "ordre historique respecté, sœur flaccide clonée en mémoire à la fin"
        );
        let x_a = list.iter().find(|a| a["name"] == "x_a").unwrap();
        assert_eq!(x_a["famille"], json!("x"));
        assert_eq!(x_a["add"], json!("tag a"));
        assert_eq!(x_a["note"], json!("a"));
        assert_eq!(x_a["valide"], json!(true));
        assert_eq!(x_a["pas_solo"], json!(true));
        assert_eq!(
            x_a["disponible"],
            json!(false),
            "pas_solo disqualifie (règle du tirage moteur, convergence A/B)"
        );
        assert!(
            x_a.get("hors_test").is_none(),
            "champ absent = champ non inventé"
        );
        assert!(
            x_a.get("lora").is_none(),
            "action sans LoRA = pas de champ lora"
        );
        let y_c = list.iter().find(|a| a["name"] == "y_c").unwrap();
        assert_eq!(
            y_c["lora"],
            json!("pole dancing_illustrious_V2.0"),
            "lora.name littéral porté par l'action"
        );
        assert_eq!(y_c["disponible"], json!(true));
        // Sister cloned in memory: copy of the source + flaccid/valide forced,
        // banned from slimes, never written to a file.
        let sister = list.iter().find(|a| a["name"] == "y_c_flaccide").unwrap();
        assert_eq!(sister["famille"], json!("y"));
        assert_eq!(sister["add"], json!("tag c"));
        assert_eq!(sister["flaccid"], json!(true));
        assert_eq!(sister["valide"], json!(true));
        assert_eq!(sister["disponible"], json!(true));
        assert!(sister.get("hors_test").is_none(), "champ non inventé");
        assert_eq!(sister["lora"], json!("pole dancing_illustrious_V2.0"));
        let x_b = list.iter().find(|a| a["name"] == "x_b").unwrap();
        assert_eq!(x_b["valide"], json!(false));
        assert_eq!(
            x_b["disponible"],
            json!(false),
            "valide:false disqualifie (hors_test ne retire rien : le moteur ne le lit pas)"
        );
        assert!(x_b.get("note").is_none());
        // Read-only: the file bytes do not change.
        for (position, nom) in ["_ordre.json", "x.actions.json", "y.actions.json"]
            .iter()
            .enumerate()
        {
            assert_eq!(
                std::fs::read(dir.join(nom)).unwrap(),
                before[position],
                "{nom} inchangé (lecture seule)"
            );
        }
    }

    #[test]
    fn list_without_historical_order_stays_readable_by_sorted_file() {
        let dir = std::env::temp_dir().join("saya-actions-bank-sans-ordre");
        let _ = std::fs::remove_dir_all(&dir);
        write_file(
            &dir.join("z.actions.json"),
            json!({"famille": "z", "actions": {"z_1": {"add": "un"}}}).to_string(),
        );
        write_file(
            &dir.join("a.actions.json"),
            json!({"famille": "a", "actions": {"a_1": {"add": "deux"}, "a_0": {}}}).to_string(),
        );
        let list = list_actions(&dir).unwrap();
        let names: Vec<&str> = list.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            ["a_0", "a_1", "z_1"],
            "fichiers triés, _ordre absent toléré"
        );
        assert_eq!(list[0]["famille"], json!("a"));
    }

    #[test]
    fn list_refuses_a_broken_file_and_duplicates() {
        let broken = std::env::temp_dir().join("saya-actions-bank-casse");
        let _ = std::fs::remove_dir_all(&broken);
        write_file(&broken.join("x.actions.json"), "pas du json".into());
        assert!(list_actions(&broken).is_err(), "JSON cassé refusé");
        let duplicate = std::env::temp_dir().join("saya-actions-bank-doublon");
        let _ = std::fs::remove_dir_all(&duplicate);
        write_file(
            &duplicate.join("x.actions.json"),
            json!({"famille": "x", "actions": {"x_1": {}}}).to_string(),
        );
        write_file(
            &duplicate.join("y.actions.json"),
            json!({"famille": "y", "actions": {"x_1": {}}}).to_string(),
        );
        assert!(
            list_actions(&duplicate).is_err(),
            "id dupliqué entre fichiers refusé"
        );
    }

    #[test]
    fn casting_actions_validates_arguments_before_any_launch() {
        let dir = std::env::temp_dir().join("saya-actions-casting-vide");
        let _ = std::fs::remove_dir_all(&dir);
        let refusal = |result: crate::Result<DryRunOutcome>| {
            let err = result.err().expect("refus attendu");
            err.to_string()
        };
        // None of these refusals must touch the engine disk: dir stays empty.
        assert!(refusal(casting_actions_in_with_timeout(
            &dir,
            7,
            0,
            None,
            &["seins".into()],
            &[],
            Duration::from_secs(1)
        ))
        .contains("nombre d'OC hors limites"));
        assert!(refusal(casting_actions_in_with_timeout(
            &dir,
            7,
            5,
            None,
            &[],
            &[],
            Duration::from_secs(1)
        ))
        .contains("aucune action ciblée"));
        assert!(refusal(casting_actions_in_with_timeout(
            &dir,
            7,
            5,
            None,
            &["Seins!".into()],
            &[],
            Duration::from_secs(1)
        ))
        .contains("nom d'action ciblée invalide"));
        assert!(refusal(casting_actions_in_with_timeout(
            &dir,
            7,
            5,
            None,
            &["seins", "seins"].map(String::from).as_slice(),
            &[],
            Duration::from_secs(1)
        ))
        .contains("doublon"));
        assert!(refusal(casting_actions_in_with_timeout(
            &dir,
            7,
            5,
            Some("ZOMBIE"),
            &["seins".into()],
            &[],
            Duration::from_secs(1)
        ))
        .contains("famille invalide"));
        // Duplicate exclusion: refused before any engine launch.
        assert!(refusal(casting_actions_in_with_timeout(
            &dir,
            7,
            5,
            None,
            &["seins".into()],
            &["x", "x"].map(String::from).as_slice(),
            Duration::from_secs(1)
        ))
        .contains("exclue en doublon"));
        assert!(
            dir.join("sortie").exists() == false,
            "un refus ne lance jamais le moteur"
        );
    }

    /// argv → engine wiring proven by a real subprocess (isolated stub, pattern
    /// rotations.rs): the engine receives `--casting N --actions a,b --graine G` and the
    /// stub catalogue is re-read and validated.
    #[test]
    fn casting_actions_passes_right_arguments_to_engine() {
        let engine = std::env::temp_dir().join("saya-studio-v2-actions-stub");
        let _ = std::fs::remove_dir_all(&engine);
        std::fs::create_dir_all(&engine).unwrap();
        let shot = json!({
            "stem": "bny-00001-stub-oc-noobai",
            "name": "Stub OC",
            "famille": "bny",
            "model": "noobai",
            "seed": 248919022623436u64,
            "casting": {
                "prefixe": "embedding:lazypos, embedding:lazyneg",
                "decor_libre": "night club, neon signs",
                "action": "hands clasped behind head",
                "p1": "1girl, solo, elf",
                "anatomy": "(small wings:1.25), cute_tail",
                "negative": "embedding:lazyneg",
                "p2": "", "p2_anatomy": "",
                "checkpoint": "noobai.safetensors",
                "lora_loaders": {"662": {"loras": [], "text": ""}, "663": {"loras": [], "text": ""}, "664": {"loras": [], "text": ""}},
                "format_paysage": false,
                "seed": -1
            }
        });
        std::fs::write(
            engine.join("test_solo.py"),
            r#"import json, sys
from pathlib import Path
args = sys.argv[1:]
Path("argv.json").write_text(json.dumps(args))
# le stub connaît le flag d'exclusion W1 : "--exclude"
dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
shot = json.loads(Path("shot_fixture.json").read_text())
(dossier / "catalogue.json").write_text(json.dumps([shot], ensure_ascii=False))
print("CASTING stub, actions ciblées « " + args[args.index("--actions") + 1] + " », rien envoyé")
print("CATALOGUE sortie/casting/catalogue.json")
"#,
        )
        .unwrap();
        std::fs::write(engine.join("shot_fixture.json"), shot.to_string()).unwrap();

        let _outcome = casting_actions_in_with_timeout(
            &engine,
            7,
            2,
            Some("bny"),
            &["seins".to_owned(), "masturbation".to_owned()],
            &[],
            Duration::from_secs(30),
        )
        .expect("casting actions via stub");
        let argv: Vec<String> =
            serde_json::from_slice(&std::fs::read(engine.join("argv.json")).unwrap()).unwrap();
        let expected = vec![
            "--casting".to_owned(),
            "2".to_owned(),
            "--famille".to_owned(),
            "bny".to_owned(),
            "--actions".to_owned(),
            "seins,masturbation".to_owned(),
            "--graine".to_owned(),
            "7".to_owned(),
        ];
        assert_eq!(argv, expected, "argv exact transmis au moteur");
        // W1 exclusion: --exclude added to the argv (exclusion wins over targeting).
        let outcome = casting_actions_in_with_timeout(
            &engine,
            7,
            2,
            Some("bny"),
            &["seins".to_owned()],
            &["masturbation".to_owned()],
            Duration::from_secs(30),
        )
        .expect("casting actions ciblées + exclusion via stub");
        let argv: Vec<String> =
            serde_json::from_slice(&std::fs::read(engine.join("argv.json")).unwrap()).unwrap();
        assert!(
            argv.windows(2)
                .any(|w| w[0] == crate::engine_runtime::CASTING_EXCLUDE_FLAG
                    && w[1] == "masturbation"),
            "flag d'exclusion transmis : {argv:?}"
        );
        assert!(outcome.stdout.contains("stub"));
        let shots = crate::casting::read_catalogue(&outcome.catalogue).unwrap();
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0]["stem"], json!("bny-00001-stub-oc-noobai"));
    }

    /// Each branch of the converged A/B rule, on a fixture covering the real bank
    /// patterns (the `.` action that matches everything, the goth pattern that does not match
    /// "1futanari", the `\bimp\b` exclusion, naked ribbon below/above 0.59,
    /// the core's hard-coded removals, the flags, the flaccid sisters and their
    /// ban_slime). Rust mirror of `regle_actions_disponibles` (tests_actions.py:107),
    /// run through the same path as production: enrichment then entry.
    #[test]
    fn available_applies_the_engines_converged_rule() {
        let bank = json!({
            "a_valide": {"add": "tag", "valide": true},
            "b_invalide": {"add": "tag", "valide": false},
            "c_valide_absent": {"add": "tag"},
            "d_hors_random": {"add": "tag", "valide": true, "hors_random": true},
            "e_pas_solo": {"add": "tag", "valide": true, "pas_solo": true},
            "f_hors_test": {"add": "tag", "valide": true, "hors_test": true},
            "g_a_revoir": {"add": "tag", "valide": true, "a_revoir": true},
            "h_lora_refusee": {"add": "tag", "valide": true, "lora": {"name": "self_fisting_illustrious_V2.0", "strength": 0.8}},
            "i_ribbon_fort": {"add": "tag", "valide": true, "lora": {"name": "naked ribbon_illustrious_V2.0", "strength": 0.6}},
            "j_ribbon_faible": {"add": "tag", "valide": true, "lora": {"name": "naked ribbon_illustrious_V2.0", "strength": 0.5}},
            "x_dildo_chaise_variante": {"add": "tag", "valide": true},
            "x_dildo_masturbation": {"add": "tag", "valide": true},
            "m_persos_goth": {"add": "tag", "valide": true, "persos_rx": r"\bgoth(?:ic)?\b|\bemo\b|succub"},
            "n_persos_point": {"add": "tag", "valide": true, "persos_rx": "."},
            "o_exclure_imp": {"add": "tag", "valide": true, "persos_exclure_rx": r"\bimp\b"},
            "p_erection_add": {"add": "(erect penis:1.2), tag", "valide": true},
            "erect_pose": {"add": "tag", "valide": true},
            "r_soeur_existante": {"add": "tag", "valide": true},
            "r_soeur_existante_flaccide": {"add": "tag", "valide": false, "hors_random": true, "a_revoir": true},
            "s_duo_sans_soeur": {"add": "tag", "valide": true, "pas_solo": true},
        });
        let mut triples: Vec<(String, String, Value)> = bank
            .as_object()
            .unwrap()
            .iter()
            .map(|(nom, action)| (nom.clone(), "x".to_owned(), action.clone()))
            .collect();
        enrich_flaccid_sisters(&mut triples);
        let list: Vec<Value> = triples
            .iter()
            .map(|(nom, famille, action)| entry(nom, famille, action, true))
            .collect();
        let avail = |nom: &str| {
            list
                .iter()
                .find(|a| a["name"] == nom)
                .unwrap_or_else(|| panic!("{nom} absent"))["disponible"]
                .as_bool()
                .unwrap()
        };
        assert!(avail("a_valide"));
        assert!(!avail("b_invalide"), "valide:false disqualifie");
        assert!(!avail("c_valide_absent"), "valide absent disqualifie");
        assert!(!avail("d_hors_random"), "hors_random disqualifie");
        assert!(!avail("e_pas_solo"), "pas_solo disqualifie");
        assert!(
            avail("f_hors_test"),
            "hors_test ne retire RIEN : le moteur ne le lit pas"
        );
        assert!(
            avail("g_a_revoir"),
            "a_revoir ne retire RIEN : le moteur ne le lit pas"
        );
        assert!(!avail("h_lora_refusee"), "LoRA refusée disqualifie");
        assert!(!avail("i_ribbon_fort"), "naked ribbon ≥ 0.59 disqualifie");
        assert!(avail("j_ribbon_faible"), "naked ribbon < 0.59 reste");
        assert!(
            !avail("x_dildo_chaise_variante"),
            "x_dildo_chaise_* retirée par le noyau"
        );
        assert!(!avail("x_dildo_masturbation"), "_RETIREES retirée");
        assert!(
            !avail("m_persos_goth"),
            "persos_rx sans match sur « 1futanari » disqualifie"
        );
        assert!(avail("n_persos_point"), "persos_rx « . » matche tout");
        assert!(avail("o_exclure_imp"), "exclusion sans match reste");
        // Forced erection blocks flaccid CLONING, not the draw: filet_genoux_erect
        // is in test A's available listing (real bank).
        assert!(avail("p_erection_add"));
        assert!(avail("erect_pose"));
        assert!(avail("r_soeur_existante"));
        // Sister already present: flags FORCED by enrich_flaccid on the internal value
        // (hors_random/persos_exclure_rx do not come out in entry, they drive the
        // rule) — valide:true and hors_random:false win over the file.
        let internal_twin = &triples
            .iter()
            .find(|(nom, _, _)| nom == "r_soeur_existante_flaccide")
            .unwrap()
            .2;
        assert_eq!(
            internal_twin["valide"],
            json!(true),
            "valide forcé sur la sœur"
        );
        assert_eq!(
            internal_twin["hors_random"],
            json!(false),
            "hors_random forcé"
        );
        assert_eq!(
            internal_twin["persos_exclure_rx"],
            json!("slime"),
            "ban_slime appliqué au twin"
        );
        let sister = list
            .iter()
            .find(|a| a["name"] == "r_soeur_existante_flaccide")
            .unwrap();
        assert_eq!(sister["valide"], json!(true));
        assert_eq!(sister["flaccid"], json!(true));
        // Python fidelity: only a CLONED sister loses priorite/a_revoir — a sister
        // already in the files keeps them (a_revoir does not disqualify anyway).
        assert_eq!(
            sister["a_revoir"],
            json!(true),
            "twin de fichier : champs gardés"
        );
        assert!(
            avail("r_soeur_existante_flaccide"),
            "la sœur forcée valide entre dans le pool"
        );
        // Available sources clone their sister; blocked cases do not.
        for source in [
            "a_valide",
            "n_persos_point",
            "o_exclure_imp",
            "r_soeur_existante",
        ] {
            let sister_name = format!("{source}_flaccide");
            assert!(
                list.iter().any(|a| a["name"] == json!(sister_name)),
                "{source} cloné"
            );
        }
        assert!(
            !list.iter().any(|a| a["name"] == "p_erection_add_flaccide"),
            "érection forcée : pas de sœur"
        );
        assert!(
            !list
                .iter()
                .any(|a| a["name"] == "s_duo_sans_soeur_flaccide"),
            "une pas_solo ne clone pas de sœur"
        );
        assert!(
            !list.iter().any(|a| a["name"] == "erect_pose_flaccide"),
            "un nom à érection forcée ne clone pas de sœur"
        );
    }

    /// A/B CONVERGENCE against the REAL ENGINE: `list_actions` on `SAYA_ENGINE_DIR` (built-in
    /// default) must yield EXACTLY the `ACTIONS_DISPONIBLES` listing of test A
    /// (`PYTHONHASHSEED=0 python3 tests/tests_actions.py`, W1.5). Ignored by default:
    /// the campaign does not depend on Saya's machine — run it by hand:
    ///   SAYA_ENGINE_DIR="$HOME/Documents/GROK BUILD/generez" \
    ///     cargo test -p studio-core actions_converge_with_real_bank -- --ignored --nocapture
    #[test]
    #[ignore]
    fn actions_converge_with_real_bank() {
        let engine = crate::engine_runtime::engine_dir();
        let list = list_actions(&engine.join(ACTIONS_DATA)).expect("banque réelle lisible");
        let mut availables: Vec<&str> = list
            .iter()
            .filter(|a| a["disponible"] == json!(true))
            .filter_map(|a| a["name"].as_str())
            .collect();
        availables.sort_unstable();
        let hex = crate::fsutil::sha256_hex(availables.join("\n").as_bytes());
        println!(
            "ACTIONS_DISPONIBLES {} {hex} (banque : {} noms, moteur {})",
            availables.len(),
            list.len(),
            engine.display()
        );
        for nom in &availables {
            println!("{nom}");
        }
        // Reference fingerprint of test A (2026-10-26, 337-name bank).
        assert_eq!(
            hex, "7382dce6239d389f73bf6434ed8157dcda5ca1793d0f116a38c9cde29596c4f7",
            "le listing Rust diverge de la règle moteur (test A W1.5)"
        );
    }
}
