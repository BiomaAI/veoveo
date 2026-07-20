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
    /// TensorRT-LLM engines may be a single file or an engine directory.
    pub model_path: PathBuf,
    #[serde(default)]
    pub engine_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub id: String,
    pub title: String,
    pub description: String,
    pub operation: PipelineOperation,
    pub model_id: String,
    pub prompt_template_path: PathBuf,
    pub prompt_revision: String,
    pub observation: ObservationConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationConfig {
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
            .with_context(|| format!("reading reason catalog {}", path.display()))?;
        let document: CatalogDocument = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing reason catalog {}", path.display()))?;
        let catalog = Self::new(document.models, document.pipelines)?;
        catalog.validate_runtime_files()?;
        Ok(catalog)
    }

    pub fn new(models: Vec<ModelConfig>, pipelines: Vec<PipelineConfig>) -> Result<Self> {
        ensure!(
            !models.is_empty(),
            "reason catalog requires at least one model"
        );
        ensure!(
            !pipelines.is_empty(),
            "reason catalog requires at least one pipeline"
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
            if let Some(digest) = &model.engine_digest {
                ensure!(
                    !digest.trim().is_empty() && digest.len() <= 128,
                    "engine_digest for `{}` must be a short non-empty value",
                    model.id
                );
            }
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
                pipeline.prompt_template_path.is_absolute(),
                "prompt_template_path for `{}` must be absolute",
                pipeline.id
            );
            ensure!(
                !pipeline.prompt_revision.trim().is_empty()
                    && pipeline.prompt_revision.len() <= 128,
                "prompt_revision for `{}` must be a short non-empty value",
                pipeline.id
            );
            match pipeline.operation {
                PipelineOperation::VideoReasoning => {}
            }
            let ObservationConfig { width, height } = pipeline.observation;
            ensure!(
                width > 0
                    && height > 0
                    && width <= 3_840
                    && height <= 2_160
                    && width % 2 == 0
                    && height % 2 == 0,
                "observation dimensions for `{}` must be positive even values within 3840x2160",
                pipeline.id
            );
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
                model.model_path.exists(),
                "model_path for `{}` does not exist: {}",
                model.id,
                model.model_path.display()
            );
        }
        for pipeline in self.pipelines.values() {
            ensure!(
                pipeline.prompt_template_path.is_file(),
                "prompt_template_path for `{}` is not a regular file: {}",
                pipeline.id,
                pipeline.prompt_template_path.display()
            );
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
        prompt_revision: config.prompt_revision.clone(),
        observation_width: config.observation.width,
        observation_height: config.observation.height,
    }
}

pub fn model_view(config: &ModelConfig) -> ModelView {
    ModelView {
        id: config.id.clone(),
        uri: uris::model_uri(&config.id),
        title: config.title.clone(),
        description: config.description.clone(),
        format: config.format,
        engine_digest: config.engine_digest.clone(),
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

    fn model() -> ModelConfig {
        ModelConfig {
            id: "world-model".to_owned(),
            title: "World model".to_owned(),
            description: String::new(),
            format: ModelFormat::TensorRtLlmEngine,
            model_path: "/models/world-model.engine".into(),
            engine_digest: None,
        }
    }

    fn pipeline() -> PipelineConfig {
        PipelineConfig {
            id: "video-reasoning".to_owned(),
            title: "Video reasoning".to_owned(),
            description: String::new(),
            operation: PipelineOperation::VideoReasoning,
            model_id: "world-model".to_owned(),
            prompt_template_path: "/etc/veoveo/reason/prompt-template.txt".into(),
            prompt_revision: "v1".to_owned(),
            observation: ObservationConfig {
                width: 640,
                height: 360,
            },
        }
    }

    #[test]
    fn catalog_rejects_unknown_models() {
        let mut orphan = pipeline();
        orphan.model_id = "missing".to_owned();
        let error = PipelineCatalog::new(vec![model()], vec![orphan]).unwrap_err();
        assert!(error.to_string().contains("unknown model"));
    }

    #[test]
    fn catalog_rejects_odd_observation_dimensions() {
        let mut odd = pipeline();
        odd.observation.width = 641;
        let error = PipelineCatalog::new(vec![model()], vec![odd]).unwrap_err();
        assert!(error.to_string().contains("observation dimensions"));
    }

    #[test]
    fn catalog_requires_prompt_revision() {
        let mut unrevisioned = pipeline();
        unrevisioned.prompt_revision = " ".to_owned();
        let error = PipelineCatalog::new(vec![model()], vec![unrevisioned]).unwrap_err();
        assert!(error.to_string().contains("prompt_revision"));
    }

    #[test]
    fn repository_catalog_example_matches_the_typed_contract() {
        let document: CatalogDocument = serde_json::from_slice(include_bytes!(
            "../../../configs/reason/catalog.example.json"
        ))
        .unwrap();
        let catalog = PipelineCatalog::new(document.models, document.pipelines).unwrap();
        assert_eq!(catalog.pipeline_ids().len(), 1);
        assert_eq!(catalog.model_ids().len(), 1);
    }
}
