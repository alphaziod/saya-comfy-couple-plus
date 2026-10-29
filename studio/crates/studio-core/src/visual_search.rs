//! Local visual search, with no downloaded model and no network send.
//!
//! 100% pure Rust extractor (the `image` crate is already present): each image is
//! decoded only once under strict limits then reduced to 64×64 BEFORE any
//! processing; no characterization operation runs on the full
//! resolution. The vector combines:
//!
//! 1. `phash`: DCT 8×8 perceptual fingerprint (63 bits, the DC component
//!    being excluded from the median like the bits);
//! 2. `histogram`: HSV colour histogram reduced to 8×4×4 = 128 bins, normalized;
//! 3. `gradient`: mean magnitude of the central gradients on luminance,
//!    normalized to [0,1];
//! 4. `spatial`: 4×4 grid of normalized RGB means (48 values), which
//!    brings local layout information without any external model.
//!
//! Serialization (constant `VECTOR_BYTES`): 1 version byte + 8 bytes
//! pHash (u64 big-endian) + 128 f32 + 48 f32 + 1 f32 = 717 bytes, stored as a SQLite BLOB (migration v13, table `image_features`).
//!
//! Search: brute-force nearest neighbours over all the vectors of the
//! current extractor version. Cost O(N × C) with C ≈ 200 operations per
//! candidate (128-term dot product + 64-bit popcount): at 10,000 images
//! this stays around a millisecond in native Rust. An ANN index
//! (HNSW, IVF) is only justified at several hundred thousand
//! vectors; it would add a dependency and a full reindex with no
//! measurable gain at the current scale (≈ 3,200 images).
//!
//! Score = 0.35 × (1 − hamming/63) + 0.35 × cosine(histograms) + 0.15 ×
//! (1 − |gradient_a − gradient_b|) + 0.15 × cosine(spatial grid), clamped to [0,1]. The weights are
//! documented constants: global structure robust to re-encoding (pHash),
//! palette (histogram) and texture (gradient). This score measures an approximate
//! visual similarity: it is NEVER proof of a
//! character's identity.

use crate::{Database, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Extractor version stored with each vector; any change to the
/// format must increment this constant: older rows
/// go back to "pending" and are recomputed by the visual reindex.
pub const EXTRACTOR_VERSION: u32 = 2;
const FEATURE_SIZE: u32 = 64;
const PLANE: usize = (FEATURE_SIZE * FEATURE_SIZE) as usize;
const HSV_BINS: usize = 128;
const SPATIAL_GRID: usize = 4;
const SPATIAL_BINS: usize = SPATIAL_GRID * SPATIAL_GRID * 3;
const HASH_BITS: u32 = 63;
const MAX_GRADIENT: f32 = 127.5 * std::f32::consts::SQRT_2;
pub const VECTOR_BYTES: usize = 1 + 8 + HSV_BINS * 4 + SPATIAL_BINS * 4 + 4;
pub const WEIGHT_HASH: f32 = 0.35;
pub const WEIGHT_COLOR: f32 = 0.35;
pub const WEIGHT_GRADIENT: f32 = 0.15;
pub const WEIGHT_SPATIAL: f32 = 0.15;

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureVector {
    pub phash: u64,
    pub histogram: [f32; HSV_BINS],
    pub spatial: [f32; SPATIAL_BINS],
    pub gradient: f32,
}

impl FeatureVector {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(VECTOR_BYTES);
        out.push(EXTRACTOR_VERSION as u8);
        out.extend_from_slice(&self.phash.to_be_bytes());
        for value in &self.histogram {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in &self.spatial {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&self.gradient.to_le_bytes());
        out
    }
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != VECTOR_BYTES || bytes[0] != EXTRACTOR_VERSION as u8 {
            return None;
        }
        let phash = u64::from_be_bytes(bytes[1..9].try_into().ok()?);
        let mut histogram = [0f32; HSV_BINS];
        for (index, value) in histogram.iter_mut().enumerate() {
            let start = 9 + index * 4;
            *value = f32::from_le_bytes(bytes[start..start + 4].try_into().ok()?);
        }
        let mut spatial = [0f32; SPATIAL_BINS];
        let spatial_start = 9 + HSV_BINS * 4;
        for (index, value) in spatial.iter_mut().enumerate() {
            let start = spatial_start + index * 4;
            *value = f32::from_le_bytes(bytes[start..start + 4].try_into().ok()?);
        }
        let gradient = f32::from_le_bytes(bytes[VECTOR_BYTES - 4..].try_into().ok()?);
        if histogram.iter().any(|v| !v.is_finite())
            || spatial.iter().any(|v| !v.is_finite())
            || !gradient.is_finite()
        {
            return None;
        }
        Some(Self {
            phash,
            histogram,
            spatial,
            gradient,
        })
    }
    pub fn phash_bytes(&self) -> [u8; 8] {
        self.phash.to_be_bytes()
    }
}

/// Decomposed score, always presented as approximate visual similarity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity {
    pub overall: f32,
    pub structure: f32,
    pub color: f32,
    pub gradient: f32,
    pub spatial: f32,
}

pub fn similarity(a: &FeatureVector, b: &FeatureVector) -> Similarity {
    let distance = (a.phash ^ b.phash).count_ones().min(HASH_BITS) as f32;
    let structure = 1.0 - distance / HASH_BITS as f32;
    let color = cosine(&a.histogram, &b.histogram);
    let gradient = (1.0 - (a.gradient - b.gradient).abs()).clamp(0.0, 1.0);
    let spatial = cosine(&a.spatial, &b.spatial);
    let overall = (WEIGHT_HASH * structure
        + WEIGHT_COLOR * color
        + WEIGHT_GRADIENT * gradient
        + WEIGHT_SPATIAL * spatial)
        .clamp(0.0, 1.0);
    Similarity {
        overall,
        structure,
        color,
        gradient,
        spatial,
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0f32;
    let mut norm_a = 0f32;
    let mut norm_b = 0f32;
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }
    (dot / (norm_a.sqrt() * norm_b.sqrt())).clamp(0.0, 1.0)
}

/// Decodes a file under strict limits and brings it down to the 64×64 RGB thumbnail.
///
/// The `image` crate decoder materializes the original frame only once,
/// bounded here by `Limits` (16384 px per side, 128 MiB of allocation
/// at most, like the existing display path); all the
/// perceptual analysis then operates exclusively on the 64×64 buffer. An image
/// beyond the limits is refused without allocating.
///
/// `resize_exact` squashes the aspect ratio: all images get the
/// same geometric treatment, a precondition of the pHash/gradient comparison
/// (the `thumbnail` alternative preserves the ratio and would produce thumbnails
/// of varying sizes, incompatible with a fixed-length vector).
pub fn downscaled(path: &Path) -> Result<image::RgbImage> {
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode()?;
    Ok(decoded
        .resize_exact(
            FEATURE_SIZE,
            FEATURE_SIZE,
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8())
}

/// Computes the vector on the 64×64 thumbnail only; any other size
/// is refused to guarantee the analysis never runs on the full
/// resolution.
pub fn features_from_rgb(image: &image::RgbImage) -> Result<FeatureVector> {
    if image.width() != FEATURE_SIZE || image.height() != FEATURE_SIZE {
        return Err(format!(
            "L'analyse visuelle attend une vignette {FEATURE_SIZE}×{FEATURE_SIZE}, \
             reçue {}×{}",
            image.width(),
            image.height()
        )
        .into());
    }
    let plane = luma_plane(image);
    Ok(FeatureVector {
        phash: perceptual_hash(&plane),
        histogram: hsv_histogram(image),
        spatial: spatial_grid(image),
        gradient: mean_gradient(&plane),
    })
}

pub fn extract_file(path: &Path) -> Result<FeatureVector> {
    features_from_rgb(&downscaled(path)?)
}

fn spatial_grid(image: &image::RgbImage) -> [f32; SPATIAL_BINS] {
    let mut sums = [[0f32; 3]; SPATIAL_GRID * SPATIAL_GRID];
    for (x, y, pixel) in image.enumerate_pixels() {
        let cell = (y as usize / (FEATURE_SIZE as usize / SPATIAL_GRID)) * SPATIAL_GRID
            + x as usize / (FEATURE_SIZE as usize / SPATIAL_GRID);
        for channel in 0..3 {
            sums[cell][channel] += f32::from(pixel[channel]) / 255.0;
        }
    }
    let per_cell = (FEATURE_SIZE as usize / SPATIAL_GRID).pow(2) as f32;
    let mut out = [0f32; SPATIAL_BINS];
    for (cell, sum) in sums.iter().enumerate() {
        for channel in 0..3 {
            out[cell * 3 + channel] = (sum[channel] / per_cell).clamp(0.0, 1.0);
        }
    }
    out
}

fn luma_plane(image: &image::RgbImage) -> [f32; PLANE] {
    let mut plane = [0f32; PLANE];
    for (index, pixel) in image.pixels().enumerate() {
        // ITU-R 601 luminance rounded, like the usual pHash implementations.
        let value = (299 * u32::from(pixel[0])
            + 587 * u32::from(pixel[1])
            + 114 * u32::from(pixel[2])
            + 500)
            / 1000;
        plane[index] = value.min(255) as f32;
    }
    plane
}

/// 2D separable DCT-II; only the 8×8 low-frequency coefficients are
/// computed (≈ 36k multiplications per image, the FFT is useless at this
/// size).
fn low_frequency_block(plane: &[f32; PLANE]) -> [[f32; 8]; 8] {
    let mut rows = [[0f32; 8]; PLANE / 64];
    for (y, row) in rows.iter_mut().enumerate() {
        *row = dct_lowfreq(&plane[y * 64..(y + 1) * 64]);
    }
    let mut block = [[0f32; 8]; 8];
    let mut column = [0f32; 64];
    for (u, out_column) in block.iter_mut().enumerate() {
        for y in 0..64 {
            column[y] = rows[y][u];
        }
        for (v, value) in dct_lowfreq(&column).into_iter().enumerate() {
            out_column[v] = value;
        }
    }
    block
}

/// Orthonormal 64-point DCT-II, only the first 8 coefficients.
fn dct_lowfreq(input: &[f32]) -> [f32; 8] {
    let mut out = [0f32; 8];
    for (u, value) in out.iter_mut().enumerate() {
        let scale = if u == 0 {
            f32::sqrt(1.0 / 64.0)
        } else {
            f32::sqrt(2.0 / 64.0)
        };
        let sum: f32 = input
            .iter()
            .enumerate()
            .map(|(x, sample)| {
                sample * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 128.0).cos()
            })
            .sum();
        *value = scale * sum;
    }
    out
}

fn perceptual_hash(plane: &[f32; PLANE]) -> u64 {
    let block = low_frequency_block(plane);
    // Row-by-row flattening into [f32; 64], without allocation.
    let flat: [f32; 64] = std::array::from_fn(|index| block[index / 8][index % 8]);
    // Median of the 63 alternating coefficients; the DC component (index 0)
    // is excluded from the median like the bits to stay insensitive to
    // global exposure.
    let mut alternating: Vec<f32> = flat[1..].to_vec();
    alternating.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = alternating[alternating.len() / 2];
    let mut hash = 0u64;
    for (index, value) in flat.iter().enumerate().skip(1) {
        if *value > median {
            hash |= 1 << index;
        }
    }
    hash
}

fn hsv_histogram(image: &image::RgbImage) -> [f32; HSV_BINS] {
    let mut counts = [0u32; HSV_BINS];
    for pixel in image.pixels() {
        let (hue, saturation, value) = rgb_to_hsv(pixel[0], pixel[1], pixel[2]);
        let hue_bin = (hue / 45.0).floor().clamp(0.0, 7.0) as usize;
        let saturation_bin = (saturation * 4.0).floor().clamp(0.0, 3.0) as usize;
        let value_bin = (value * 4.0).floor().clamp(0.0, 3.0) as usize;
        counts[hue_bin * 16 + saturation_bin * 4 + value_bin] += 1;
    }
    let total = PLANE as f32;
    let mut histogram = [0f32; HSV_BINS];
    for (out, count) in histogram.iter_mut().zip(counts) {
        *out = count as f32 / total;
    }
    histogram
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut hue = if delta == 0.0 {
        0.0
    } else if (max - r).abs() < f32::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    if hue < 0.0 {
        hue += 360.0;
    }
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// Central differences with replicated borders; mean magnitude normalized
/// by the theoretical maximum of an 8-bit central gradient (127.5·√2).
fn mean_gradient(plane: &[f32; PLANE]) -> f32 {
    let at = |x: usize, y: usize| plane[y * 64 + x];
    let mut total = 0f32;
    for y in 0..64 {
        for x in 0..64 {
            let left = at(if x > 0 { x - 1 } else { 0 }, y);
            let right = at((x + 1).min(63), y);
            let up = at(x, if y > 0 { y - 1 } else { 0 });
            let down = at(x, (y + 1).min(63));
            let gx = (right - left) / 2.0;
            let gy = (down - up) / 2.0;
            total += (gx * gx + gy * gy).sqrt();
        }
    }
    (total / PLANE as f32 / MAX_GRADIENT).clamp(0.0, 1.0)
}

struct Candidate {
    id: String,
    path: String,
    vector: FeatureVector,
}

/// Common body of the "pending" queries: images present in an
/// active library without a vector of the current extractor version.
const PENDING_FROM: &str = "FROM assets a \
    JOIN libraries l ON l.id=a.library_id \
    LEFT JOIN image_features f ON f.asset_id=a.id AND f.extractor_version=?1 \
    WHERE a.availability='present' AND l.enabled=1 AND f.asset_id IS NULL";

impl Database {
    fn stored_vector(&self, asset_id: &str) -> Result<Option<FeatureVector>> {
        let vector: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT vector FROM image_features WHERE asset_id=?1 AND extractor_version=?2",
                params![asset_id, EXTRACTOR_VERSION],
                |r| r.get(0),
            )
            .optional()?;
        Ok(vector.as_deref().and_then(FeatureVector::from_bytes))
    }

    fn candidates(&self, exclude: &str) -> Result<Vec<Candidate>> {
        let mut statement = self.connection.prepare(
            "SELECT f.vector,a.id,a.relative_path FROM image_features f \
             JOIN assets a ON a.id=f.asset_id \
             JOIN libraries l ON l.id=a.library_id \
             WHERE a.availability='present' AND l.enabled=1 \
             AND f.extractor_version=?1 AND a.id!=?2",
        )?;
        let rows = statement
            .query_map(params![EXTRACTOR_VERSION, exclude], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(bytes, id, path)| {
                FeatureVector::from_bytes(&bytes).map(|vector| Candidate { id, path, vector })
            })
            .collect())
    }

    /// Brute-force nearest neighbours; the score is an approximate
    /// visual similarity, never a character identification.
    /// A query image without a stored vector is extracted on the fly in read-only
    /// mode (`vectorized:false` in the response) without writing anything.
    pub fn visual_search(&self, asset_id: &str, limit: u32) -> Result<Value> {
        let limit = limit.clamp(1, 96);
        let (query, vectorized) = match self.stored_vector(asset_id)? {
            Some(vector) => (vector, true),
            None => {
                let path = self.asset_path(asset_id)?;
                (extract_file(&path)?, false)
            }
        };
        let candidates = self.candidates(asset_id)?;
        let compared = candidates.len();
        let mut scored: Vec<(Similarity, &Candidate)> = candidates
            .iter()
            .map(|candidate| (similarity(&query, &candidate.vector), candidate))
            .collect();
        scored.sort_by(|a, b| {
            b.0.overall
                .partial_cmp(&a.0.overall)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.id.cmp(&b.1.id))
        });
        let matches: Vec<Value> = scored
            .into_iter()
            .take(limit as usize)
            .map(|(score, candidate)| {
                json!({
                    "id": candidate.id,
                    "path": candidate.path,
                    "score": score.overall,
                    "structure": score.structure,
                    "color": score.color,
                    "gradient": score.gradient,
                })
            })
            .collect();
        Ok(json!({
            "query": {"id": asset_id, "vectorized": vectorized},
            "matches": matches,
            "compared": compared,
            "weights": {
                "structure": WEIGHT_HASH,
                "color": WEIGHT_COLOR,
                "gradient": WEIGHT_GRADIENT,
                "spatial": WEIGHT_SPATIAL,
            },
            "extractorVersion": EXTRACTOR_VERSION,
        }))
    }

    /// Images present without a vector of the current version. `limit=0` returns
    /// only the counter; otherwise at most `limit` paths (capped at 200).
    pub fn visual_pending(&self, limit: u32) -> Result<Value> {
        let count: u32 = self.connection.query_row(
            &format!("SELECT count(*) {PENDING_FROM}"),
            params![EXTRACTOR_VERSION],
            |r| r.get(0),
        )?;
        let mut items = Vec::new();
        if limit > 0 {
            let mut statement = self.connection.prepare(&format!(
                "SELECT a.id,a.relative_path {PENDING_FROM} \
                 ORDER BY l.root_path,a.relative_path LIMIT {}",
                limit.min(200)
            ))?;
            items = statement
                .query_map(params![EXTRACTOR_VERSION], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "path": r.get::<_, String>(1)?,
                    }))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(json!({"count": count, "items": items}))
    }

    /// Incremental vectorization file by file: one transaction per
    /// image, error kept without blocking the following ones, clean stop on
    /// request. Indexing lock shared with the gallery traversal to
    /// avoid two simultaneous decoders. `budget` bounds the number of images
    /// processed (`None` = all the rest).
    pub fn visual_backfill(
        &mut self,
        budget: Option<u32>,
        mut progress: impl FnMut(u32, u32),
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Value> {
        let _lock = self.index_lock()?;
        let rows: Vec<(String, String, String)> = self.pending_rows(budget)?;
        let total = rows.len() as u32;
        let mut vectorized = 0u32;
        let mut failed = 0u32;
        let mut stop = false;
        let mut errors: Vec<String> = Vec::new();
        for (done, (id, relative, root)) in rows.iter().enumerate() {
            if cancelled() {
                stop = true;
                break;
            }
            let path: PathBuf = Path::new(root).join(relative);
            let outcome = extract_file(&path).and_then(|vector| {
                let tx = self.connection.transaction()?;
                tx.execute(
                    "INSERT INTO image_features(asset_id,extractor_version,phash,vector) \
                     VALUES(?1,?2,?3,?4) ON CONFLICT(asset_id) DO UPDATE SET \
                     extractor_version=excluded.extractor_version,phash=excluded.phash, \
                     vector=excluded.vector,created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                    params![
                        id,
                        EXTRACTOR_VERSION,
                        vector.phash_bytes().as_slice(),
                        vector.to_bytes()
                    ],
                )?;
                tx.commit()?;
                Ok(())
            });
            match outcome {
                Ok(()) => vectorized += 1,
                Err(error) => {
                    failed += 1;
                    if errors.len() < 30 {
                        errors.push(format!("{} : {error}", path.display()));
                    }
                }
            }
            progress(done as u32 + 1, total);
        }
        let remaining: u32 = self.connection.query_row(
            &format!("SELECT count(*) {PENDING_FROM}"),
            params![EXTRACTOR_VERSION],
            |r| r.get(0),
        )?;
        Ok(json!({
            "vectorized": vectorized,
            "failed": failed,
            "cancelled": stop,
            "remaining": remaining,
            "errors": errors,
        }))
    }

    /// Invalidates all vectors (all extractor versions): images
    /// go back to "pending" and the visual reindex recomputes everything.
    /// Returns the number of rows deleted. Maintenance usage / extractor
    /// version upgrade; the gallery itself is never touched.
    pub fn visual_invalidate(&self) -> Result<u64> {
        Ok(self.connection.execute("DELETE FROM image_features", [])? as u64)
    }

    fn pending_rows(&self, limit: Option<u32>) -> Result<Vec<(String, String, String)>> {
        let sql = match limit {
            Some(limit) => format!(
                "SELECT a.id,a.relative_path,l.root_path {PENDING_FROM} \
                 ORDER BY l.root_path,a.relative_path LIMIT {limit}"
            ),
            None => format!(
                "SELECT a.id,a.relative_path,l.root_path {PENDING_FROM} \
                 ORDER BY l.root_path,a.relative_path"
            ),
        };
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map(params![EXTRACTOR_VERSION], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "vissearch-{label}-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Blue → red gradient with a fixed pattern: a rich synthetic image
    /// (colour + texture), with no external resource.
    fn synthetic_rgb(width: u32, height: u32, shift: u8) -> image::RgbImage {
        let mut image = image::RgbImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let stripe = if (x / 8 + y / 8) % 2 == 0 { 40u8 } else { 0 };
                let r = ((x * 255) / width.max(1)) as u8;
                let g = ((y * 255) / height.max(1)) as u8;
                image.put_pixel(x, y, image::Rgb([r.saturating_add(shift), g, 200 - stripe]));
            }
        }
        image
    }

    fn encoded(format: image::ImageFormat, shift: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(synthetic_rgb(320, 240, shift))
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
        bytes
    }

    fn png_bytes(shift: u8) -> Vec<u8> {
        encoded(image::ImageFormat::Png, shift)
    }

    /// Temporary library indexed in one go; never the user
    /// gallery.
    fn library_with(files: &[(&str, Vec<u8>)]) -> (Database, TempDir) {
        let dir = TempDir::new("library");
        for (name, bytes) in files {
            std::fs::write(dir.0.join(name), bytes).unwrap();
        }
        let mut db = Database::in_memory().unwrap();
        let report = db
            .index_library_controlled(&dir.0, |_, _| {}, || false)
            .unwrap();
        assert_eq!(report.error_count, 0, "{:?}", report.errors);
        (db, dir)
    }

    fn asset_for(db: &Database, name: &str) -> String {
        db.connection
            .query_row(
                "SELECT a.id FROM assets a WHERE a.relative_path=?1",
                [name],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn migration_v13_is_additive_strict_and_typed() {
        let db = Database::in_memory().unwrap();
        let version: u32 = db
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert!(
            version >= 13,
            "migration 0013 must be applied, got {version}"
        );
        let name: String = db
            .connection
            .query_row(
                "SELECT name FROM schema_migrations WHERE version=13",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "visual-search-features");
        db.connection
            .execute(
                "INSERT INTO libraries(id,label,root_path,platform) VALUES('l','l','/tmp/no-such-library-root','linux')",
                [],
            )
            .unwrap();
        db.connection
            .execute(
                "INSERT INTO assets(id,library_id,relative_path) VALUES('a','l','x.png')",
                [],
            )
            .unwrap();
        // Foreign key to assets respected.
        assert!(db
            .connection
            .execute(
                "INSERT INTO image_features(asset_id,extractor_version,phash,vector) VALUES('missing',1,x'0000000000000000',zeroblob(525))",
                [],
            )
            .is_err());
        // Vector length and version checked.
        assert!(db
            .connection
            .execute(
                "INSERT INTO image_features(asset_id,extractor_version,phash,vector) VALUES('a',1,x'0000000000000000',zeroblob(524))",
                [],
            )
            .is_err());
        assert!(db
            .connection
            .execute(
                "INSERT INTO image_features(asset_id,extractor_version,phash,vector) VALUES('a',0,x'0000000000000000',zeroblob(525))",
                [],
            )
            .is_err());
        db.connection
            .execute(
                "INSERT INTO image_features(asset_id,extractor_version,phash,vector) VALUES('a',1,x'0000000000000000',zeroblob(525))",
                [],
            )
            .unwrap();
    }

    #[test]
    fn vector_serialization_is_versioned_and_typed() {
        let mut image = image::RgbImage::new(64, 64);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x * 4) as u8, (y * 4) as u8, 128]);
        }
        let vector = features_from_rgb(&image).unwrap();
        let bytes = vector.to_bytes();
        assert_eq!(bytes.len(), VECTOR_BYTES);
        assert_eq!(bytes[0], EXTRACTOR_VERSION as u8);
        assert_eq!(&bytes[1..9], &vector.phash_bytes());
        assert_eq!(FeatureVector::from_bytes(&bytes).unwrap(), vector);
        let mut tampered = bytes.clone();
        tampered[0] = 1;
        assert!(FeatureVector::from_bytes(&tampered).is_none());
        let mut truncated = bytes.clone();
        truncated.pop();
        assert!(FeatureVector::from_bytes(&truncated).is_none());
        assert!(features_from_rgb(&image::RgbImage::new(32, 32)).is_err());
    }

    #[test]
    fn same_image_yields_a_deterministic_vector() {
        let dir = TempDir::new("deterministic");
        std::fs::write(dir.0.join("a.png"), png_bytes(0)).unwrap();
        let first = extract_file(&dir.0.join("a.png")).unwrap();
        let second = extract_file(&dir.0.join("a.png")).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.to_bytes(), second.to_bytes());
    }

    #[test]
    fn jpeg_reencoding_keeps_high_similarity_while_a_different_image_scores_lower() {
        let dir = TempDir::new("robustness");
        std::fs::write(dir.0.join("original.png"), png_bytes(0)).unwrap();
        std::fs::write(
            dir.0.join("reencoded.jpg"),
            encoded(image::ImageFormat::Jpeg, 0),
        )
        .unwrap();
        // Deliberately very different image: saturated uniform flat.
        let mut flat = image::RgbImage::new(320, 240);
        for pixel in flat.pixels_mut() {
            *pixel = image::Rgb([220, 40, 10]);
        }
        let mut flat_bytes = Vec::new();
        image::DynamicImage::ImageRgb8(flat)
            .write_to(&mut Cursor::new(&mut flat_bytes), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.0.join("flat.png"), flat_bytes).unwrap();
        let query = extract_file(&dir.0.join("original.png")).unwrap();
        let reencoded = extract_file(&dir.0.join("reencoded.jpg")).unwrap();
        let flat = extract_file(&dir.0.join("flat.png")).unwrap();
        let similar = similarity(&query, &reencoded);
        let different = similarity(&query, &flat);
        assert!(
            similar.overall > 0.75,
            "re-encodage JPEG trop pénalisé : {similar:?}"
        );
        assert!(
            similar.overall > different.overall,
            "une image différente score trop haut : {similar:?} vs {different:?}"
        );
        assert!(different.overall < 0.6, "{different:?}");
    }

    #[test]
    fn large_images_go_through_the_64x64_downscale_path() {
        let dir = TempDir::new("large");
        // 2000×1500: real decoding of a large synthetic image; the
        // result must be exactly that of its 64×64 thumbnail.
        let large = synthetic_rgb(2000, 1500, 0);
        let mut large_bytes = Vec::new();
        image::DynamicImage::ImageRgb8(large.clone())
            .write_to(&mut Cursor::new(&mut large_bytes), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.0.join("large.png"), large_bytes).unwrap();
        let from_file = extract_file(&dir.0.join("large.png")).unwrap();
        let expected = features_from_rgb(
            &image::DynamicImage::ImageRgb8(large)
                .resize_exact(64, 64, image::imageops::FilterType::Triangle)
                .to_rgb8(),
        )
        .unwrap();
        assert_eq!(from_file, expected);
        // Decoding limits exceeded refused without allocating.
        let mut wide = image::RgbImage::new(16385, 2);
        wide.pixels_mut().for_each(|p| *p = image::Rgb([1, 2, 3]));
        let mut wide_bytes = Vec::new();
        image::DynamicImage::ImageRgb8(wide)
            .write_to(&mut Cursor::new(&mut wide_bytes), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.0.join("wide.png"), wide_bytes).unwrap();
        assert!(extract_file(&dir.0.join("wide.png")).is_err());
    }

    #[test]
    fn freshly_indexed_images_are_vectorized_and_visible_in_top_k_search() {
        let (db, _dir) = library_with(&[
            ("query.png", png_bytes(0)),
            ("near.png", png_bytes(0)),
            ("far.png", png_bytes(200)),
        ]);
        assert_eq!(db.visual_pending(0).unwrap()["count"], 0);
        assert!(db.visual_search("no-such-asset", 10).is_err());
        let response = db.visual_search(&asset_for(&db, "query.png"), 2).unwrap();
        assert_eq!(response["query"]["vectorized"], true);
        assert_eq!(response["compared"], 2);
        let matches = response["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
        let scores: Vec<f64> = matches
            .iter()
            .map(|m| m["score"].as_f64().unwrap())
            .collect();
        assert!(
            scores.windows(2).all(|window| window[0] >= window[1]),
            "scores non triés : {scores:?}"
        );
        // The near-duplicate comes before the very different image.
        assert_eq!(matches[0]["path"], "near.png");
        assert_eq!(matches[1]["path"], "far.png");
    }

    #[test]
    fn images_added_after_the_first_index_become_searchable() {
        let (mut db, dir) = library_with(&[("original.png", png_bytes(0))]);
        std::fs::write(dir.0.join("later.png"), png_bytes(0)).unwrap();
        db.index_library_controlled(&dir.0, |_, _| {}, || false)
            .unwrap();
        assert_eq!(db.visual_pending(0).unwrap()["count"], 0);
        let query = asset_for(&db, "original.png");
        let matches = db.visual_search(&query, 10).unwrap()["matches"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "later.png");
        assert!(matches[0]["score"].as_f64().unwrap() > 0.75);
    }

    #[test]
    fn pre_existing_assets_without_vector_are_listed_then_backfilled() {
        let (mut db, _dir) = library_with(&[("original.png", png_bytes(0))]);
        // Database prior to v13: the asset exists but no vector.
        let asset = asset_for(&db, "original.png");
        db.connection
            .execute("DELETE FROM image_features", [])
            .unwrap();
        let pending = db.visual_pending(5).unwrap();
        assert_eq!(pending["count"], 1);
        assert_eq!(pending["items"][0]["path"], "original.png");
        // Zero budget: nothing is processed, the queue stays whole.
        let report = db.visual_backfill(Some(0), |_, _| {}, || false).unwrap();
        assert_eq!(report["vectorized"], 0);
        assert_eq!(report["remaining"], 1);
        let report = db.visual_backfill(None, |_, _| {}, || false).unwrap();
        assert_eq!(report["vectorized"], 1);
        assert_eq!(report["failed"], 0);
        assert_eq!(report["remaining"], 0);
        assert_eq!(db.visual_pending(0).unwrap()["count"], 0);
        assert_eq!(
            db.visual_search(&asset, 5).unwrap()["query"]["vectorized"],
            true
        );
    }

    #[test]
    fn vectors_and_scores_survive_a_database_reopen() {
        let dir = TempDir::new("persist");
        std::fs::write(dir.0.join("original.png"), png_bytes(0)).unwrap();
        std::fs::write(dir.0.join("other.png"), png_bytes(200)).unwrap();
        let path = dir.0.join("studio.sqlite3");
        let before = {
            let mut db = Database::open(&path).unwrap();
            db.index_library_controlled(&dir.0, |_, _| {}, || false)
                .unwrap();
            let asset = asset_for(&db, "original.png");
            db.visual_search(&asset, 10).unwrap()
        };
        let after = {
            let db = Database::open(&path).unwrap();
            let asset = asset_for(&db, "original.png");
            db.visual_search(&asset, 10).unwrap()
        };
        assert_eq!(before, after);
        assert_eq!(after["matches"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn corrupt_files_never_break_the_visual_index() {
        let dir = TempDir::new("corrupt");
        // Valid truncated PNG: the header passes, decoding fails, the asset
        // stays indexed without a vector and the pending queue flags it.
        let mut truncated = png_bytes(0);
        truncated.truncate(120);
        std::fs::write(dir.0.join("truncated.png"), truncated).unwrap();
        std::fs::write(dir.0.join("good.png"), png_bytes(0)).unwrap();
        std::fs::write(dir.0.join("garbage.png"), b"not an image at all").unwrap();
        let mut db = Database::in_memory().unwrap();
        let report = db
            .index_library_controlled(&dir.0, |_, _| {}, || false)
            .unwrap();
        // The gallery keeps its resilient behaviour: errors logged,
        // indexing not blocked. "garbage.png" fails as soon as the dimensions
        // are read (never indexed); "truncated.png" is indexed but its
        // vectorization on add fails; "good.png" is vectorized on add.
        assert_eq!(report.error_count, 1, "{:?}", report.errors);
        let pending = db.visual_pending(10).unwrap();
        assert_eq!(pending["count"], 1, "{pending}");
        assert_eq!(pending["items"][0]["path"], "truncated.png");
        let report = db.visual_backfill(None, |_, _| {}, || false).unwrap();
        assert_eq!(report["vectorized"], 0);
        assert_eq!(report["failed"], 1);
        assert_eq!(report["errors"].as_array().unwrap().len(), 1);
        assert_eq!(report["remaining"], 1);
        // A healthy image added afterwards is vectorized and findable despite
        // the corrupt files.
        std::fs::write(dir.0.join("second.png"), png_bytes(0)).unwrap();
        db.index_library_controlled(&dir.0, |_, _| {}, || false)
            .unwrap();
        let query = asset_for(&db, "second.png");
        let matches = db.visual_search(&query, 10).unwrap()["matches"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "good.png");
    }

    #[test]
    fn modified_image_with_failed_extraction_drops_its_stale_vector() {
        // V12 audit: re-indexing a changed file whose decoding now fails must
        // not keep the previous content's vector (it would match the old image).
        let (mut db, dir) =
            library_with(&[("changing.png", png_bytes(0)), ("twin.png", png_bytes(0))]);
        let asset = asset_for(&db, "changing.png");
        assert_eq!(db.visual_pending(10).unwrap()["count"], 0);
        let mut truncated = png_bytes(200);
        truncated.truncate(120);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.0.join("changing.png"), truncated).unwrap();
        db.index_library_controlled(&dir.0, |_, _| {}, || false)
            .unwrap();
        let stored: u32 = db
            .connection
            .query_row(
                "SELECT count(*) FROM image_features WHERE asset_id=?1",
                [&asset],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            stored, 0,
            "stale vector kept for a changed, undecodable file"
        );
        let twin = asset_for(&db, "twin.png");
        let matches = db.visual_search(&twin, 10).unwrap()["matches"]
            .as_array()
            .unwrap()
            .clone();
        assert!(matches.iter().all(|m| m["path"] != "changing.png"));
    }

    #[test]
    fn unvectorized_query_is_extracted_on_the_fly_without_writing() {
        let (db, _dir) = library_with(&[("original.png", png_bytes(0))]);
        let asset = asset_for(&db, "original.png");
        db.connection
            .execute("DELETE FROM image_features WHERE asset_id=?1", [&asset])
            .unwrap();
        let response = db.visual_search(&asset, 10).unwrap();
        assert_eq!(response["query"]["vectorized"], false);
        assert_eq!(response["matches"].as_array().unwrap().len(), 0);
        // No write: the search stays read-only.
        assert_eq!(db.visual_pending(0).unwrap()["count"], 1);
    }

    #[test]
    fn backfill_reports_progress_and_honours_cancellation() {
        let (mut db, _dir) = library_with(&[("original.png", png_bytes(0))]);
        db.connection
            .execute("DELETE FROM image_features", [])
            .unwrap();
        let mut ticks = Vec::new();
        let first = db
            .visual_backfill(None, |done, total| ticks.push((done, total)), || false)
            .unwrap();
        assert_eq!(first["vectorized"], 1);
        assert_eq!(ticks, vec![(1, 1)]);
        db.connection
            .execute("DELETE FROM image_features", [])
            .unwrap();
        let stopped = db.visual_backfill(None, |_, _| {}, || true).unwrap();
        assert_eq!(stopped["cancelled"], true);
        assert_eq!(stopped["vectorized"], 0);
        assert_eq!(db.visual_pending(0).unwrap()["count"], 1);
    }
}
