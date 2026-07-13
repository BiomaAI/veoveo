use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::contract::{ModelFormat, ModelView, PipelineOperation, PipelineView};
use crate::uris;

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
    pub operation: PipelineOperation,
    pub model_id: String,
    pub deepstream_config_path: PathBuf,
    #[serde(default)]
    pub tracker: Option<TrackerConfig>,
}

#[derive(Clone, Debug, Deserialize)]
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
            .with_context(|| format!("reading perception catalog {}", path.display()))?;
        let document: CatalogDocument = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing perception catalog {}", path.display()))?;
        let catalog = Self::new(document.models, document.pipelines)?;
        catalog.validate_runtime_files()?;
        Ok(catalog)
    }

    pub fn new(models: Vec<ModelConfig>, pipelines: Vec<PipelineConfig>) -> Result<Self> {
        ensure!(
            !models.is_empty(),
            "perception catalog requires at least one model"
        );
        ensure!(
            !pipelines.is_empty(),
            "perception catalog requires at least one pipeline"
        );
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
        let mut pipeline_map = BTreeMap::new();
        for pipeline in pipelines {
            validate_id("pipeline id", &pipeline.id)?;
            ensure!(
                model_map.contains_key(&pipeline.model_id),
                "pipeline `{}` references unknown model `{}`",
                pipeline.id,
                pipeline.model_id
            );
            ensure!(
                pipeline.deepstream_config_path.is_absolute(),
                "deepstream_config_path for `{}` must be absolute",
                pipeline.id
            );
            match pipeline.operation {
                PipelineOperation::ObjectDetection => ensure!(
                    pipeline.tracker.is_none(),
                    "pipeline `{}` must not configure a tracker for object_detection",
                    pipeline.id
                ),
                PipelineOperation::ObjectDetectionTracking => ensure!(
                    pipeline.tracker.is_some(),
                    "pipeline `{}` requires a tracker for object_detection_tracking",
                    pipeline.id
                ),
                PipelineOperation::InstanceSegmentation | PipelineOperation::PoseEstimation => {
                    anyhow::bail!(
                        "pipeline `{}` uses an operation not implemented by the production runner",
                        pipeline.id
                    )
                }
            }
            if let Some(tracker) = &pipeline.tracker {
                ensure!(
                    tracker.config_path.is_absolute(),
                    "tracker config_path for `{}` must be absolute",
                    pipeline.id
                );
                ensure!(
                    tracker.width > 0
                        && tracker.height > 0
                        && tracker.width % 32 == 0
                        && tracker.height % 32 == 0,
                    "tracker dimensions for `{}` must be positive multiples of 32",
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
            ensure!(
                pipeline.deepstream_config_path.is_file(),
                "deepstream_config_path for `{}` is not a regular file: {}",
                pipeline.id,
                pipeline.deepstream_config_path.display()
            );
            if let Some(tracker) = &pipeline.tracker {
                ensure!(
                    tracker.config_path.is_file(),
                    "tracker config_path for `{}` is not a regular file: {}",
                    pipeline.id,
                    tracker.config_path.display()
                );
            }
        }
        Ok(())
    }
}

pub fn pipeline_view(config: &PipelineConfig) -> PipelineView {
    PipelineView {
        id: config.id.clone(),
        uri: uris::pipeline_uri(&config.id),
        title: config.title.clone(),
        description: config.description.clone(),
        operation: config.operation,
        model_uri: uris::model_uri(&config.model_id),
        tracking: config.tracker.is_some(),
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

    #[test]
    fn catalog_rejects_unknown_models() {
        let error = PipelineCatalog::new(
            vec![ModelConfig {
                id: "detector".to_owned(),
                title: "Detector".to_owned(),
                description: String::new(),
                format: ModelFormat::TensorRtEngine,
                model_path: "/models/detector.engine".into(),
            }],
            vec![PipelineConfig {
                id: "detect".to_owned(),
                title: "Detect".to_owned(),
                description: String::new(),
                operation: PipelineOperation::ObjectDetection,
                model_id: "missing".to_owned(),
                deepstream_config_path: "/etc/perception/detect.txt".into(),
                tracker: None,
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown model"));
    }

    #[test]
    fn tracking_pipeline_requires_typed_tracker_config() {
        let error = PipelineCatalog::new(
            vec![ModelConfig {
                id: "detector".to_owned(),
                title: "Detector".to_owned(),
                description: String::new(),
                format: ModelFormat::TensorRtEngine,
                model_path: "/models/detector.engine".into(),
            }],
            vec![PipelineConfig {
                id: "track".to_owned(),
                title: "Track".to_owned(),
                description: String::new(),
                operation: PipelineOperation::ObjectDetectionTracking,
                model_id: "detector".to_owned(),
                deepstream_config_path: "/etc/perception/detect.txt".into(),
                tracker: None,
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("requires a tracker"));
    }

    #[test]
    fn repository_catalog_example_matches_the_typed_contract() {
        let document: CatalogDocument = serde_json::from_slice(include_bytes!(
            "../../../configs/perception/catalog.example.json"
        ))
        .unwrap();
        let catalog = PipelineCatalog::new(document.models, document.pipelines).unwrap();
        assert_eq!(catalog.pipeline_ids().len(), 2);
        assert_eq!(catalog.model_ids().len(), 1);
    }
}
