use studio_core::{gallery::Filters, Database};
#[test]
fn neighbors_follow_the_same_filters_and_sort_and_fallback_after_removal() {
    let dir = std::env::temp_dir().join(format!("saya-nav-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for name in ["a.png", "b.png", "c.png", "other.png"] {
        image::RgbImage::new(2, 2).save(dir.join(name)).unwrap();
    }
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let all = db.gallery(&Filters::default()).unwrap();
    for asset in all.items.iter().take(3) {
        db.annotate_asset(&asset.id, true, "kept", "accepted")
            .unwrap();
    }
    let mut f = Filters {
        favorite: true,
        review: "accepted".into(),
        sort: "name_desc".into(),
        offset: 48,
        ..Default::default()
    };
    let b = &all.items[1].id;
    let n = db.gallery_neighbors(&f, b, 0).unwrap();
    assert_eq!(n["total"], 3);
    assert_eq!(n["position"], 1);
    assert_eq!(n["current"]["id"], *b);
    assert_eq!(n["previous"]["id"], all.items[2].id);
    assert_eq!(n["next"]["id"], all.items[0].id);
    assert!(db.gallery_neighbors(&f, &all.items[2].id, 0).unwrap()["previous"].is_null());
    assert!(db.gallery_neighbors(&f, &all.items[0].id, 0).unwrap()["next"].is_null());
    db.set_catalog_removed(std::slice::from_ref(b), true)
        .unwrap();
    let n = db.gallery_neighbors(&f, b, 1).unwrap();
    assert_eq!(n["total"], 2);
    assert_eq!(n["current"]["id"], all.items[0].id);
    assert_eq!(n["found"], false);
    f.search = "no such image".into();
    let n = db.gallery_neighbors(&f, b, 500).unwrap();
    assert_eq!(n["total"], 0);
    assert!(n["current"].is_null());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn checkpoint_selection_is_a_union_before_lora_filtering() {
    let dir = std::env::temp_dir().join(format!("saya-union-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for name in ["gallery-a.png", "gallery-b.png"] {
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
            dir.join(name),
        )
        .unwrap();
    }
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let all = db.gallery(&Filters::default()).unwrap();
    assert_eq!(all.total, 2);
    let union = db
        .gallery(&Filters {
            checkpoints: vec!["A.safetensors".into(), "B.safetensors".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(union.total, 2);
    let one = db
        .gallery(&Filters {
            checkpoints: vec!["A.safetensors".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(one.total, 1);
    let lora = db
        .gallery(&Filters {
            checkpoints: vec!["A.safetensors".into(), "B.safetensors".into()],
            loras: vec!["L1".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(lora.total, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_reader_sort_lists_and_navigates_the_same_assets() {
    let dir = std::env::temp_dir().join(format!("saya-sorts-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for (name, size) in [("small.png", 2), ("large.png", 8), ("mid.png", 4)] {
        image::RgbImage::new(size, size)
            .save(dir.join(name))
            .unwrap();
    }
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let all = db.gallery(&Filters::default()).unwrap();
    let large = all
        .items
        .iter()
        .find(|a| a.path.ends_with("large.png"))
        .unwrap()
        .id
        .clone();
    let mid = all
        .items
        .iter()
        .find(|a| a.path.ends_with("mid.png"))
        .unwrap()
        .id
        .clone();
    db.annotate_asset(&mid, true, "", "accepted").unwrap();
    for sort in [
        "favorites",
        "accepted",
        "resolution",
        "filesize",
        "character",
        "shuffle",
    ] {
        let f = Filters {
            sort: sort.into(),
            ..Default::default()
        };
        let page = db.gallery(&f).unwrap();
        assert_eq!(page.total, 3, "{sort}");
        let n = db.gallery_neighbors(&f, &page.items[1].id, 0).unwrap();
        assert_eq!(n["position"], 1, "{sort}");
        if sort == "resolution" {
            assert_eq!(page.items[0].id, large);
        }
        if sort == "favorites" || sort == "accepted" {
            assert_eq!(page.items[0].id, mid, "{sort}");
        }
    }
    assert!(db
        .gallery(&Filters {
            sort: "bogus".into(),
            ..Default::default()
        })
        .is_err());
}

#[test]
fn missing_files_disappear_without_reindex_and_keep_catalog_data() {
    let dir = std::env::temp_dir().join(format!("saya-missing-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for name in ["a.png", "b.png", "c.png"] {
        image::RgbImage::new(2, 2).save(dir.join(name)).unwrap();
    }
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let filters = Filters {
        sort: "name_asc".into(),
        ..Default::default()
    };
    let all = db.gallery(&filters).unwrap();
    let missing = &all.items[0].id;
    db.annotate_asset(missing, true, "preserved note", "accepted")
        .unwrap();
    std::fs::remove_file(dir.join("a.png")).unwrap();
    let page = db.gallery_window(&filters, 1).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].id, all.items[1].id);
    let neighbors = db.gallery_neighbors(&filters, missing, 0).unwrap();
    assert_eq!(neighbors["total"], 2);
    assert_eq!(neighbors["found"], false);
    let absent = db
        .gallery(&Filters {
            availability: "missing".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(absent.total, 1);
    assert_eq!(absent.items[0].id, *missing);
    assert_eq!(absent.items[0].note, "preserved note");
    assert!(absent.items[0].favorite);
    std::fs::remove_dir_all(dir).unwrap();
}
