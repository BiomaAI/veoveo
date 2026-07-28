use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contract::{
    LiveIngressView, LiveTransport, LiveVideoView, ModelFormat, ModelView, PerceptionOperation,
    PipelineProfile, PipelineView,
};
use crate::uris;

const MAX_LAUNCH_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogDocument {
    models: Vec<ModelConfig>,
    pipelines: Vec<PipelineConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub id: String,
    pub title: String,
    pub description: String,
    pub format: ModelFormat,
    pub model_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub id: String,
    pub title: String,
    pub description: String,
    pub profile: PipelineProfileConfig,
    #[serde(default)]
    pub recording_replay: Option<GStreamerGraphConfig>,
    #[serde(default)]
    pub live: Option<LivePipelineConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PipelineProfileConfig {
    PassThrough,
    Perception {
        operation: PerceptionOperation,
        model_id: String,
        inference_config_path: PathBuf,
        #[serde(default)]
        tracker: Option<TrackerConfig>,
    },
}

impl PipelineProfileConfig {
    pub fn public_profile(&self) -> PipelineProfile {
        match self {
            Self::PassThrough => PipelineProfile::PassThrough,
            Self::Perception {
                operation, tracker, ..
            } => PipelineProfile::Perception {
                operation: *operation,
                tracking: tracker.is_some(),
            },
        }
    }

    pub fn model_id(&self) -> Option<&str> {
        match self {
            Self::PassThrough => None,
            Self::Perception { model_id, .. } => Some(model_id),
        }
    }

    pub fn perception(&self) -> Option<PerceptionProfile<'_>> {
        match self {
            Self::PassThrough => None,
            Self::Perception {
                operation,
                model_id,
                inference_config_path,
                tracker,
            } => Some(PerceptionProfile {
                operation: *operation,
                model_id,
                inference_config_path,
                tracker: tracker.as_ref(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PerceptionProfile<'a> {
    pub operation: PerceptionOperation,
    pub model_id: &'a str,
    pub inference_config_path: &'a Path,
    pub tracker: Option<&'a TrackerConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GStreamerGraphConfig {
    pub launch: String,
    #[serde(default)]
    pub source_element: Option<String>,
    #[serde(default)]
    pub stream_muxer_element: Option<String>,
    #[serde(default)]
    pub inference_element: Option<String>,
    #[serde(default)]
    pub tracker_element: Option<String>,
    #[serde(default)]
    pub results_element: Option<String>,
    #[serde(default)]
    pub encoded_output_element: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivePipelineConfig {
    pub input_width: u16,
    pub input_height: u16,
    pub codec: String,
    pub frame_rate: u16,
    pub expected_bitrate_bps: u32,
    pub ingress: RtpH264UdpIngress,
    pub graph: GStreamerGraphConfig,
    #[serde(default)]
    pub recording_output: Option<RecordingOutputConfig>,
}

impl LivePipelineConfig {
    pub fn video_view(&self) -> LiveVideoView {
        LiveVideoView {
            codec: self.codec.clone(),
            width: self.input_width,
            height: self.input_height,
            frame_rate: self.frame_rate,
            expected_bitrate_bps: self.expected_bitrate_bps,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingOutputConfig {
    pub proxy_url: String,
    pub application_id: String,
    pub entity_path: String,
    #[serde(default = "default_recording_timeline")]
    pub timeline: String,
    #[serde(default = "default_recording_queue_capacity")]
    pub queue_capacity: usize,
}

fn default_recording_timeline() -> String {
    "stream-time".to_owned()
}

const fn default_recording_queue_capacity() -> usize {
    256
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtpH264UdpIngress {
    pub advertised_host: String,
    pub port: u16,
    #[serde(default = "default_payload_type")]
    pub payload_type: u8,
    #[serde(default = "default_clock_rate")]
    pub clock_rate: u32,
}

impl RtpH264UdpIngress {
    pub fn view(&self) -> LiveIngressView {
        LiveIngressView {
            transport: LiveTransport::RtpH264Udp,
            host: self.advertised_host.clone(),
            port: self.port,
            payload_type: self.payload_type,
            clock_rate: self.clock_rate,
            caps: format!(
                "application/x-rtp,media=video,encoding-name=H264,payload={},clock-rate={}",
                self.payload_type, self.clock_rate
            ),
        }
    }
}

const fn default_payload_type() -> u8 {
    96
}

const fn default_clock_rate() -> u32 {
    90_000
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackerConfig {
    pub config_path: PathBuf,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct PipelineCatalog {
    models: BTreeMap<String, ModelConfig>,
    pipelines: BTreeMap<String, PipelineConfig>,
}

impl PipelineCatalog {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading stream catalog {}", path.display()))?;
        let document: CatalogDocument = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing stream catalog {}", path.display()))?;
        let catalog = Self::new(document.models, document.pipelines)?;
        catalog.validate_runtime_files()?;
        Ok(catalog)
    }

    pub fn new(models: Vec<ModelConfig>, pipelines: Vec<PipelineConfig>) -> Result<Self> {
        let mut model_map = BTreeMap::new();
        for model in models {
            validate_id("model id", &model.id)?;
            ensure!(!model.title.trim().is_empty(), "model title is required");
            ensure!(
                model.model_path.is_absolute(),
                "model_path for `{}` must be absolute",
                model.id
            );
            let id = model.id.clone();
            ensure!(
                model_map.insert(id.clone(), model).is_none(),
                "duplicate model `{id}`"
            );
        }
        ensure!(
            !pipelines.is_empty(),
            "stream catalog requires at least one pipeline"
        );
        let mut live_ports = BTreeSet::new();
        let mut pipeline_map = BTreeMap::new();
        for pipeline in pipelines {
            validate_id("pipeline id", &pipeline.id)?;
            ensure!(
                !pipeline.title.trim().is_empty(),
                "pipeline title is required"
            );
            ensure!(
                pipeline.recording_replay.is_some() || pipeline.live.is_some(),
                "pipeline `{}` must admit recording replay, live input, or both",
                pipeline.id
            );
            validate_profile(&pipeline.profile, &model_map, &pipeline.id)?;
            if let Some(graph) = &pipeline.recording_replay {
                ensure!(
                    pipeline.profile.perception().is_some(),
                    "recording replay pipeline `{}` requires a typed perception profile",
                    pipeline.id
                );
                validate_graph(graph, &pipeline.profile, GraphMode::RecordingReplay)?;
                ensure!(
                    graph.source_element.is_some(),
                    "recording replay graph for `{}` requires source_element",
                    pipeline.id
                );
            }
            if let Some(live) = &pipeline.live {
                ensure!(
                    live.input_width > 0 && live.input_height > 0,
                    "live pipeline `{}` input dimensions must be non-zero",
                    pipeline.id
                );
                ensure!(
                    live.frame_rate > 0 && live.frame_rate <= 240,
                    "live pipeline `{}` frame_rate must be within 1..=240",
                    pipeline.id
                );
                ensure!(
                    live.expected_bitrate_bps > 0,
                    "live pipeline `{}` expected_bitrate_bps must be non-zero",
                    pipeline.id
                );
                validate_avc_codec(&live.codec, &pipeline.id)?;
                validate_ingress(&live.ingress, &pipeline.id)?;
                if let Some(recording) = &live.recording_output {
                    validate_recording_output(recording, &pipeline.id)?;
                }
                ensure!(
                    live_ports.insert(live.ingress.port),
                    "live pipeline `{}` reuses UDP port {}",
                    pipeline.id,
                    live.ingress.port
                );
                validate_graph(&live.graph, &pipeline.profile, GraphMode::Live)?;
                ensure!(
                    live.graph.encoded_output_element.is_some(),
                    "live pipeline `{}` requires encoded_output_element for its MCP App and optional recording route",
                    pipeline.id
                );
            }
            let id = pipeline.id.clone();
            ensure!(
                pipeline_map.insert(id.clone(), pipeline).is_none(),
                "duplicate pipeline `{id}`"
            );
        }
        Ok(Self {
            models: model_map,
            pipelines: pipeline_map,
        })
    }

    pub fn pipeline(&self, id: &str) -> Option<&PipelineConfig> {
        self.pipelines.get(id)
    }

    pub fn model(&self, id: &str) -> Option<&ModelConfig> {
        self.models.get(id)
    }

    pub fn pipeline_views(&self) -> Vec<PipelineView> {
        self.pipelines.values().map(pipeline_view).collect()
    }

    pub fn model_views(&self) -> Vec<ModelView> {
        self.models.values().map(model_view).collect()
    }

    pub fn pipeline_ids(&self) -> BTreeSet<String> {
        self.pipelines.keys().cloned().collect()
    }

    pub fn model_ids(&self) -> BTreeSet<String> {
        self.models.keys().cloned().collect()
    }

    fn validate_runtime_files(&self) -> Result<()> {
        for model in self.models.values() {
            ensure!(
                model.model_path.is_file(),
                "model_path for `{}` is not a regular file: {}",
                model.id,
                model.model_path.display()
            );
        }
        for pipeline in self.pipelines.values() {
            if let Some(profile) = pipeline.profile.perception() {
                ensure!(
                    profile.inference_config_path.is_file(),
                    "inference_config_path for `{}` is not a regular file: {}",
                    pipeline.id,
                    profile.inference_config_path.display()
                );
                if let Some(tracker) = profile.tracker {
                    ensure!(
                        tracker.config_path.is_file(),
                        "tracker config_path for `{}` is not a regular file: {}",
                        pipeline.id,
                        tracker.config_path.display()
                    );
                }
            }
        }
        Ok(())
    }
}

fn validate_recording_output(output: &RecordingOutputConfig, pipeline_id: &str) -> Result<()> {
    ensure!(
        output.proxy_url.starts_with("rerun+http://127.0.0.1:")
            || output.proxy_url.starts_with("rerun+http://[::1]:"),
        "live pipeline `{pipeline_id}` recording output must use a loopback Rerun proxy"
    );
    validate_id("recording application_id", &output.application_id)?;
    ensure!(
        output.entity_path.starts_with('/')
            && output.entity_path.len() <= 1_024
            && !output.entity_path.chars().any(char::is_control),
        "live pipeline `{pipeline_id}` recording entity_path must be an absolute bounded path"
    );
    validate_id("recording timeline", &output.timeline)?;
    ensure!(
        (1..=65_536).contains(&output.queue_capacity),
        "live pipeline `{pipeline_id}` recording queue_capacity must be within 1..=65536"
    );
    Ok(())
}

pub fn pipeline_view(config: &PipelineConfig) -> PipelineView {
    PipelineView {
        id: config.id.clone(),
        uri: uris::pipeline_uri(&config.id),
        title: config.title.clone(),
        description: config.description.clone(),
        profile: config.profile.public_profile(),
        model_uri: config.profile.model_id().map(uris::model_uri),
        supports_recording_replay: config.recording_replay.is_some(),
        supports_live_input: config.live.is_some(),
    }
}

pub fn model_view(config: &ModelConfig) -> ModelView {
    ModelView {
        id: config.id.clone(),
        uri: uris::model_uri(&config.id),
        title: config.title.clone(),
        description: config.description.clone(),
        format: config.format,
    }
}

fn validate_profile(
    profile: &PipelineProfileConfig,
    models: &BTreeMap<String, ModelConfig>,
    pipeline_id: &str,
) -> Result<()> {
    let Some(profile) = profile.perception() else {
        return Ok(());
    };
    ensure!(
        models.contains_key(profile.model_id),
        "pipeline `{pipeline_id}` references unknown model `{}`",
        profile.model_id
    );
    ensure!(
        profile.inference_config_path.is_absolute(),
        "inference_config_path for `{pipeline_id}` must be absolute"
    );
    match profile.operation {
        PerceptionOperation::ObjectDetection => ensure!(
            profile.tracker.is_none(),
            "pipeline `{pipeline_id}` must not configure a tracker for object_detection"
        ),
        PerceptionOperation::ObjectDetectionTracking => ensure!(
            profile.tracker.is_some(),
            "pipeline `{pipeline_id}` requires a tracker for object_detection_tracking"
        ),
        PerceptionOperation::InstanceSegmentation | PerceptionOperation::PoseEstimation => {
            bail!(
                "pipeline `{pipeline_id}` uses a perception operation not implemented by the production runner"
            )
        }
    }
    if let Some(tracker) = profile.tracker {
        ensure!(
            tracker.config_path.is_absolute(),
            "tracker config_path for `{pipeline_id}` must be absolute"
        );
        ensure!(
            tracker.width > 0
                && tracker.height > 0
                && tracker.width % 32 == 0
                && tracker.height % 32 == 0,
            "tracker dimensions for `{pipeline_id}` must be positive multiples of 32"
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum GraphMode {
    RecordingReplay,
    Live,
}

fn validate_graph(
    graph: &GStreamerGraphConfig,
    profile: &PipelineProfileConfig,
    mode: GraphMode,
) -> Result<()> {
    ensure!(
        !graph.launch.trim().is_empty() && graph.launch.len() <= MAX_LAUNCH_BYTES,
        "GStreamer launch text must contain 1..={MAX_LAUNCH_BYTES} bytes"
    );
    ensure!(
        !graph.launch.contains('\0'),
        "GStreamer launch text must not contain NUL"
    );
    for (name, value) in [
        ("source_element", graph.source_element.as_deref()),
        (
            "stream_muxer_element",
            graph.stream_muxer_element.as_deref(),
        ),
        ("inference_element", graph.inference_element.as_deref()),
        ("tracker_element", graph.tracker_element.as_deref()),
        ("results_element", graph.results_element.as_deref()),
        (
            "encoded_output_element",
            graph.encoded_output_element.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            validate_id(name, value)?;
            ensure!(
                graph.launch.contains(&format!("name={value}"))
                    || graph.launch.contains(&format!("name=\"{value}\"")),
                "GStreamer graph declares {name} `{value}` but launch text does not name it"
            );
        }
    }
    if profile.perception().is_some() {
        ensure!(
            graph.stream_muxer_element.is_some()
                && graph.inference_element.is_some()
                && graph.results_element.is_some(),
            "perception graphs require named stream muxer, inference, and results elements"
        );
    } else {
        ensure!(
            graph.stream_muxer_element.is_none()
                && graph.inference_element.is_none()
                && graph.tracker_element.is_none()
                && graph.results_element.is_none(),
            "pass-through graphs must not declare perception result elements"
        );
    }
    if matches!(mode, GraphMode::Live) {
        ensure!(
            graph.source_element.is_some(),
            "live graph requires a named source element"
        );
    }
    Ok(())
}

fn validate_ingress(ingress: &RtpH264UdpIngress, pipeline_id: &str) -> Result<()> {
    ensure!(
        !ingress.advertised_host.trim().is_empty()
            && ingress.advertised_host.len() <= 253
            && !ingress.advertised_host.contains('/')
            && !ingress.advertised_host.chars().any(char::is_whitespace),
        "live pipeline `{pipeline_id}` has an invalid advertised_host"
    );
    ensure!(
        (96..=127).contains(&ingress.payload_type),
        "live pipeline `{pipeline_id}` payload_type must be a dynamic RTP payload type"
    );
    ensure!(
        ingress.clock_rate == 90_000,
        "live pipeline `{pipeline_id}` H.264 clock_rate must be 90000"
    );
    Ok(())
}

fn validate_avc_codec(codec: &str, pipeline_id: &str) -> Result<()> {
    ensure!(
        codec.len() == 11
            && codec.starts_with("avc1.")
            && codec[5..]
                .chars()
                .all(|character| character.is_ascii_hexdigit()),
        "live pipeline `{pipeline_id}` codec must be an RFC 6381 avc1.PPCCLL value"
    );
    Ok(())
}

fn validate_id(name: &str, value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-')
            && value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric),
        "{name} must be a lowercase path-safe identifier"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelConfig {
        ModelConfig {
            id: "detector".to_owned(),
            title: "Detector".to_owned(),
            description: String::new(),
            format: ModelFormat::TensorRtEngine,
            model_path: "/models/detector.engine".into(),
        }
    }

    fn replay_graph() -> GStreamerGraphConfig {
        GStreamerGraphConfig {
            launch: "filesrc name=source ! fakesink name=results".to_owned(),
            source_element: Some("source".to_owned()),
            stream_muxer_element: Some("mux".to_owned()),
            inference_element: Some("infer".to_owned()),
            tracker_element: None,
            results_element: Some("results".to_owned()),
            encoded_output_element: None,
        }
    }

    #[test]
    fn catalog_rejects_unknown_models() {
        let error = PipelineCatalog::new(
            vec![model()],
            vec![PipelineConfig {
                id: "detect".to_owned(),
                title: "Detect".to_owned(),
                description: String::new(),
                profile: PipelineProfileConfig::Perception {
                    operation: PerceptionOperation::ObjectDetection,
                    model_id: "missing".to_owned(),
                    inference_config_path: "/etc/stream/detect.txt".into(),
                    tracker: None,
                },
                recording_replay: Some(replay_graph()),
                live: None,
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown model"));
    }

    #[test]
    fn tracking_pipeline_requires_typed_tracker_config() {
        let error = PipelineCatalog::new(
            vec![model()],
            vec![PipelineConfig {
                id: "track".to_owned(),
                title: "Track".to_owned(),
                description: String::new(),
                profile: PipelineProfileConfig::Perception {
                    operation: PerceptionOperation::ObjectDetectionTracking,
                    model_id: "detector".to_owned(),
                    inference_config_path: "/etc/stream/detect.txt".into(),
                    tracker: None,
                },
                recording_replay: Some(replay_graph()),
                live: None,
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("requires a tracker"));
    }

    #[test]
    fn live_pass_through_pipeline_needs_no_model_or_recording() {
        let catalog = PipelineCatalog::new(
            vec![],
            vec![PipelineConfig {
                id: "preview".to_owned(),
                title: "Preview".to_owned(),
                description: String::new(),
                profile: PipelineProfileConfig::PassThrough,
                recording_replay: None,
                live: Some(LivePipelineConfig {
                    input_width: 640,
                    input_height: 480,
                    codec: "avc1.42e01f".to_owned(),
                    frame_rate: 30,
                    expected_bitrate_bps: 4_000_000,
                    ingress: RtpH264UdpIngress {
                        advertised_host: "stream-mcp".to_owned(),
                        port: 9001,
                        payload_type: 97,
                        clock_rate: 90_000,
                    },
                    graph: GStreamerGraphConfig {
                        launch: "udpsrc name=source ! h264parse ! identity name=encoded-output ! fakesink".to_owned(),
                        source_element: Some("source".to_owned()),
                        stream_muxer_element: None,
                        inference_element: None,
                        tracker_element: None,
                        results_element: None,
                        encoded_output_element: Some("encoded-output".to_owned()),
                    },
                    recording_output: None,
                }),
            }],
        )
        .unwrap();
        let pipeline = catalog.pipeline_views().pop().unwrap();
        assert!(pipeline.supports_live_input);
        assert!(!pipeline.supports_recording_replay);
        assert_eq!(pipeline.profile, PipelineProfile::PassThrough);
        assert!(pipeline.model_uri.is_none());
    }

    #[test]
    fn recording_output_is_loopback_and_bounded() {
        let mut live = LivePipelineConfig {
            input_width: 640,
            input_height: 480,
            codec: "avc1.42e01f".to_owned(),
            frame_rate: 30,
            expected_bitrate_bps: 4_000_000,
            ingress: RtpH264UdpIngress {
                advertised_host: "stream-mcp".to_owned(),
                port: 9001,
                payload_type: 97,
                clock_rate: 90_000,
            },
            graph: GStreamerGraphConfig {
                launch: "udpsrc name=source ! h264parse ! identity name=encoded-output ! fakesink"
                    .to_owned(),
                source_element: Some("source".to_owned()),
                stream_muxer_element: None,
                inference_element: None,
                tracker_element: None,
                results_element: None,
                encoded_output_element: Some("encoded-output".to_owned()),
            },
            recording_output: Some(RecordingOutputConfig {
                proxy_url: "rerun+http://recording-forwarder:9876/proxy".to_owned(),
                application_id: "veoveo-sensor".to_owned(),
                entity_path: "/stream/live".to_owned(),
                timeline: "stream-time".to_owned(),
                queue_capacity: 256,
            }),
        };
        let error = PipelineCatalog::new(
            vec![],
            vec![PipelineConfig {
                id: "preview".to_owned(),
                title: "Preview".to_owned(),
                description: String::new(),
                profile: PipelineProfileConfig::PassThrough,
                recording_replay: None,
                live: Some(live.clone()),
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("loopback Rerun proxy"));

        live.recording_output.as_mut().unwrap().proxy_url =
            "rerun+http://127.0.0.1:9876/proxy".to_owned();
        live.recording_output.as_mut().unwrap().queue_capacity = 0;
        let error = PipelineCatalog::new(
            vec![],
            vec![PipelineConfig {
                id: "preview".to_owned(),
                title: "Preview".to_owned(),
                description: String::new(),
                profile: PipelineProfileConfig::PassThrough,
                recording_replay: None,
                live: Some(live),
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("queue_capacity"));
    }

    #[test]
    fn repository_catalog_example_matches_the_typed_contract() {
        let document: CatalogDocument = serde_json::from_slice(include_bytes!(
            "../../../configs/stream/catalog.example.json"
        ))
        .unwrap();
        let catalog = PipelineCatalog::new(document.models, document.pipelines).unwrap();
        assert_eq!(catalog.pipeline_ids().len(), 2);
        assert_eq!(catalog.model_ids().len(), 1);
        assert!(
            catalog
                .pipeline("detect-objects")
                .is_some_and(|pipeline| pipeline.live.is_some())
        );
    }
}
