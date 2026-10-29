//! Memory measurement and engine release between passes.
//!
//! Distinguishes what can be told apart on Linux/ROCm without assuming CUDA
//! semantics: Saya Studio's own RSS, the ComfyUI process RSS (found through the
//! socket listening on its port), the engine's own device view
//! (`/system_stats`: device VRAM total/free, torch reserved total/free) and the
//! kernel's whole-GPU counters (amdgpu sysfs `mem_info_vram_used`, which
//! include every other GPU user such as the compositor).
//!
//! Release uses the engine's real mechanism (`POST /free` with
//! `unload_models` + `free_memory`, applied asynchronously by its worker) and
//! reports `released: true` only when Torch reservations measurably decrease
//! on the same devices. Whole-GPU counters alone cannot prove a release.
use crate::comfy::Comfy;
use crate::pass_states::{MemoryOutcome, PassMemoryManager};
use crate::Result;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMemory {
    pub name: String,
    pub device_type: String,
    pub index: Option<u64>,
    pub vram_total: u64,
    pub vram_free: u64,
    pub torch_vram_total: Option<u64>,
    pub torch_vram_free: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuCounters {
    pub card: String,
    pub vram_used: u64,
    pub vram_total: u64,
    pub gtt_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySnapshot {
    pub saya_rss: Option<u64>,
    pub comfy_pid: Option<u32>,
    pub comfy_rss: Option<u64>,
    pub system_ram_free: Option<u64>,
    pub devices: Vec<DeviceMemory>,
    pub gpu: Vec<GpuCounters>,
    pub engine_error: Option<String>,
}

impl MemorySnapshot {
    /// VRAM used as seen by the engine (sum over devices), if reported.
    pub fn engine_vram_used(&self) -> Option<u64> {
        (!self.devices.is_empty()).then(|| {
            self.devices
                .iter()
                .map(|d| d.vram_total.saturating_sub(d.vram_free))
                .sum()
        })
    }
    /// Torch reserved memory (ComfyUI reports `torch_vram_total` as reserved).
    pub fn torch_reserved(&self) -> Option<u64> {
        let values: Option<Vec<u64>> = self.devices.iter().map(|d| d.torch_vram_total).collect();
        values.filter(|v| !v.is_empty()).map(|v| v.iter().sum())
    }
}

fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// RSS in bytes from `/proc/<pid>/status`.
pub fn process_rss(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// RSS of a process and all its descendants (app + WebKit helpers), computed
/// from `/proc/<pid>/task/<pid>/children` without touching the process.
pub fn process_tree_rss(root: u32) -> Option<(u64, Vec<(u32, u64)>)> {
    fn children(pid: u32) -> Vec<u32> {
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
            .ok()
            .into_iter()
            .flat_map(|text| {
                text.split_whitespace()
                    .filter_map(|value| value.parse().ok())
                    .collect::<Vec<u32>>()
            })
            .collect()
    }
    let mut stack = vec![root];
    let mut seen = std::collections::BTreeSet::new();
    let mut processes = Vec::new();
    let mut total = 0u64;
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(rss) = process_rss(pid) {
            total = total.saturating_add(rss);
            processes.push((pid, rss));
        }
        stack.extend(children(pid));
    }
    processes.sort_by_key(|(pid, _)| *pid);
    (!processes.is_empty()).then_some((total, processes))
}

/// PID of the local process listening on a TCP port (same user only).
pub fn listening_pid(port: u16) -> Option<u32> {
    let mut inode = None;
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 || fields[3] != "0A" {
                continue; // 0A = LISTEN
            }
            let local_port = fields[1]
                .rsplit(':')
                .next()
                .and_then(|p| u16::from_str_radix(p, 16).ok());
            if local_port == Some(port) {
                inode = Some(fields[9].to_owned());
                break;
            }
        }
        if inode.is_some() {
            break;
        }
    }
    let target = format!("socket:[{}]", inode?);
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if std::fs::read_link(fd.path()).is_ok_and(|link| link.to_string_lossy() == target) {
                return Some(pid);
            }
        }
    }
    None
}

/// amdgpu counters under a sysfs root (`/sys/class/drm` in production).
pub fn gpu_counters(drm: &Path) -> Vec<GpuCounters> {
    let mut cards = Vec::new();
    for entry in std::fs::read_dir(drm).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device = entry.path().join("device");
        if let (Some(used), Some(total)) = (
            read_u64(&device.join("mem_info_vram_used")),
            read_u64(&device.join("mem_info_vram_total")),
        ) {
            cards.push(GpuCounters {
                card: name,
                vram_used: used,
                vram_total: total,
                gtt_used: read_u64(&device.join("mem_info_gtt_used")),
            });
        }
    }
    cards.sort_by(|a, b| a.card.cmp(&b.card));
    cards
}

fn devices(stats: &Value) -> Vec<DeviceMemory> {
    stats["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            // ComfyUI uses these same fields for CPU/MPS system RAM.
            let device_type = d["type"].as_str()?;
            if !matches!(device_type, "cuda" | "xpu" | "npu" | "mlu") {
                return None;
            }
            Some(DeviceMemory {
                name: d["name"].as_str()?.to_owned(),
                device_type: device_type.into(),
                index: d["index"].as_u64(),
                vram_total: d["vram_total"].as_u64()?,
                vram_free: d["vram_free"].as_u64()?,
                torch_vram_total: d["torch_vram_total"].as_u64(),
                torch_vram_free: d["torch_vram_free"].as_u64(),
            })
        })
        .collect()
}

/// One read-only measurement. Nothing here allocates or releases anything.
pub fn measure(client: Option<&Comfy>, port: u16, drm: Option<&Path>) -> MemorySnapshot {
    let (devices, ram_free, engine_error) = match client.map(|c| c.get("/system_stats")) {
        Some(Ok(stats)) => (devices(&stats), stats["system"]["ram_free"].as_u64(), None),
        Some(Err(error)) => (Vec::new(), None, Some(error.to_string())),
        None => (Vec::new(), None, Some("Moteur non interrogé".into())),
    };
    let comfy_pid = listening_pid(port);
    MemorySnapshot {
        saya_rss: process_rss(std::process::id()),
        comfy_pid,
        comfy_rss: comfy_pid.and_then(process_rss),
        system_ram_free: ram_free,
        devices,
        gpu: drm.map(gpu_counters).unwrap_or_default(),
        engine_error,
    }
}

/// Real engine-backed release between passes (ComfyUI `/free`).
pub struct ComfyMemoryManager {
    pub client: Comfy,
    pub port: u16,
    /// `Some("/sys/class/drm")` in production; tests may point elsewhere or skip.
    pub drm: Option<PathBuf>,
    /// How long to wait for the engine worker to apply the request.
    pub settle: Duration,
    /// Minimal measured decrease to claim a release (bytes).
    pub threshold: u64,
}

fn mib(bytes: Option<u64>) -> String {
    bytes.map_or("?".into(), |b| format!("{:.0} Mio", b as f64 / 1048576.0))
}

impl ComfyMemoryManager {
    fn snapshot(&self) -> MemorySnapshot {
        measure(Some(&self.client), self.port, self.drm.as_deref())
    }

    /// Release with before/after measurements (JSON) for the run history.
    pub fn release_measured(&self) -> Result<(bool, Value)> {
        let queue = self.client.get("/queue")?;
        for key in ["queue_running", "queue_pending"] {
            let entries = queue[key]
                .as_array()
                .ok_or("File ComfyUI invalide ; déchargement refusé")?;
            if !entries.is_empty() {
                return Err("ComfyUI occupé ; déchargement refusé".into());
            }
        }
        let before = self.snapshot();
        self.client
            .control("/free", json!({"unload_models": true, "free_memory": true}))?;
        let started = Instant::now();
        let decreased = |after: &MemorySnapshot| -> bool {
            let same_devices = before.engine_error.is_none()
                && after.engine_error.is_none()
                && before.devices.len() == after.devices.len()
                && before.devices.iter().zip(&after.devices).all(|(b, a)| {
                    b.name == a.name
                        && b.device_type == a.device_type
                        && b.index == a.index
                        && b.vram_total == a.vram_total
                });
            same_devices
                && matches!((before.torch_reserved(), after.torch_reserved()),
                (Some(b), Some(a)) if b > a && b - a >= self.threshold.max(1))
        };
        let mut after = self.snapshot();
        while !decreased(&after) && started.elapsed() < self.settle {
            std::thread::sleep(Duration::from_millis(250));
            after = self.snapshot();
        }
        let released = decreased(&after);
        Ok((
            released,
            json!({"before": before, "after": after, "released": released, "waitedMs": started.elapsed().as_millis() as u64}),
        ))
    }
}

impl PassMemoryManager for ComfyMemoryManager {
    fn backend(&self) -> &'static str {
        "comfyui-free+amdgpu-sysfs"
    }

    fn release_after_pass(&self, _pipeline: &str, _pass: &str) -> Result<MemoryOutcome> {
        let (released, report) = self.release_measured()?;
        let before = &report["before"];
        let after = &report["after"];
        let pick = |v: &Value, key: &str| v[key].as_u64();
        let engine = |v: &Value| -> Option<u64> {
            v["devices"].as_array().filter(|d| !d.is_empty()).map(|d| {
                d.iter()
                    .map(|x| {
                        pick(x, "vramTotal")
                            .unwrap_or(0)
                            .saturating_sub(pick(x, "vramFree").unwrap_or(0))
                    })
                    .sum()
            })
        };
        let details = format!(
            "VRAM moteur {} → {} ; noyau {} → {} ; RSS ComfyUI {} → {} ; {}",
            mib(engine(before)),
            mib(engine(after)),
            mib(before["gpu"]
                .as_array()
                .filter(|g| !g.is_empty())
                .map(|g| g.iter().filter_map(|x| pick(x, "vramUsed")).sum())),
            mib(after["gpu"]
                .as_array()
                .filter(|g| !g.is_empty())
                .map(|g| g.iter().filter_map(|x| pick(x, "vramUsed")).sum())),
            mib(pick(before, "comfyRss")),
            mib(pick(after, "comfyRss")),
            if released {
                "baisse mesurée"
            } else {
                "aucune baisse mesurée : libération non revendiquée"
            }
        );
        Ok(MemoryOutcome {
            backend: self.backend().into(),
            released,
            details,
        })
    }

    fn prepare_pass(&self, _pipeline: &str, _pass: &str) -> Result<MemoryOutcome> {
        let snapshot = self.snapshot();
        Ok(MemoryOutcome {
            backend: self.backend().into(),
            released: false,
            details: format!(
                "Mesure avant passe : VRAM moteur {}",
                mib(snapshot.engine_vram_used())
            ),
        })
    }
}
