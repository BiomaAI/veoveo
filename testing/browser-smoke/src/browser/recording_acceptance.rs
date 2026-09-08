use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::Cdp;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordingPlaybackNetworkEvidence {
    playback_mode: RecordingPlaybackMode,
    manifest_responses: usize,
    redap_responses: usize,
    live_responses: usize,
    completed_live_responses: usize,
    blueprint_responses: usize,
    legacy_archive_requests: usize,
    canceled_playback_requests: usize,
    cancellations: Vec<PlaybackRequestIssue>,
    failed_playback_requests: usize,
    failures: Vec<PlaybackRequestIssue>,
    redap_paths: Vec<String>,
    successful_redap_paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecordingPlaybackMode {
    Live,
    Archive,
}

impl RecordingPlaybackNetworkEvidence {
    fn validate(&self) -> Result<()> {
        let transport_is_exclusive = match self.playback_mode {
            RecordingPlaybackMode::Live => {
                self.live_responses > 0
                    && self
                        .redap_paths
                        .iter()
                        .all(|path| rerun_live_control_plane_probe(path))
            }
            RecordingPlaybackMode::Archive => {
                self.live_responses == 0
                    && self.redap_responses > 0
                    && required_redap_paths_succeeded(&self.successful_redap_paths)
            }
        };
        ensure!(
            self.manifest_responses > 0
                && self.blueprint_responses > 0
                && (matches!(self.playback_mode, RecordingPlaybackMode::Archive)
                    || (self.live_responses >= 1 && self.completed_live_responses == 0))
                && self.legacy_archive_requests == 0
                && self.failed_playback_requests == 0
                && transport_is_exclusive,
            "Console did not complete exclusive governed {:?} Rerun playback: {self:?}",
            self.playback_mode
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackRequestIssue {
    path: String,
    status: Option<u16>,
    error_text: Option<String>,
    canceled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ElementBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RerunRenderEvidence {
    sample_width: u32,
    sample_height: u32,
    sampled_pixels: u64,
    unique_quantized_colors: usize,
    dominant_color_ratio: f64,
    luminance_standard_deviation: f64,
    chromatic_pixel_ratio: f64,
    edge_ratio: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RerunCameraRenderEvidence {
    render: RerunRenderEvidence,
    changed_pixel_ratio: f64,
}

impl RerunCameraRenderEvidence {
    pub(super) fn validate(&self) -> Result<()> {
        self.render.validate_content(240, 120)?;
        ensure!(
            self.changed_pixel_ratio >= 0.005,
            "Rerun leader camera did not visibly advance: {self:?}"
        );
        Ok(())
    }
}

impl RerunRenderEvidence {
    pub(super) fn validate(&self) -> Result<()> {
        self.validate_content(320, 240)
    }

    fn validate_content(&self, minimum_width: u32, minimum_height: u32) -> Result<()> {
        ensure!(
            self.sample_width >= minimum_width
                && self.sample_height >= minimum_height
                && self.sampled_pixels >= 10_000,
            "Rerun render sample is too small to establish visual evidence: {self:?}"
        );
        ensure!(
            self.unique_quantized_colors >= 4
                && self.dominant_color_ratio <= 0.95
                && self.luminance_standard_deviation >= 1.0
                && self.chromatic_pixel_ratio >= 0.0005
                && self.edge_ratio >= 0.0002,
            "Rerun viewport is blank or still showing a loading surface: {self:?}"
        );
        Ok(())
    }
}

pub(super) fn analyze_rerun_render(
    screenshot_path: &Path,
    viewer_bounds: ElementBounds,
) -> Result<RerunRenderEvidence> {
    let screenshot = fs::read(screenshot_path)
        .with_context(|| format!("reading Rerun screenshot {}", screenshot_path.display()))?;
    let pixels = image::load_from_memory(&screenshot)
        .context("decoding Rerun screenshot for visual evidence")?
        .to_rgb8();
    analyze_rerun_pixels(&pixels, viewer_bounds, [0.24, 0.07, 0.96, 0.82])
}

pub(super) fn analyze_rerun_camera_render(
    before_path: &Path,
    after_path: &Path,
    viewer_bounds: ElementBounds,
) -> Result<RerunCameraRenderEvidence> {
    let before =
        image::load_from_memory(&fs::read(before_path).with_context(|| {
            format!("reading Rerun camera screenshot {}", before_path.display())
        })?)
        .context("decoding initial Rerun camera screenshot")?
        .to_rgb8();
    let after =
        image::load_from_memory(&fs::read(after_path).with_context(|| {
            format!("reading Rerun camera screenshot {}", after_path.display())
        })?)
        .context("decoding final Rerun camera screenshot")?
        .to_rgb8();
    ensure!(
        before.dimensions() == after.dimensions(),
        "Rerun camera screenshots have different dimensions"
    );
    let region = [0.63, 0.15, 0.96, 0.34];
    let render = analyze_rerun_pixels(&after, viewer_bounds, region)?;
    let (x_start, y_start, x_end, y_end) = sample_bounds(&after, viewer_bounds, region)?;
    let mut compared = 0_u64;
    let mut changed = 0_u64;
    for y in (y_start..y_end).step_by(2) {
        for x in (x_start..x_end).step_by(2) {
            let first = before.get_pixel(x, y).0;
            let second = after.get_pixel(x, y).0;
            compared += 1;
            if first
                .into_iter()
                .zip(second)
                .any(|(left, right)| left.abs_diff(right) >= 5)
            {
                changed += 1;
            }
        }
    }
    ensure!(compared > 0, "Rerun camera pane yielded no visual samples");
    Ok(RerunCameraRenderEvidence {
        render,
        changed_pixel_ratio: changed as f64 / compared as f64,
    })
}

#[allow(dead_code)]
pub(super) fn analyze_rerun_camera_frame(
    screenshot_path: &Path,
    viewer_bounds: ElementBounds,
) -> Result<RerunRenderEvidence> {
    let screenshot = fs::read(screenshot_path).with_context(|| {
        format!(
            "reading archived Rerun camera screenshot {}",
            screenshot_path.display()
        )
    })?;
    let pixels = image::load_from_memory(&screenshot)
        .context("decoding archived Rerun camera screenshot")?
        .to_rgb8();
    let render = analyze_rerun_pixels(&pixels, viewer_bounds, [0.63, 0.15, 0.96, 0.34])?;
    ensure!(
        render.sample_width >= 240
            && render.sample_height >= 120
            && render.sampled_pixels >= 10_000
            && render.unique_quantized_colors >= 4
            && render.dominant_color_ratio <= 0.995
            && render.luminance_standard_deviation >= 1.0,
        "archived Rerun camera frame is blank or still loading: {render:?}"
    );
    Ok(render)
}

fn analyze_rerun_pixels(
    pixels: &image::RgbImage,
    viewer_bounds: ElementBounds,
    region: [f64; 4],
) -> Result<RerunRenderEvidence> {
    ensure!(
        viewer_bounds.x.is_finite()
            && viewer_bounds.y.is_finite()
            && viewer_bounds.width.is_finite()
            && viewer_bounds.height.is_finite()
            && viewer_bounds.viewport_width.is_finite()
            && viewer_bounds.viewport_height.is_finite()
            && viewer_bounds.width > 0.0
            && viewer_bounds.height > 0.0
            && viewer_bounds.viewport_width > 0.0
            && viewer_bounds.viewport_height > 0.0,
        "Rerun viewport bounds are invalid: {viewer_bounds:?}"
    );

    let (x_start, y_start, x_end, y_end) = sample_bounds(pixels, viewer_bounds, region)?;

    let mut colors = BTreeMap::<u16, u64>::new();
    let mut sampled_pixels = 0_u64;
    let mut chromatic_pixels = 0_u64;
    let mut edge_pixels = 0_u64;
    let mut edge_comparisons = 0_u64;
    let mut luminance_sum = 0.0;
    let mut luminance_squared_sum = 0.0;
    for y in (y_start..y_end).step_by(2) {
        let mut previous_luminance = None;
        for x in (x_start..x_end).step_by(2) {
            let [red, green, blue] = pixels.get_pixel(x, y).0;
            let luminance =
                0.2126 * f64::from(red) + 0.7152 * f64::from(green) + 0.0722 * f64::from(blue);
            luminance_sum += luminance;
            luminance_squared_sum += luminance * luminance;
            sampled_pixels += 1;
            let color =
                (u16::from(red / 16) << 8) | (u16::from(green / 16) << 4) | u16::from(blue / 16);
            *colors.entry(color).or_default() += 1;
            let minimum = red.min(green).min(blue);
            let maximum = red.max(green).max(blue);
            if maximum.saturating_sub(minimum) >= 12 {
                chromatic_pixels += 1;
            }
            if let Some(previous) = previous_luminance {
                edge_comparisons += 1;
                if f64::abs(luminance - previous) >= 8.0 {
                    edge_pixels += 1;
                }
            }
            previous_luminance = Some(luminance);
        }
    }
    ensure!(
        sampled_pixels > 0 && edge_comparisons > 0,
        "Rerun viewport yielded no visual samples"
    );
    let sampled = sampled_pixels as f64;
    let mean_luminance = luminance_sum / sampled;
    let luminance_variance =
        (luminance_squared_sum / sampled - mean_luminance * mean_luminance).max(0.0);
    let dominant_color_count = colors.values().copied().max().unwrap_or_default();
    Ok(RerunRenderEvidence {
        sample_width: x_end - x_start,
        sample_height: y_end - y_start,
        sampled_pixels,
        unique_quantized_colors: colors.len(),
        dominant_color_ratio: dominant_color_count as f64 / sampled,
        luminance_standard_deviation: luminance_variance.sqrt(),
        chromatic_pixel_ratio: chromatic_pixels as f64 / sampled,
        edge_ratio: edge_pixels as f64 / edge_comparisons as f64,
    })
}

fn sample_bounds(
    pixels: &image::RgbImage,
    viewer_bounds: ElementBounds,
    [x_start, y_start, x_end, y_end]: [f64; 4],
) -> Result<(u32, u32, u32, u32)> {
    ensure!(
        (0.0..1.0).contains(&x_start)
            && (0.0..1.0).contains(&y_start)
            && (0.0..=1.0).contains(&x_end)
            && (0.0..=1.0).contains(&y_end)
            && x_end > x_start
            && y_end > y_start,
        "Rerun sample region is invalid"
    );
    let screenshot_scale_x = f64::from(pixels.width()) / viewer_bounds.viewport_width;
    let screenshot_scale_y = f64::from(pixels.height()) / viewer_bounds.viewport_height;
    let x_start = ((viewer_bounds.x + viewer_bounds.width * x_start) * screenshot_scale_x)
        .floor()
        .clamp(0.0, f64::from(pixels.width())) as u32;
    let x_end = ((viewer_bounds.x + viewer_bounds.width * x_end) * screenshot_scale_x)
        .ceil()
        .clamp(0.0, f64::from(pixels.width())) as u32;
    let y_start = ((viewer_bounds.y + viewer_bounds.height * y_start) * screenshot_scale_y)
        .floor()
        .clamp(0.0, f64::from(pixels.height())) as u32;
    let y_end = ((viewer_bounds.y + viewer_bounds.height * y_end) * screenshot_scale_y)
        .ceil()
        .clamp(0.0, f64::from(pixels.height())) as u32;
    ensure!(
        x_end > x_start && y_end > y_start,
        "Rerun viewport does not intersect the screenshot: {viewer_bounds:?}"
    );
    Ok((x_start, y_start, x_end, y_end))
}

impl Cdp {
    pub(super) fn recording_playback_network_evidence(
        &self,
        recording_id: &str,
        playback_mode: RecordingPlaybackMode,
    ) -> Result<RecordingPlaybackNetworkEvidence> {
        let recording_prefix = format!("/console/api/recordings/{recording_id}/");
        let mut requests = BTreeMap::<String, PlaybackRequest>::new();
        let mut manifest_responses = 0;
        let mut redap_responses = 0;
        let mut live_responses = 0;
        let mut completed_live_responses = BTreeSet::new();
        let mut blueprint_responses = 0;
        let mut legacy_archive_requests = BTreeSet::new();
        let mut issues = BTreeMap::<String, PlaybackRequestIssue>::new();
        let mut successful_paths = BTreeSet::new();
        let mut redap_paths = BTreeSet::new();
        let mut successful_redap_paths = BTreeSet::new();

        for event in &self.events {
            let Some(method) = event.get("method").and_then(Value::as_str) else {
                continue;
            };
            match method {
                "Network.requestWillBeSent" => {
                    let Some(request_id) =
                        event.pointer("/params/requestId").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let Some(url) = event.pointer("/params/request/url").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    if let Some((kind, path)) = playback_request_kind(url, &recording_prefix) {
                        if kind == PlaybackRequestKind::LegacyArchive {
                            legacy_archive_requests.insert(request_id.to_owned());
                        }
                        if kind == PlaybackRequestKind::Redap {
                            redap_paths.insert(path.clone());
                        }
                        requests.insert(request_id.to_owned(), PlaybackRequest { kind, path });
                    }
                }
                "Network.responseReceived" => {
                    let Some(request_id) =
                        event.pointer("/params/requestId").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let request = requests.get(request_id).cloned().or_else(|| {
                        let url = event.pointer("/params/response/url")?.as_str()?;
                        playback_request_kind(url, &recording_prefix)
                            .map(|(kind, path)| PlaybackRequest { kind, path })
                    });
                    let Some(request) = request else {
                        continue;
                    };
                    let status = event
                        .pointer("/params/response/status")
                        .and_then(Value::as_f64)
                        .unwrap_or_default();
                    if !(200.0..300.0).contains(&status) {
                        record_playback_issue(
                            &mut issues,
                            request_id,
                            &request.path,
                            Some(status as u16),
                            None,
                            false,
                        );
                        continue;
                    }
                    successful_paths.insert(request.path.clone());
                    match request.kind {
                        PlaybackRequestKind::Manifest => manifest_responses += 1,
                        PlaybackRequestKind::Redap => {
                            redap_responses += 1;
                            successful_redap_paths.insert(request.path.clone());
                        }
                        PlaybackRequestKind::Live => live_responses += 1,
                        PlaybackRequestKind::Blueprint => blueprint_responses += 1,
                        PlaybackRequestKind::LegacyArchive => {}
                    }
                }
                "Network.loadingFailed" => {
                    let Some(request_id) =
                        event.pointer("/params/requestId").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let Some(request) = requests.get(request_id) else {
                        continue;
                    };
                    record_playback_issue(
                        &mut issues,
                        request_id,
                        &request.path,
                        None,
                        event.pointer("/params/errorText").and_then(Value::as_str),
                        event
                            .pointer("/params/canceled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    );
                }
                "Network.loadingFinished" => {
                    let Some(request_id) =
                        event.pointer("/params/requestId").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    if requests
                        .get(request_id)
                        .is_some_and(|request| request.kind == PlaybackRequestKind::Live)
                    {
                        completed_live_responses.insert(request_id.to_owned());
                    }
                }
                _ => {}
            }
        }

        let (cancellations, failures) =
            classify_playback_issues(issues.into_values(), &successful_paths);
        let evidence = RecordingPlaybackNetworkEvidence {
            playback_mode,
            manifest_responses,
            redap_responses,
            live_responses,
            completed_live_responses: completed_live_responses.len(),
            blueprint_responses,
            legacy_archive_requests: legacy_archive_requests.len(),
            canceled_playback_requests: cancellations.len(),
            cancellations,
            failed_playback_requests: failures.len(),
            failures,
            redap_paths: redap_paths.into_iter().collect(),
            successful_redap_paths: successful_redap_paths.into_iter().collect(),
        };
        evidence.validate()?;
        Ok(evidence)
    }
}

#[derive(Clone, Debug)]
struct PlaybackRequest {
    kind: PlaybackRequestKind,
    path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaybackRequestKind {
    Manifest,
    Redap,
    Live,
    Blueprint,
    LegacyArchive,
}

fn playback_request_kind(
    value: &str,
    recording_prefix: &str,
) -> Option<(PlaybackRequestKind, String)> {
    let url = Url::parse(value).ok()?;
    let path = url.path().to_owned();
    if path == format!("{recording_prefix}playback") {
        return Some((PlaybackRequestKind::Manifest, path));
    }
    if path.contains("/rerun.cloud.v1alpha1.RerunCloudService") {
        return Some((PlaybackRequestKind::Redap, path));
    }
    if path.starts_with(recording_prefix) && path.ends_with("/live/rrd-stream") {
        return Some((PlaybackRequestKind::Live, path));
    }
    if path == "/rerun.sdk_comms.v1alpha1.MessageProxyService/ReadMessages"
        || (path.starts_with(recording_prefix) && path.ends_with("/live/proxy"))
    {
        return Some((PlaybackRequestKind::LegacyArchive, path));
    }
    if path.starts_with(recording_prefix)
        && path.contains("/blueprints/")
        && path.ends_with("/data.rrd")
    {
        return Some((PlaybackRequestKind::Blueprint, path));
    }
    if path.starts_with(recording_prefix)
        && (path.contains("/playback-sessions/")
            || path.contains("/archive/")
            || path.ends_with("/data.rrd"))
    {
        return Some((PlaybackRequestKind::LegacyArchive, path));
    }
    None
}

fn record_playback_issue(
    issues: &mut BTreeMap<String, PlaybackRequestIssue>,
    request_id: &str,
    path: &str,
    status: Option<u16>,
    error_text: Option<&str>,
    canceled: bool,
) {
    let issue = issues
        .entry(request_id.to_owned())
        .or_insert_with(|| PlaybackRequestIssue {
            path: path.to_owned(),
            status: None,
            error_text: None,
            canceled: false,
        });
    issue.status = status.or(issue.status);
    if let Some(error_text) = error_text {
        issue.error_text = Some(error_text.to_owned());
    }
    issue.canceled |= canceled;
}

fn classify_playback_issues(
    issues: impl IntoIterator<Item = PlaybackRequestIssue>,
    successful_paths: &BTreeSet<String>,
) -> (Vec<PlaybackRequestIssue>, Vec<PlaybackRequestIssue>) {
    let mut cancellations = Vec::new();
    let mut failures = Vec::new();
    for issue in issues {
        let is_superseded_cancellation = issue.status.is_none()
            && issue.canceled
            && issue.error_text.as_deref() == Some("net::ERR_ABORTED")
            && successful_paths.contains(&issue.path);
        if is_superseded_cancellation {
            cancellations.push(issue);
        } else {
            failures.push(issue);
        }
    }
    (cancellations, failures)
}

fn required_redap_paths_succeeded(paths: &[String]) -> bool {
    [
        "/WhoAmI",
        "/FindEntries",
        "/ReadDatasetEntry",
        "/GetRrdManifest",
        "/GetSegmentTableSchema",
    ]
    .into_iter()
    .all(|required| paths.iter().any(|path| path.ends_with(required)))
}

fn rerun_live_control_plane_probe(path: &str) -> bool {
    path.ends_with("/WhoAmI") || path.ends_with("/Version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_network_paths_distinguish_redap_from_legacy_shards() {
        let prefix = "/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/";
        let kind = |url| playback_request_kind(url, prefix).map(|(kind, _)| kind);
        assert_eq!(
            kind(
                "https://installation.example/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/playback"
            ),
            Some(PlaybackRequestKind::Manifest)
        );
        assert_eq!(
            kind("https://installation.example/rerun.cloud.v1alpha1.RerunCloudService/Query"),
            Some(PlaybackRequestKind::Redap)
        );
        assert_eq!(
            kind(
                "https://installation.example/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/live/rrd-stream"
            ),
            Some(PlaybackRequestKind::Live)
        );
        assert_eq!(
            kind(
                "https://installation.example/rerun.sdk_comms.v1alpha1.MessageProxyService/ReadMessages"
            ),
            Some(PlaybackRequestKind::LegacyArchive)
        );
        assert_eq!(
            kind(
                "https://installation.example/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/blueprints/1/data.rrd"
            ),
            Some(PlaybackRequestKind::Blueprint)
        );
        assert_eq!(
            kind(
                "https://installation.example/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/playback-sessions/session/segments/shard/data.rrd"
            ),
            Some(PlaybackRequestKind::LegacyArchive)
        );
        assert_eq!(
            kind("https://installation.example/console/assets/re_viewer.wasm"),
            None
        );
    }

    #[test]
    fn one_failed_request_is_not_double_counted_by_response_and_loading_events() {
        let mut issues = BTreeMap::new();
        record_playback_issue(
            &mut issues,
            "request-1",
            "/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/live/rrd-stream",
            Some(401),
            None,
            false,
        );
        record_playback_issue(
            &mut issues,
            "request-1",
            "/console/api/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/live/rrd-stream",
            None,
            Some("net::ERR_ABORTED"),
            false,
        );

        let failure = issues.get("request-1").unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(failure.status, Some(401));
        assert_eq!(failure.error_text.as_deref(), Some("net::ERR_ABORTED"));
    }

    #[test]
    fn superseded_abort_is_evidenced_only_after_the_same_path_succeeds() {
        let path = "/rerun.cloud.v1alpha1.RerunCloudService/GetRrdManifest";
        let issue = PlaybackRequestIssue {
            path: path.to_owned(),
            status: None,
            error_text: Some("net::ERR_ABORTED".to_owned()),
            canceled: true,
        };

        let (cancellations, failures) =
            classify_playback_issues([issue.clone()], &BTreeSet::from([path.to_owned()]));
        assert_eq!(cancellations.len(), 1);
        assert!(failures.is_empty());

        let (cancellations, failures) = classify_playback_issues([issue], &BTreeSet::new());
        assert!(cancellations.is_empty());
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn scoped_lazy_redap_requires_every_core_success_path() {
        let paths = [
            "WhoAmI",
            "FindEntries",
            "ReadDatasetEntry",
            "GetRrdManifest",
            "GetSegmentTableSchema",
        ]
        .map(|method| format!("/rerun.cloud.v1alpha1.RerunCloudService/{method}"));
        assert!(required_redap_paths_succeeded(&paths));
        assert!(!required_redap_paths_succeeded(&paths[..4]));
    }

    #[test]
    fn live_playback_requires_only_the_live_receiver() {
        let evidence = RecordingPlaybackNetworkEvidence {
            playback_mode: RecordingPlaybackMode::Live,
            manifest_responses: 1,
            redap_responses: 0,
            live_responses: 1,
            completed_live_responses: 0,
            blueprint_responses: 1,
            legacy_archive_requests: 0,
            canceled_playback_requests: 0,
            cancellations: Vec::new(),
            failed_playback_requests: 0,
            failures: Vec::new(),
            redap_paths: Vec::new(),
            successful_redap_paths: Vec::new(),
        };
        evidence.validate().unwrap();

        let control_plane_probe = RecordingPlaybackNetworkEvidence {
            redap_responses: 2,
            redap_paths: vec![
                "/rerun.cloud.v1alpha1.RerunCloudService/WhoAmI".to_owned(),
                "/rerun.cloud.v1alpha1.RerunCloudService/Version".to_owned(),
            ],
            ..evidence.clone()
        };
        control_plane_probe.validate().unwrap();

        let mixed = RecordingPlaybackNetworkEvidence {
            redap_responses: 1,
            redap_paths: vec![
                "/rerun.cloud.v1alpha1.RerunCloudService/ReadDatasetEntry".to_owned(),
            ],
            ..evidence.clone()
        };
        assert!(mixed.validate().is_err());

        let completed = RecordingPlaybackNetworkEvidence {
            completed_live_responses: 1,
            ..evidence
        };
        assert!(completed.validate().is_err());
    }

    #[test]
    fn archive_playback_requires_the_complete_redap_read_subset() {
        let successful_redap_paths = [
            "WhoAmI",
            "FindEntries",
            "ReadDatasetEntry",
            "GetRrdManifest",
            "GetSegmentTableSchema",
        ]
        .map(|method| format!("/rerun.cloud.v1alpha1.RerunCloudService/{method}"))
        .to_vec();
        let evidence = RecordingPlaybackNetworkEvidence {
            playback_mode: RecordingPlaybackMode::Archive,
            manifest_responses: 1,
            redap_responses: successful_redap_paths.len(),
            live_responses: 0,
            completed_live_responses: 0,
            blueprint_responses: 1,
            legacy_archive_requests: 0,
            canceled_playback_requests: 0,
            cancellations: Vec::new(),
            failed_playback_requests: 0,
            failures: Vec::new(),
            redap_paths: successful_redap_paths.clone(),
            successful_redap_paths,
        };
        evidence.validate().unwrap();

        let mixed = RecordingPlaybackNetworkEvidence {
            live_responses: 1,
            ..evidence
        };
        assert!(mixed.validate().is_err());
    }

    #[test]
    fn rerun_render_evidence_rejects_uniform_loading_surfaces() {
        let bounds = ElementBounds {
            x: 0.0,
            y: 0.0,
            width: 1_000.0,
            height: 700.0,
            viewport_width: 1_000.0,
            viewport_height: 700.0,
        };
        let blank = image::RgbImage::from_pixel(1_000, 700, image::Rgb([15, 17, 18]));
        assert!(
            analyze_rerun_pixels(&blank, bounds, [0.24, 0.07, 0.96, 0.82])
                .unwrap()
                .validate()
                .is_err()
        );

        let mut rendered = blank;
        for y in 50..600 {
            for x in 250..950 {
                if x % 80 < 4 || y % 60 < 3 {
                    let variation = ((x / 20 + y / 15) % 6) as u8 * 12;
                    let color = match (x / 80 + y / 60) % 3 {
                        0 => image::Rgb([48 + variation, 170, 130]),
                        1 => image::Rgb([130, 100 + variation, 210]),
                        _ => image::Rgb([195, 155, 60 + variation]),
                    };
                    rendered.put_pixel(x, y, color);
                }
            }
        }
        analyze_rerun_pixels(&rendered, bounds, [0.24, 0.07, 0.96, 0.82])
            .unwrap()
            .validate()
            .unwrap();
    }

    #[test]
    fn rerun_render_evidence_accepts_sparse_spatial_views() {
        RerunRenderEvidence {
            sample_width: 420,
            sample_height: 240,
            sampled_pixels: 25_200,
            unique_quantized_colors: 6,
            dominant_color_ratio: 0.79,
            luminance_standard_deviation: 1.6,
            chromatic_pixel_ratio: 1.0,
            edge_ratio: 0.00036,
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn rerun_render_evidence_scales_css_bounds_to_screenshot_pixels() {
        let bounds = ElementBounds {
            x: 100.0,
            y: 50.0,
            width: 800.0,
            height: 600.0,
            viewport_width: 1_000.0,
            viewport_height: 700.0,
        };
        let pixels = image::RgbImage::from_pixel(2_000, 1_400, image::Rgb([15, 17, 18]));
        let sampled = sample_bounds(&pixels, bounds, [0.25, 0.25, 0.75, 0.75]).unwrap();
        assert_eq!(sampled, (600, 400, 1_400, 1_000));
    }
}
