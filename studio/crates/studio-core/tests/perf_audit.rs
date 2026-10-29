//! G5 bench (brief point 7): slow paths flagged, measured at the real
//! scale of Saya's database (6107 assets, 24151 model links, average
//! metadata ~640 KB, snapshots ~180 KB, real object_info ~15.9 MB).
//! Synthetic only: no user file read, no GPU, no
//! ComfyUI launched. Ignored by default.
//! Run: cargo test -p studio-core --release --test perf_audit -- --ignored --nocapture
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;
use studio_core::comfy::model_choices;
use studio_core::gallery::{image_data, metadata, Filters};
use studio_core::workflow::api_graph;
use studio_core::Database;

/// Median of N measurements after warm-up (the first measurement pays for the
/// system's cold caches, not the measured code).
fn median_ms<R>(warmup: impl FnOnce(), mut run: impl FnMut() -> R, samples: usize) -> f64 {
    warmup();
    let mut times = Vec::new();
    for _ in 0..samples {
        let start = Instant::now();
        run();
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (times[times.len() / 2] * 1000.0).round() / 1000.0
}

fn crc32(kind: &[u8], body: &[u8]) -> u32 {
    fn table() -> &'static [u32; 256] {
        use std::sync::OnceLock;
        static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
        TABLE.get_or_init(|| {
            let mut table = [0u32; 256];
            for (i, entry) in table.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xEDB8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
                *entry = c;
            }
            table
        })
    }
    let mut crc = 0xFFFF_FFFFu32;
    for byte in kind.iter().chain(body.iter()) {
        crc = table()[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut raw = (body.len() as u32).to_be_bytes().to_vec();
    raw.extend_from_slice(kind);
    raw.extend_from_slice(body);
    raw.extend_from_slice(&crc32(kind, body).to_be_bytes());
    raw
}

/// 1216×1632 PNG (ComfyUI portrait output): filtered scanlines, stored zlib
/// blocks (the decoder pays for filters and conversion, not inflate),
/// `prompt` tEXt chunk embedded like a real ComfyUI output.
fn write_realistic_png(path: &Path, width: u32, height: u32, prompt: &str) {
    use std::io::Write;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut raw = Vec::with_capacity((width * 3 + 1) as usize * height as usize);
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            let r = (x * 255 / width) as u8;
            let g = (y * 255 / height) as u8;
            let b = ((x / 8 + y / 8) % 2 * 255) as u8;
            raw.extend_from_slice(&[r, g, b]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut offset = 0;
    while offset < raw.len() {
        let take = (raw.len() - offset).min(65_535);
        let last = offset + take == raw.len();
        z.push(if last { 1 } else { 0 });
        z.extend_from_slice(&(take as u16).to_le_bytes());
        z.extend_from_slice(&(!(take as u16)).to_le_bytes());
        z.extend_from_slice(&raw[offset..offset + take]);
        offset += take;
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(b"\x89PNG\r\n\x1a\n").unwrap();
    file.write_all(&chunk(b"IHDR", &ihdr)).unwrap();
    file.write_all(&chunk(
        b"tEXt",
        [b"prompt\0", prompt.as_bytes()].concat().as_slice(),
    ))
    .unwrap();
    file.write_all(&chunk(b"IDAT", &z)).unwrap();
    file.write_all(&chunk(b"IEND", b"")).unwrap();
}

/// Synthetic ComfyUI API graph; `chars` tunes the serialized size.
fn synthetic_graph(nodes: usize, chars: usize) -> Value {
    let text = "masterpiece, best quality, very aesthetic, absurdres, ".repeat(chars / 55 + 1);
    let mut graph = serde_json::Map::new();
    for i in 0..nodes {
        let id = (i + 1).to_string();
        let class = match i % 6 {
            0 => "CLIPTextEncode",
            1 => "KSampler",
            2 => "LoraLoader",
            3 => "CheckpointLoaderSimple",
            4 => "VAEDecode",
            _ => "SaveImage",
        };
        let inputs = match i % 6 {
            0 => json!({"text": text, "clip": [((i + 30) % nodes + 1).to_string(), 0]}),
            1 => {
                json!({"seed": i as u64, "steps": 28, "cfg": 5.5, "model": [((i + 30) % nodes + 1).to_string(), 0]})
            }
            2 => {
                json!({"lora_name": format!("lora_{i}.safetensors"), "strength_model": 0.8, "strength_clip": 0.9})
            }
            3 => json!({"ckpt_name": format!("ckpt_{}.safetensors", i % 7)}),
            4 => {
                json!({"samples": [((i + 30) % nodes + 1).to_string(), 0], "vae": [((i + 30) % nodes + 1).to_string(), 0]})
            }
            _ => {
                json!({"images": [((i + 30) % nodes + 1).to_string(), 0], "filename_prefix": "img"})
            }
        };
        graph.insert(
            id,
            json!({"class_type": class, "inputs": inputs, "_meta": {"title": format!("Node {i}")}}),
        );
    }
    Value::Object(graph)
}

/// Graph calibrated to a target size (real metadata ~640 KB,
/// real snapshot ~180 KB): the JSON structure varies, the parse/scan cost
/// depends on bytes, not on structure.
fn graph_of_size(nodes: usize, target_bytes: usize) -> String {
    let mut chars = 200usize;
    loop {
        let value = synthetic_graph(nodes, chars);
        let size = value.to_string().len();
        if size >= target_bytes || chars > 400_000 {
            return value.to_string();
        }
        chars = (chars * target_bytes / size.max(1)).max(chars + 200);
    }
}

struct Fixture {
    dir: PathBuf,
    db: Database,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn build_fixture() -> Fixture {
    let dir = std::env::temp_dir().join(format!("saya-perf-audit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    drop(Database::open(&path).unwrap());
    let raw = rusqlite::Connection::open(&path).unwrap();
    let images = dir.join("images");
    std::fs::create_dir_all(&images).unwrap();
    raw.execute(
        "INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Perf',?1,'linux')",
        [&images.to_string_lossy()],
    )
    .unwrap();

    // Real scale measured read-only on Saya's database:
    // 6107 assets · 24151 asset_models · 170 model_resources · 139 characters
    // · 83 links · average metadata 640 KB · average snapshot 180 KB.
    let heavy_metadata = graph_of_size(220, 640_000);
    let snapshot = {
        let graph: Value = serde_json::from_str(&graph_of_size(220, 170_000)).unwrap();
        json!({"workflowId": "perf-wf", "prompt": graph}).to_string()
    };
    println!(
        "FIXTURE metadata_bytes={} snapshot_bytes={}",
        heavy_metadata.len(),
        snapshot.len()
    );
    let mut tx = raw.unchecked_transaction().unwrap();
    for i in 0..6107u32 {
        // 24 heavy assets (real ~640 KB metadata); the rest is light.
        let heavy = i < 24;
        let name = if heavy {
            format!("heavy{i:03}.png")
        } else {
            format!("set{:02}/img{i:05}.png", i % 100)
        };
        std::fs::create_dir_all(images.join(&name).parent().unwrap()).unwrap();
        std::fs::write(images.join(&name), b"png").unwrap();
        tx.execute(
            "INSERT INTO assets(id,library_id,relative_path,absolute_path,availability,metadata_json,metadata_state,width,height,content_sha256,favorite) VALUES(?1,'lib',?2,?3,'present',?4,'read',1216,1632,?5,?6)",
            rusqlite::params![
                format!("a{i:06}"),
                name,
                images.join(&name).to_string_lossy(),
                if heavy { heavy_metadata.as_str() } else { "{}" },
                format!("{:064x}", i),
                i % 10 == 0,
            ],
        )
        .unwrap();
    }
    for m in 0..170u32 {
        let kind = if m < 20 { "checkpoint" } else { "lora" };
        tx.execute(
            "INSERT INTO model_resources(id,kind,name) VALUES(?1,?2,?3)",
            rusqlite::params![format!("m{m:03}"), kind, format!("model_{m}.safetensors")],
        )
        .unwrap();
    }
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..24_151u32 {
        let asset = format!("a{:06}", next() % 6107);
        let model = format!("m{:03}", next() % 170);
        tx.execute(
            "INSERT OR IGNORE INTO asset_models(asset_id,model_id,evidence) VALUES(?1,?2,'embedded')",
            rusqlite::params![asset, model],
        )
        .unwrap();
    }
    for c in 0..139u32 {
        tx.execute(
            "INSERT INTO characters(id,name,identity_json) VALUES(?1,?2,'{}')",
            rusqlite::params![format!("c{c:03}"), format!("Perso {c}")],
        )
        .unwrap();
    }
    for i in 0..83u32 {
        tx.execute(
            "INSERT INTO asset_characters(asset_id,character_id,role,evidence) VALUES(?1,?2,'depicted','sidecar')",
            rusqlite::params![format!("a{:06}", i), format!("c{:03}", i % 139)],
        )
        .unwrap();
    }
    tx.execute(
        "INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('perf-wf','Perf','h','comfy_api','{}')",
        [],
    )
    .unwrap();
    for j in 0..8u32 {
        let job = format!("perf-job-{j}");
        tx.execute(
            "INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state) VALUES(?1,'perf-wf',?3,?2,'succeeded')",
            rusqlite::params![job, snapshot, (42_000_000 + j).to_string()],
        )
        .unwrap();
        for a in 0..3u32 {
            tx.execute(
                "INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES(?1,?2,'9',0,'saved')",
                rusqlite::params![job, format!("a{:06}", j * 3 + a)],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    drop(raw);
    let db = Database::open(&path).unwrap();
    Fixture { dir, db }
}

#[test]
#[ignore = "G5 perf audit: measured paths at Saya's real scale"]
fn perf_audit_reported_paths() {
    let mut report = serde_json::Map::new();
    let object_info_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-results/gpu-real/object_info.json");

    // ---- Gallery SQL (real scale 6107) ----
    {
        let fixture = build_fixture();
        let f = Filters::default();
        let mut total_seen = 0u32;
        report.insert(
            "gallery_window_page1_ms".into(),
            json!(median_ms(
                || {},
                || {
                    let page = fixture.db.gallery_window(&f, 48).unwrap();
                    total_seen = page.total;
                },
                7
            )),
        );
        println!("FIXTURE total_present={total_seen}");
        let deep = Filters {
            offset: 6048,
            ..Default::default()
        };
        let mut items_len = 0;
        let mut deep_total = 0u32;
        report.insert(
            "gallery_window_deep_ms".into(),
            json!(median_ms(
                || {},
                || {
                    let page = fixture.db.gallery_window(&deep, 48).unwrap();
                    items_len = page.items.len();
                    deep_total = page.total;
                },
                7
            )),
        );
        println!("FIXTURE deep_total={deep_total} deep_items={items_len}");
        assert_eq!(items_len, 48, "la limite borne la dernière fenêtre");
        let anchor = format!("a{:06}", 6106);
        report.insert(
            "gallery_neighbors_ms".into(),
            json!(median_ms(
                || {},
                || fixture.db.gallery_neighbors(&deep, &anchor, 6106).unwrap(),
                7
            )),
        );
        report.insert(
            "facets_ms".into(),
            json!(median_ms(|| {}, || fixture.db.facets().unwrap(), 7)),
        );
        let mut detail_bytes = 0;
        report.insert(
            "asset_detail_heavy_ms".into(),
            json!(median_ms(
                || {},
                || {
                    let detail = fixture.db.asset_detail("a000000").unwrap();
                    detail_bytes = detail.to_string().len();
                },
                7
            )),
        );
        report.insert("asset_detail_heavy_bytes".into(), json!(detail_bytes));
        // Some paths open their own connection per action.
        let path = fixture.dir.join("studio.sqlite3");
        report.insert(
            "database_open_ms".into(),
            json!(median_ms(
                || {},
                || {
                    drop(Database::open(&path).unwrap());
                },
                7
            )),
        );
    }

    // ---- Images: decoding and thumbnails (realistic 1216×1632 PNG) ----
    {
        let dir = std::env::temp_dir().join(format!("saya-perf-img-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let prompt = graph_of_size(220, 640_000);
        let image = dir.join("real.png");
        write_realistic_png(&image, 1216, 1632, &prompt);
        report.insert(
            "png_bytes".into(),
            json!(std::fs::metadata(&image).unwrap().len()),
        );
        report.insert(
            "metadata_parse_ms".into(),
            json!(median_ms(|| {}, || metadata(&image).unwrap(), 5)),
        );
        let cache = dir.join("thumbs");
        report.insert(
            "thumbnail_cold_ms".into(),
            json!(median_ms(
                || {},
                || image_data(&image, &cache, false).unwrap(),
                1
            )),
        );
        report.insert(
            "thumbnail_warm_ms".into(),
            json!(median_ms(
                || {},
                || image_data(&image, &cache, false).unwrap(),
                9
            )),
        );
        let cache2 = dir.join("previews");
        report.insert(
            "preview_cold_ms".into(),
            json!(median_ms(
                || {},
                || image_data(&image, &cache2, true).unwrap(),
                1
            )),
        );
        report.insert(
            "preview_warm_ms".into(),
            json!(median_ms(
                || {},
                || image_data(&image, &cache2, true).unwrap(),
                9
            )),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Parsing workflow + object_info ----
    {
        let snapshot = {
            let graph: Value = serde_json::from_str(&graph_of_size(220, 170_000)).unwrap();
            json!({"workflowId": "perf-wf", "prompt": graph}).to_string()
        };
        report.insert("workflow_snapshot_bytes".into(), json!(snapshot.len()));
        report.insert(
            "api_graph_parse_ms".into(),
            json!(median_ms(|| {}, || api_graph(&snapshot).unwrap(), 9)),
        );
        let graph = api_graph(&snapshot).unwrap();
        report.insert(
            "resources_scan_ms".into(),
            json!(median_ms(
                || {},
                || studio_core::gallery::resources(&graph).1.len(),
                9
            )),
        );
        if object_info_path.exists() {
            let text = std::fs::read_to_string(&object_info_path).unwrap();
            let info: Value = serde_json::from_str(&text).unwrap();
            report.insert("object_info_bytes".into(), json!(text.len()));
            report.insert(
                "object_info_json_parse_ms".into(),
                json!(median_ms(
                    || {},
                    || {
                        serde_json::from_str::<Value>(
                            &std::fs::read_to_string(&object_info_path).unwrap(),
                        )
                        .unwrap()
                    },
                    3
                )),
            );
            report.insert(
                "object_info_model_choices_ms".into(),
                json!(median_ms(|| {}, || model_choices(&info).unwrap(), 5)),
            );
        } else {
            report.insert(
                "object_info_skipped".into(),
                json!(format!("absent: {}", object_info_path.display())),
            );
        }
    }

    println!("PERF_AUDIT {}", Value::Object(report));
}
