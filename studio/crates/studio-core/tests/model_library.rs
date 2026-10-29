use serde_json::json;
use std::path::PathBuf;
use studio_core::model_library as lib;

fn fixture() -> (PathBuf, Vec<(String, PathBuf)>) {
    let dir = std::env::temp_dir().join(format!("saya-models-{}", uuid::Uuid::new_v4()));
    let root = dir.join("loras");
    std::fs::create_dir_all(root.join("misc")).unwrap();
    std::fs::write(root.join("misc/hero.safetensors"), b"model-bytes").unwrap();
    std::fs::write(
        root.join("misc/hero.metadata.json"),
        serde_json::to_vec(&json!({"model_name":"Hero Costume","base_model":"Illustrious","tags":["anime","character"],
            "civitai":{"id":7,"modelId":3,"name":"v1.0","trainedWords":["hero outfit"]},"notes":"keep"})).unwrap(),
    )
    .unwrap();
    image::RgbImage::new(800, 1200)
        .save(root.join("misc/hero.jpeg"))
        .unwrap();
    std::fs::write(root.join("plain.safetensors"), b"other").unwrap();
    let roots = lib::roots(
        &json!({"loras":[root.to_string_lossy(), dir.join("absent").to_string_lossy()]}),
        "loras",
    )
    .unwrap();
    (dir, roots)
}

#[test]
fn library_reads_loramanager_sidecars_and_previews() {
    let (dir, roots) = fixture();
    assert_eq!(roots.len(), 1, "missing roots are ignored");
    let lib_json = lib::library(&roots).unwrap();
    assert_eq!(lib_json["count"], 2);
    let hero = lib_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["stem"] == "hero")
        .unwrap();
    assert_eq!(hero["name"], "Hero Costume");
    assert_eq!(hero["base"], "Illustrious");
    assert_eq!(hero["folder"], "misc");
    assert_eq!(hero["triggers"], json!(["hero outfit"]));
    assert_eq!(hero["versionName"], "v1.0");
    assert!(hero["preview"].as_str().unwrap().ends_with("hero.jpeg"));
    let thumb = lib::thumbnail(&roots, hero["preview"].as_str().unwrap()).unwrap();
    assert!(thumb.starts_with("data:image/jpeg;base64,"));
    let outside = dir.join("outside.png");
    image::RgbImage::new(2, 2).save(&outside).unwrap();
    assert!(lib::thumbnail(&roots, &outside.to_string_lossy()).is_err());
    lib::set_favorite(&roots, hero["path"].as_str().unwrap(), true).unwrap();
    let again = lib::library(&roots).unwrap();
    let hero = again["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["stem"] == "hero")
        .unwrap();
    assert_eq!(hero["favorite"], true);
    assert_eq!(hero["name"], "Hero Costume", "other sidecar fields kept");
}

#[test]
fn organize_moves_model_with_companions_and_undo_restores() {
    let (dir, roots) = fixture();
    let plan = lib::organize_plan(&roots).unwrap();
    assert_eq!(plan["skipped"], 1, "no metadata, no move");
    let moves = plan["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0]["toFolder"], "Illustrious/character");
    assert_eq!(moves[0]["conflict"], false);
    let pairs = vec![(
        moves[0]["from"].as_str().unwrap().to_owned(),
        moves[0]["to"].as_str().unwrap().to_owned(),
    )];
    let applied = lib::organize_apply(&roots, &pairs, &dir.join("journal")).unwrap();
    let target = roots[0].1.join("Illustrious/character");
    for name in ["hero.safetensors", "hero.metadata.json", "hero.jpeg"] {
        assert!(target.join(name).is_file(), "{name} moved");
    }
    assert!(!roots[0].1.join("misc/hero.safetensors").exists());
    assert!(
        lib::organize_plan(&roots).unwrap()["moves"]
            .as_array()
            .unwrap()
            .is_empty(),
        "already organized"
    );
    let undo = lib::organize_undo(
        &roots,
        std::path::Path::new(applied["journal"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(undo["restored"], 3);
    assert!(roots[0].1.join("misc/hero.jpeg").is_file());
    let escape = vec![(
        pairs[0].0.clone(),
        dir.join("elsewhere/hero.safetensors")
            .to_string_lossy()
            .to_string(),
    )];
    let refused = lib::organize_apply(&roots, &escape, &dir.join("journal")).unwrap();
    assert_eq!(refused["moved"], 0);
    assert_eq!(refused["errors"].as_array().unwrap().len(), 1);
}

#[test]
fn organize_undo_refuses_journal_leaving_the_roots() {
    let (dir, roots) = fixture();
    let root = &roots[0].1;
    let outside = dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), b"keep").unwrap();
    let journal = dir.join("crafted.json");
    let write = |moved: serde_json::Value| {
        std::fs::write(
            &journal,
            serde_json::to_vec(&json!({ "moved": moved })).unwrap(),
        )
        .unwrap()
    };
    // A legitimate pair first, then one whose source lives outside the root: nothing moves.
    write(json!([
        [
            root.join("misc/back.safetensors"),
            root.join("plain.safetensors")
        ],
        [
            outside.join("secret.txt"),
            root.join("misc/hero.safetensors")
        ],
    ]));
    assert!(lib::organize_undo(&roots, &journal).is_err());
    assert!(root.join("plain.safetensors").is_file(), "no partial apply");
    assert!(root.join("misc/hero.safetensors").is_file());
    // Destination outside the root (would pull an arbitrary file in).
    write(json!([[
        root.join("stolen.txt"),
        outside.join("secret.txt")
    ]]));
    assert!(lib::organize_undo(&roots, &journal).is_err());
    // Lexical escape through `..`.
    write(json!([[
        root.join("../outside/moved.txt"),
        root.join("plain.safetensors")
    ]]));
    assert!(lib::organize_undo(&roots, &journal).is_err());
    assert!(outside.join("secret.txt").is_file());
    assert!(!outside.join("moved.txt").exists());
    assert!(root.join("plain.safetensors").is_file());
    // Symlinked folder inside the root that points outside is resolved and refused.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        write(json!([[
            root.join("link/moved.txt"),
            root.join("plain.safetensors")
        ]]));
        assert!(lib::organize_undo(&roots, &journal).is_err());
        assert!(!outside.join("moved.txt").exists());
    }
}

#[test]
#[ignore = "network: Civitai by-hash lookup through curl"]
fn civitai_by_hash_reachable_over_https() {
    let (code, body) = lib::https_get(
        "https://civitai.red/api/v1/model-versions/by-hash/a82263f9b2e38876359f4bb93c6975ca8f33ddb30204622359a6a43403570c32",
        "",
        16 * 1024 * 1024,
    )
    .unwrap();
    assert_eq!(
        code,
        200,
        "{}",
        String::from_utf8_lossy(&body[..body.len().min(200)])
    );
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["modelId"].as_u64().is_some());
}

#[test]
fn finished_download_is_filed_under_base_and_category() {
    let (dir, roots) = fixture();
    let downloads = dir.join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let file = downloads.join("new-style.safetensors");
    std::fs::write(&file, b"downloaded").unwrap();
    let target =
        lib::import_download(&roots, &file, "Pony", &["style".into(), "anime".into()]).unwrap();
    assert_eq!(target, roots[0].1.join("Pony/style/new-style.safetensors"));
    assert!(target.is_file() && !file.exists());
    std::fs::write(&file, b"again").unwrap();
    assert!(
        lib::import_download(&roots, &file, "Pony", &["style".into()]).is_err(),
        "never overwrites"
    );
}

#[test]
#[ignore = "network: fetch Civitai metadata for a sandbox copy of a real LoRA"]
fn fetch_metadata_writes_loramanager_sidecar_and_preview() {
    let Some(source) = std::env::var_os("SAYA_REAL_LORA_FILE").map(std::path::PathBuf::from) else {
        eprintln!("skip: régler SAYA_REAL_LORA_FILE (un .safetensors local à copier en bac à sable)");
        return;
    };
    let source = source.as_path();
    let dir = std::env::temp_dir().join(format!("saya-fetch-{}", uuid::Uuid::new_v4()));
    let root = dir.join("loras");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::copy(source, root.join("sample.safetensors")).unwrap();
    let roots = lib::roots(&json!({"loras":[root.to_string_lossy()]}), "loras").unwrap();
    let path = roots[0].1.join("sample.safetensors");
    let r = lib::fetch_metadata(&roots, &path.to_string_lossy(), "").unwrap();
    assert_eq!(r["found"], true, "{r}");
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(roots[0].1.join("sample.metadata.json")).unwrap())
            .unwrap();
    assert!(!m["model_name"].as_str().unwrap().is_empty());
    assert!(!m["base_model"].as_str().unwrap().is_empty());
    assert_eq!(m["from_civitai"], true);
    let item = &lib::library(&roots).unwrap()["items"][0];
    assert!(
        item["preview"].as_str().is_some(),
        "preview downloaded beside the model"
    );
    assert!(lib::thumbnail(&roots, item["preview"].as_str().unwrap()).is_ok());
}
