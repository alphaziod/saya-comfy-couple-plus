use rusqlite::Connection;
use serde_json::{json, Value};
use studio_core::{gallery::Filters, Database};

/// Synthetic library on a temporary database (never the user's database).
fn synthetic_db(tag: &str) -> (Database, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "saya-character-gallery-{tag}-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let raw = Connection::open(&path).unwrap();
    for sql in [
        include_str!("../../../migrations/0001_initial.sql"),
        include_str!("../../../migrations/0002_functional.sql"),
        include_str!("../../../migrations/0003_gallery.sql"),
        include_str!("../../../migrations/0004_character_history.sql"),
        include_str!("../../../migrations/0005_prompt_proposals.sql"),
        include_str!("../../../migrations/0006_generation_provenance.sql"),
        include_str!("../../../migrations/0007_character_references.sql"),
        include_str!("../../../migrations/0008_reference_history.sql"),
        include_str!("../../../migrations/0009_index_runs.sql"),
    ] {
        raw.execute_batch(sql).unwrap();
    }
    raw.pragma_update(None, "user_version", 9).unwrap();
    raw.execute_batch(
        "INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Synthetic','/synthetic','linux');
         INSERT INTO characters(id,name,identity_json) VALUES
           ('a','Alpha','{\"identity_prompt\":\"alpha identity, full P1\",\"anatomy_prompt\":\"alpha anatomy, detailed P1\",\"description\":\"desc\"}'),
           ('b','Beta','{\"identity_prompt\":\"beta identity\"}'),
           ('c','Gamma','{}');
         INSERT INTO character_variants(id,character_id,name,prompt_blocks_json) VALUES
           ('a1','a','Night','{\"P1\":\"alpha P1 night\",\"BG\":\"rooftop at night\",\"ACTION\":\"running\"}'),
           ('a2','a','Day','{\"P1\":\"alpha P1 day\"}'),
           ('b1','b','Only','{\"P1\":\"beta P1 only\",\"BG\":\"cafe interior\"}');",
    )
    .unwrap();
    drop(raw);
    (Database::open(&path).unwrap(), path)
}

fn insert_asset(raw: &Connection, id: &str, path: &str, modified: &str, extra: &str) {
    raw.execute(
        "INSERT INTO assets(id,library_id,relative_path,availability,modified_at,favorite,review) VALUES(?1,'lib',?2,'present',?3,?4,?5)",
        rusqlite::params![
            id,
            path,
            modified,
            extra.contains("favorite"),
            if extra.contains("rejected") {
                "rejected"
            } else if extra.contains("accepted") {
                "accepted"
            } else {
                "unreviewed"
            }
        ],
    )
    .unwrap();
}

fn link(raw: &Connection, asset: &str, character: &str, role: &str) {
    raw.execute(
        "INSERT INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,?3,'human') ON CONFLICT DO NOTHING",
        rusqlite::params![asset, character, role],
    )
    .unwrap();
}

#[test]
fn cards_rank_portrait_counts_and_single_variant_without_inventing_scores() {
    let (db, path) = synthetic_db("cards");
    let raw = Connection::open(path.as_path()).unwrap();
    // Alpha: primary reference wins over other references and depicted images.
    insert_asset(&raw, "img-dep", "dep.png", "20", "");
    insert_asset(&raw, "img-ref2", "ref2.png", "19", "");
    insert_asset(&raw, "img-primary", "ref1.png", "18", "");
    link(&raw, "img-dep", "a", "depicted");
    link(&raw, "img-ref2", "a", "reference");
    link(&raw, "img-primary", "a", "reference");
    drop(raw);
    // The reference links already created ordinary ordered references;
    // the human API marks the primary one.
    db.primary_reference("a", None, "img-primary").unwrap();
    // Beta has no reference: falls back to its only depicted image.
    let raw = Connection::open(path.as_path()).unwrap();
    insert_asset(&raw, "img-beta", "beta.png", "17", "");
    link(&raw, "img-beta", "b", "depicted");
    drop(raw);
    // Gamma has no image at all.
    let cards = db.character_cards().unwrap();
    assert_eq!(cards.len(), 3);
    let alpha = cards.iter().find(|c| c["id"] == "a").unwrap();
    assert_eq!(alpha["portrait"], json!("img-primary"));
    assert_eq!(alpha["imageCount"], json!(3));
    assert_eq!(alpha["variantCount"], json!(2));
    // Two variants: no main variant is claimed, nothing is invented.
    assert_eq!(alpha["mainVariant"], Value::Null);
    let beta = cards.iter().find(|c| c["id"] == "b").unwrap();
    assert_eq!(beta["portrait"], json!("img-beta"));
    assert_eq!(beta["mainVariant"], json!("Only"));
    let gamma = cards.iter().find(|c| c["id"] == "c").unwrap();
    assert_eq!(gamma["portrait"], Value::Null);
    assert_eq!(gamma["imageCount"], json!(0));
}

#[test]
fn sheet_exposes_identity_anatomy_backgrounds_and_reference_order() {
    let (db, path) = synthetic_db("sheet");
    let raw = Connection::open(path.as_path()).unwrap();
    insert_asset(&raw, "ref-a", "a.png", "20", "");
    insert_asset(&raw, "ref-b", "b.png", "19", "");
    link(&raw, "ref-a", "a", "reference");
    drop(raw);
    db.add_reference("a", Some("a1"), "ref-b").unwrap();
    db.primary_reference("a", None, "ref-a").unwrap();
    db.primary_reference("a", Some("a1"), "ref-b").unwrap();
    let sheet = db.character_sheet("a").unwrap();
    assert_eq!(sheet["name"], json!("Alpha"));
    assert_eq!(sheet["description"], "desc");
    assert_eq!(sheet["traits"], "");
    assert_eq!(sheet["identityPrompt"], json!("alpha identity, full P1"));
    assert_eq!(sheet["anatomyPrompt"], json!("alpha anatomy, detailed P1"));
    assert_eq!(sheet["portrait"], json!("ref-a"));
    assert_eq!(
        sheet["backgrounds"].as_array().unwrap(),
        &vec![json!({"variantId":"a1","variantName":"Night","bg":"rooftop at night"}),]
    );
    assert_eq!(sheet["variants"].as_array().unwrap().len(), 2);
    let references = sheet["references"].as_array().unwrap();
    assert_eq!(references.len(), 2);
    assert_eq!(references[0]["assetId"], json!("ref-a"));
    assert_eq!(references[0]["primary"], json!(true));
    assert_eq!(references[1]["assetId"], json!("ref-b"));
    assert_eq!(references[1]["variantName"], json!("Night"));
    assert_eq!(sheet["imageCount"], json!(2));
    // Beta: anatomy prompt absent stays empty, no fabricated text.
    let beta = db.character_sheet("b").unwrap();
    assert_eq!(beta["anatomyPrompt"], json!(""));
    assert_eq!(beta["backgrounds"].as_array().unwrap().len(), 1);
    assert!(db.character_sheet("missing").is_err());
}

#[test]
fn top_images_follow_favorites_then_reviews_then_recency_with_limit_ten() {
    let (db, path) = synthetic_db("top");
    let raw = Connection::open(path.as_path()).unwrap();
    // Expected order: favorites first (any review state), then review state
    // (accepted, unreviewed, rejected), then recency (higher stamp is newer,
    // stamps share one length so string order is chronological); maximum 10.
    insert_asset(&raw, "r1", "r1.png", "10", "rejected");
    insert_asset(&raw, "u1", "u1.png", "20", "");
    insert_asset(&raw, "a1", "a1.png", "30", "accepted");
    insert_asset(&raw, "fa", "fa.png", "40", "favorite accepted");
    insert_asset(&raw, "r2", "r2.png", "50", "favorite rejected");
    insert_asset(&raw, "a2", "a2.png", "29", "accepted");
    insert_asset(&raw, "fu", "fu.png", "35", "favorite");
    for id in ["r1", "u1", "a1", "fa", "r2", "a2", "fu"] {
        link(&raw, id, "a", "depicted");
    }
    for i in 0..6 {
        let id = format!("pad{i}");
        insert_asset(&raw, &id, &format!("pad{i}.png"), &format!("{i:02}"), "");
        link(&raw, &id, "a", "depicted");
    }
    drop(raw);
    let top = db.character_top_images("a").unwrap();
    assert_eq!(top.len(), 10);
    let ids: Vec<&str> = top.iter().filter_map(|v| v["id"].as_str()).collect();
    assert_eq!(
        ids,
        vec!["fa", "fu", "r2", "a1", "a2", "u1", "pad5", "pad4", "pad3", "pad2"]
    );
    assert_eq!(top[0]["favorite"], json!(true));
    assert_eq!(top[0]["review"], json!("accepted"));
    // A favorite rejected image outranks a non-favorite accepted one.
    assert_eq!(top[2]["review"], json!("rejected"));
    assert_eq!(top[3]["review"], json!("accepted"));
}

#[test]
fn windowed_loading_matches_pagination_and_keeps_filters() {
    let (db, path) = synthetic_db("window");
    let raw = Connection::open(path.as_path()).unwrap();
    let root = path.with_extension("images");
    std::fs::create_dir_all(&root).unwrap();
    raw.execute(
        "UPDATE libraries SET root_path=? WHERE id='lib'",
        [root.to_str().unwrap()],
    )
    .unwrap();
    for i in 0..120u32 {
        std::fs::write(root.join(format!("w{i:03}.png")), b"fixture").unwrap();
        let id = format!("w{i:03}");
        insert_asset(
            &raw,
            &id,
            &format!("w{i:03}.png"),
            &format!("{i:019}"),
            if i % 3 == 0 { "favorite" } else { "" },
        );
        if i % 2 == 0 {
            link(&raw, &id, "a", "depicted");
        }
    }
    raw.execute(
        "UPDATE assets SET absolute_path=? || '/' || relative_path",
        [root.to_str().unwrap()],
    )
    .unwrap();
    drop(raw);
    let mut filters = Filters {
        sort: "name_asc".into(),
        ..Default::default()
    };
    // Window with the historical page size reproduces `gallery` exactly.
    let page = db.gallery(&filters).unwrap();
    let window = db.gallery_window(&filters, 48).unwrap();
    assert_eq!(page.total, 120);
    assert_eq!(window.total, page.total);
    assert_eq!(
        serde_json::to_value(&page.items).unwrap(),
        serde_json::to_value(&window.items).unwrap()
    );
    // Concatenated windows cover the whole result exactly once.
    let mut seen = Vec::new();
    loop {
        filters.offset = seen.len() as u32;
        let window = db.gallery_window(&filters, 50).unwrap();
        let done = window.items.is_empty();
        for asset in window.items {
            seen.push(asset.id);
        }
        if done {
            break;
        }
    }
    assert_eq!(seen.len(), 120);
    assert_eq!(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        120
    );
    // Filters apply inside windows; the character filter keeps linked assets only.
    filters.characters = vec!["a".into()];
    filters.offset = 0;
    let window = db.gallery_window(&filters, 250).unwrap();
    assert_eq!(window.total, 60);
    assert_eq!(window.items.len(), 60);
    assert!(window
        .items
        .iter()
        .all(|a| a.id.strip_prefix('w').unwrap().parse::<u32>().unwrap() % 2 == 0));
    // Invalid limits are refused before any query.
    assert!(db.gallery_window(&filters, 0).is_err());
    assert!(db.gallery_window(&filters, 251).is_err());
    // Newest ordering stays stable across window boundaries.
    let mut newest = Filters {
        sort: "newest".into(),
        ..Default::default()
    };
    let first = db.gallery_window(&newest, 100).unwrap();
    newest.offset = 100;
    let second = db.gallery_window(&newest, 100).unwrap();
    assert_eq!(first.items.len(), 100);
    assert_eq!(second.items.len(), 20);
    assert_eq!(first.items[0].id, "w119");
    assert_eq!(second.items[0].id, "w019");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sheet_survives_reopen_and_archived_characters_stay_viewable() {
    let (db, path) = synthetic_db("reopen");
    {
        let raw = Connection::open(&path).unwrap();
        raw.execute_batch(
            "INSERT INTO assets(id,library_id,relative_path,availability) VALUES('img-a','lib','a.png','present');
             INSERT INTO asset_characters VALUES('img-a','a','depicted','human');
             UPDATE characters SET archived=1 WHERE id='a';",
        )
        .unwrap();
    }
    let sheet = db.character_sheet("a").unwrap();
    assert_eq!(sheet["archived"], json!(true));
    assert_eq!(sheet["identityPrompt"], json!("alpha identity, full P1"));
    assert_eq!(sheet["portrait"], json!("img-a"));
    // Archived characters do not appear as cards.
    assert!(db.character_cards().unwrap().iter().all(|c| c["id"] != "a"));
    drop(db);
    // Reopening keeps the sheet readable.
    let db = Database::open(&path).unwrap();
    assert_eq!(db.character_sheet("a").unwrap()["portrait"], json!("img-a"));
    std::fs::remove_file(&path).ok();
}

#[test]
fn linked_and_visible_counts_keep_roles_missing_and_disabled_libraries_distinct() {
    let (db, path) = synthetic_db("visibility");
    let raw = Connection::open(&path).unwrap();
    for id in ["shown", "missing", "disabled"] {
        insert_asset(&raw, id, &format!("{id}.png"), "20", "");
        link(&raw, id, "a", "depicted");
    }
    link(&raw, "shown", "a", "reference");
    raw.execute(
        "UPDATE assets SET availability='missing' WHERE id='missing'",
        [],
    )
    .unwrap();
    raw.execute("INSERT INTO libraries(id,label,root_path,platform,enabled) VALUES('off','disabled','/off','linux',0)",[]).unwrap();
    raw.execute("UPDATE assets SET library_id='off' WHERE id='disabled'", [])
        .unwrap();
    let cards = db.character_cards().unwrap();
    assert_eq!(cards[0]["imageCount"], 3);
    assert_eq!(cards[0]["visibleImageCount"], 1);
    let sheet = db.character_sheet("a").unwrap();
    assert_eq!(sheet["imageCount"], 3);
    assert_eq!(sheet["visibleImageCount"], 1);
    assert_eq!(sheet["topImages"].as_array().unwrap().len(), 1);
    assert_eq!(sheet["topImages"][0]["id"], "shown");
}

#[test]
fn gallery_view_preference_accepts_only_known_modes_and_persists() {
    let db = Database::in_memory().unwrap();
    db.set_preference("galleryView", json!("pages")).unwrap();
    assert_eq!(db.preferences().unwrap()["galleryView"], json!("pages"));
    db.set_preference("galleryView", json!("stream")).unwrap();
    assert_eq!(db.preferences().unwrap()["galleryView"], json!("stream"));
    for bad in [json!("grid"), json!(true), json!(null)] {
        assert!(db.set_preference("galleryView", bad).is_err());
    }
    assert_eq!(db.preferences().unwrap()["galleryView"], json!("stream"));
}
