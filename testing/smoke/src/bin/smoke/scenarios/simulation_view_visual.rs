use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MANIFEST_SCHEMA: &str = "veoveo.io/simulation-view-render-comparison/v1";
const EVIDENCE_SCHEMA: &str = "veoveo.io/simulation-view-render-comparison-evidence/v1";
const REQUIRED_WIDTH: u32 = 1280;
const REQUIRED_HEIGHT: u32 = 720;
const MINIMUM_PAIRS_PER_CAMERA: usize = 5;
const MAXIMUM_MEAN_LUMA_DELTA: f64 = 35.0;
const MAXIMUM_CONTRAST_DELTA: f64 = 20.0;
const MAXIMUM_HISTOGRAM_DISTANCE: f64 = 0.30;
const MINIMUM_CONTRAST_RATIO: f64 = 0.65;
const MINIMUM_DETAIL_RATIO: f64 = 0.60;
const MINIMUM_DYNAMIC_RANGE_RATIO: f64 = 0.65;
const MAXIMUM_ADDED_HIGHLIGHT_FRACTION: f64 = 0.02;
const MAXIMUM_HIGHLIGHT_FRACTION: f64 = 0.05;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ComparisonCameraKind {
    MountedEntity,
    ChaseEntity,
    FormationOverview,
}

impl ComparisonCameraKind {
    const ALL: [Self; 3] = [
        Self::MountedEntity,
        Self::ChaseEntity,
        Self::FormationOverview,
    ];
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenderComparisonManifest {
    schema: String,
    comparison_id: String,
    human_review_passed: bool,
    pairs: Vec<RenderComparisonPair>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenderComparisonPair {
    pair_id: String,
    camera_kind: ComparisonCameraKind,
    pose_snapshot_digest: String,
    camera_transform_digest: String,
    native_image: PathBuf,
    simulation_view_image: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderComparisonEvidence {
    schema: &'static str,
    comparison_id: String,
    human_review_passed: bool,
    thresholds: ComparisonThresholds,
    pairs: Vec<PairEvidence>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonThresholds {
    required_width: u32,
    required_height: u32,
    minimum_pairs_per_camera: usize,
    maximum_mean_luma_delta: f64,
    maximum_contrast_delta: f64,
    maximum_histogram_distance: f64,
    minimum_contrast_ratio: f64,
    minimum_detail_ratio: f64,
    minimum_dynamic_range_ratio: f64,
    maximum_added_highlight_fraction: f64,
    maximum_highlight_fraction: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairEvidence {
    pair_id: String,
    camera_kind: ComparisonCameraKind,
    pose_snapshot_digest: String,
    camera_transform_digest: String,
    native_image_sha256: String,
    simulation_view_image_sha256: String,
    native: FrameMetrics,
    simulation_view: FrameMetrics,
    mean_luma_delta: f64,
    contrast_delta: f64,
    histogram_distance: f64,
    contrast_ratio: f64,
    detail_ratio: f64,
    dynamic_range_ratio: f64,
    added_highlight_fraction: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameMetrics {
    mean_luma: f64,
    luma_standard_deviation: f64,
    p05_luma: u8,
    p95_luma: u8,
    dynamic_range: u8,
    mean_luma_gradient: f64,
    clipped_highlight_fraction: f64,
    luma_histogram: Vec<f64>,
}

pub(crate) fn simulation_view_visual_compare(
    manifest_path: &Path,
    output_path: &Path,
) -> Result<()> {
    let manifest_bytes = fs::read(manifest_path).with_context(|| {
        format!(
            "reading Simulation View comparison manifest {}",
            manifest_path.display()
        )
    })?;
    let manifest: RenderComparisonManifest = serde_json::from_slice(&manifest_bytes)
        .context("Simulation View comparison manifest is not valid JSON")?;
    validate_manifest(&manifest)?;
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let pairs = manifest
        .pairs
        .into_iter()
        .map(|pair| compare_pair(root, pair))
        .collect::<Result<Vec<_>>>()?;
    let evidence = RenderComparisonEvidence {
        schema: EVIDENCE_SCHEMA,
        comparison_id: manifest.comparison_id,
        human_review_passed: manifest.human_review_passed,
        thresholds: ComparisonThresholds {
            required_width: REQUIRED_WIDTH,
            required_height: REQUIRED_HEIGHT,
            minimum_pairs_per_camera: MINIMUM_PAIRS_PER_CAMERA,
            maximum_mean_luma_delta: MAXIMUM_MEAN_LUMA_DELTA,
            maximum_contrast_delta: MAXIMUM_CONTRAST_DELTA,
            maximum_histogram_distance: MAXIMUM_HISTOGRAM_DISTANCE,
            minimum_contrast_ratio: MINIMUM_CONTRAST_RATIO,
            minimum_detail_ratio: MINIMUM_DETAIL_RATIO,
            minimum_dynamic_range_ratio: MINIMUM_DYNAMIC_RANGE_RATIO,
            maximum_added_highlight_fraction: MAXIMUM_ADDED_HIGHLIGHT_FRACTION,
            maximum_highlight_fraction: MAXIMUM_HIGHLIGHT_FRACTION,
        },
        pairs,
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "creating comparison evidence directory {}",
                parent.display()
            )
        })?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .with_context(|| {
            format!(
                "creating immutable comparison evidence {}",
                output_path.display()
            )
        })?;
    serde_json::to_writer_pretty(&mut output, &evidence)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    println!(
        "Simulation View render comparison evidence: {}",
        output_path.display()
    );
    Ok(())
}

fn validate_manifest(manifest: &RenderComparisonManifest) -> Result<()> {
    ensure!(
        manifest.schema == MANIFEST_SCHEMA,
        "Simulation View comparison schema is unsupported"
    );
    ensure!(
        valid_identity(&manifest.comparison_id),
        "comparisonId is not a valid identity"
    );
    ensure!(
        manifest.human_review_passed,
        "paired native and Simulation View captures require completed human visual review"
    );
    let mut identities = BTreeSet::new();
    let mut counts = BTreeMap::<ComparisonCameraKind, usize>::new();
    for pair in &manifest.pairs {
        ensure!(
            valid_identity(&pair.pair_id) && identities.insert(pair.pair_id.clone()),
            "comparison pair identities must be valid and unique"
        );
        validate_sha256(&pair.pose_snapshot_digest, "pose snapshot")?;
        validate_sha256(&pair.camera_transform_digest, "camera transform")?;
        *counts.entry(pair.camera_kind).or_default() += 1;
    }
    for kind in ComparisonCameraKind::ALL {
        ensure!(
            counts.get(&kind).copied().unwrap_or_default() >= MINIMUM_PAIRS_PER_CAMERA,
            "comparison requires at least {MINIMUM_PAIRS_PER_CAMERA} stable pairs for {kind:?}"
        );
    }
    Ok(())
}

fn compare_pair(root: &Path, pair: RenderComparisonPair) -> Result<PairEvidence> {
    let native_path = resolve_image(root, &pair.native_image)?;
    let simulation_path = resolve_image(root, &pair.simulation_view_image)?;
    let (native_sha256, native_pixels) = read_image(&native_path)?;
    let (simulation_sha256, simulation_pixels) = read_image(&simulation_path)?;
    ensure!(
        native_pixels.dimensions() == (REQUIRED_WIDTH, REQUIRED_HEIGHT)
            && simulation_pixels.dimensions() == (REQUIRED_WIDTH, REQUIRED_HEIGHT),
        "comparison pair {} must contain two {REQUIRED_WIDTH}x{REQUIRED_HEIGHT} frames",
        pair.pair_id
    );
    let native = measure_frame(&native_pixels);
    let simulation_view = measure_frame(&simulation_pixels);
    let mean_luma_delta = (simulation_view.mean_luma - native.mean_luma).abs();
    let contrast_delta =
        (simulation_view.luma_standard_deviation - native.luma_standard_deviation).abs();
    let histogram_distance = native
        .luma_histogram
        .iter()
        .zip(&simulation_view.luma_histogram)
        .map(|(left, right)| (left - right).abs())
        .sum::<f64>()
        / 2.0;
    let contrast_ratio = ratio(
        simulation_view.luma_standard_deviation,
        native.luma_standard_deviation,
    );
    let detail_ratio = ratio(
        simulation_view.mean_luma_gradient,
        native.mean_luma_gradient,
    );
    let dynamic_range_ratio = ratio(
        f64::from(simulation_view.dynamic_range),
        f64::from(native.dynamic_range),
    );
    let added_highlight_fraction =
        (simulation_view.clipped_highlight_fraction - native.clipped_highlight_fraction).max(0.0);
    ensure!(
        mean_luma_delta <= MAXIMUM_MEAN_LUMA_DELTA
            && contrast_delta <= MAXIMUM_CONTRAST_DELTA
            && histogram_distance <= MAXIMUM_HISTOGRAM_DISTANCE
            && contrast_ratio >= MINIMUM_CONTRAST_RATIO
            && detail_ratio >= MINIMUM_DETAIL_RATIO
            && dynamic_range_ratio >= MINIMUM_DYNAMIC_RANGE_RATIO
            && added_highlight_fraction <= MAXIMUM_ADDED_HIGHLIGHT_FRACTION
            && simulation_view.clipped_highlight_fraction <= MAXIMUM_HIGHLIGHT_FRACTION,
        "comparison pair {} exceeds the native-camera fidelity bounds: \
         mean_delta={mean_luma_delta:.3} contrast_delta={contrast_delta:.3} \
         histogram={histogram_distance:.3} contrast_ratio={contrast_ratio:.3} \
         detail_ratio={detail_ratio:.3} dynamic_range_ratio={dynamic_range_ratio:.3} \
         highlight={:.5} added_highlight={added_highlight_fraction:.5}",
        pair.pair_id,
        simulation_view.clipped_highlight_fraction,
    );
    Ok(PairEvidence {
        pair_id: pair.pair_id,
        camera_kind: pair.camera_kind,
        pose_snapshot_digest: pair.pose_snapshot_digest,
        camera_transform_digest: pair.camera_transform_digest,
        native_image_sha256: native_sha256,
        simulation_view_image_sha256: simulation_sha256,
        native,
        simulation_view,
        mean_luma_delta,
        contrast_delta,
        histogram_distance,
        contrast_ratio,
        detail_ratio,
        dynamic_range_ratio,
        added_highlight_fraction,
    })
}

fn resolve_image(root: &Path, path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    ensure!(
        path.is_file() && !path.is_symlink(),
        "comparison image must be a materialized regular file"
    );
    Ok(path)
}

fn read_image(path: &Path) -> Result<(String, RgbImage)> {
    let bytes =
        fs::read(path).with_context(|| format!("reading comparison image {}", path.display()))?;
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    let pixels = image::load_from_memory(&bytes)
        .with_context(|| format!("decoding comparison image {}", path.display()))?
        .to_rgb8();
    Ok((digest, pixels))
}

fn measure_frame(pixels: &RgbImage) -> FrameMetrics {
    let mut histogram = [0_u64; 256];
    let mut sum = 0.0;
    let mut squared_sum = 0.0;
    let mut gradient_sum = 0.0;
    let mut gradient_count = 0_u64;
    let mut previous_row = vec![0.0; pixels.width() as usize];
    for y in 0..pixels.height() {
        let mut previous = None;
        for x in 0..pixels.width() {
            let [red, green, blue] = pixels.get_pixel(x, y).0;
            let luma =
                0.2126 * f64::from(red) + 0.7152 * f64::from(green) + 0.0722 * f64::from(blue);
            let bin = luma.round().clamp(0.0, 255.0) as usize;
            histogram[bin] += 1;
            sum += luma;
            squared_sum += luma * luma;
            if let Some(left) = previous {
                gradient_sum += f64::abs(luma - left);
                gradient_count += 1;
            }
            if y > 0 {
                gradient_sum += f64::abs(luma - previous_row[x as usize]);
                gradient_count += 1;
            }
            previous = Some(luma);
            previous_row[x as usize] = luma;
        }
    }
    let count = u64::from(pixels.width()) * u64::from(pixels.height());
    let count_f64 = count as f64;
    let mean = sum / count_f64;
    let variance = (squared_sum / count_f64 - mean * mean).max(0.0);
    let p05 = percentile(&histogram, count, 5);
    let p95 = percentile(&histogram, count, 95);
    FrameMetrics {
        mean_luma: mean,
        luma_standard_deviation: variance.sqrt(),
        p05_luma: p05,
        p95_luma: p95,
        dynamic_range: p95.saturating_sub(p05),
        mean_luma_gradient: gradient_sum / gradient_count as f64,
        clipped_highlight_fraction: histogram[250..].iter().sum::<u64>() as f64 / count_f64,
        luma_histogram: histogram
            .into_iter()
            .map(|value| value as f64 / count_f64)
            .collect(),
    }
}

fn percentile(histogram: &[u64; 256], count: u64, percentile: u64) -> u8 {
    let target = (count * percentile).div_ceil(100);
    let mut observed = 0_u64;
    for (index, value) in histogram.iter().enumerate() {
        observed += value;
        if observed >= target {
            return index as u8;
        }
    }
    255
}

fn ratio(value: f64, reference: f64) -> f64 {
    if reference <= f64::EPSILON {
        if value <= f64::EPSILON {
            1.0
        } else {
            f64::INFINITY
        }
    } else {
        value / reference
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    let hexadecimal = value.strip_prefix("sha256:").unwrap_or_default();
    ensure!(
        hexadecimal.len() == 64
            && hexadecimal
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "{label} digest must be lowercase SHA-256"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_metrics_accept_equivalent_frames_and_reject_washout() {
        let native = fixture_frame(false);
        let equivalent = fixture_frame(false);
        let washed = fixture_frame(true);
        let native_metrics = measure_frame(&native);
        let equivalent_metrics = measure_frame(&equivalent);
        let washed_metrics = measure_frame(&washed);
        assert!(
            (native_metrics.mean_luma - equivalent_metrics.mean_luma).abs()
                <= MAXIMUM_MEAN_LUMA_DELTA
        );
        assert!(
            (native_metrics.mean_luma - washed_metrics.mean_luma).abs() > MAXIMUM_MEAN_LUMA_DELTA
                || ratio(
                    washed_metrics.luma_standard_deviation,
                    native_metrics.luma_standard_deviation,
                ) < MINIMUM_CONTRAST_RATIO
        );
    }

    #[test]
    fn comparison_command_writes_path_free_immutable_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let native_path = directory.path().join("native.png");
        let simulation_path = directory.path().join("simulation.png");
        fixture_frame_at_required_size(false)
            .save(&native_path)
            .unwrap();
        fixture_frame_at_required_size(false)
            .save(&simulation_path)
            .unwrap();
        let manifest_path = directory.path().join("manifest.json");
        let output_path = directory.path().join("evidence.json");
        let mut pairs = Vec::new();
        for camera in ["mounted_entity", "chase_entity", "formation_overview"] {
            for index in 0..MINIMUM_PAIRS_PER_CAMERA {
                pairs.push(serde_json::json!({
                    "pairId": format!("{camera}-{index}"),
                    "cameraKind": camera,
                    "poseSnapshotDigest": format!("sha256:{}", "1".repeat(64)),
                    "cameraTransformDigest": format!("sha256:{}", "2".repeat(64)),
                    "nativeImage": "native.png",
                    "simulationViewImage": "simulation.png"
                }));
            }
        }
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": MANIFEST_SCHEMA,
                "comparisonId": "anonymous-render-comparison",
                "humanReviewPassed": true,
                "pairs": pairs
            }))
            .unwrap(),
        )
        .unwrap();

        simulation_view_visual_compare(&manifest_path, &output_path).unwrap();

        let evidence = fs::read_to_string(&output_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&evidence).unwrap();
        assert_eq!(value["schema"], EVIDENCE_SCHEMA);
        assert_eq!(value["pairs"].as_array().unwrap().len(), 15);
        assert!(!evidence.contains(directory.path().to_str().unwrap()));
        assert!(simulation_view_visual_compare(&manifest_path, &output_path).is_err());
    }

    fn fixture_frame(washed: bool) -> RgbImage {
        RgbImage::from_fn(320, 180, |x, y| {
            let detail = ((x * 13 + y * 7) % 128) as u8;
            if washed {
                image::Rgb([210 + detail / 8, 218 + detail / 10, 205 + detail / 12])
            } else {
                image::Rgb([30 + detail, 42 + detail / 2, 25 + detail / 3])
            }
        })
    }

    fn fixture_frame_at_required_size(washed: bool) -> RgbImage {
        RgbImage::from_fn(REQUIRED_WIDTH, REQUIRED_HEIGHT, |x, y| {
            let detail = ((x * 13 + y * 7) % 128) as u8;
            if washed {
                image::Rgb([210 + detail / 8, 218 + detail / 10, 205 + detail / 12])
            } else {
                image::Rgb([30 + detail, 42 + detail / 2, 25 + detail / 3])
            }
        })
    }
}
