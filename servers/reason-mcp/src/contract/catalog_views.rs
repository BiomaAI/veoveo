//! Catalog identities are derived from their typed addresses.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    ModelFormat, ModelId, ModelUri, PipelineId, PipelineOperation, PipelineUri, ReasonContractError,
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ModelViewWire", into = "ModelViewWire")]
pub struct ModelView {
    pub uri: ModelUri,
    pub title: String,
    pub description: String,
    pub format: ModelFormat,
    pub model_digest: Option<String>,
}

impl ModelView {
    pub fn new(id: ModelId, title: String, description: String, format: ModelFormat) -> Self {
        Self {
            uri: ModelUri::new(id),
            title,
            description,
            format,
            model_digest: None,
        }
    }
    pub fn with_model_digest(mut self, digest: Option<String>) -> Self {
        self.model_digest = digest;
        self
    }
    pub fn id(&self) -> &ModelId {
        self.uri.id()
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
struct ModelViewWire {
    id: ModelId,
    uri: ModelUri,
    title: String,
    description: String,
    format: ModelFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_digest: Option<String>,
}
impl TryFrom<ModelViewWire> for ModelView {
    type Error = ReasonContractError;
    fn try_from(wire: ModelViewWire) -> Result<Self, Self::Error> {
        if wire.uri.id() != &wire.id {
            return Err(ReasonContractError::InvalidRelationship("model ID and URI"));
        }
        Ok(
            Self::new(wire.id, wire.title, wire.description, wire.format)
                .with_model_digest(wire.model_digest),
        )
    }
}
impl From<ModelView> for ModelViewWire {
    fn from(view: ModelView) -> Self {
        Self {
            id: view.id().clone(),
            uri: view.uri,
            title: view.title,
            description: view.description,
            format: view.format,
            model_digest: view.model_digest,
        }
    }
}

/// Catalog presentation supplied after the pipeline and model identities.
#[derive(Clone, Debug)]
pub struct PipelineDetails {
    pub title: String,
    pub description: String,
    pub operation: PipelineOperation,
    pub prompt_revision: String,
    pub observation_width: u32,
    pub observation_height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PipelineViewWire", into = "PipelineViewWire")]
pub struct PipelineView {
    pub uri: PipelineUri,
    pub title: String,
    pub description: String,
    pub operation: PipelineOperation,
    pub model_uri: ModelUri,
    pub prompt_revision: String,
    pub observation_width: u32,
    pub observation_height: u32,
}
impl PipelineView {
    pub fn new(id: PipelineId, model: ModelId, details: PipelineDetails) -> Self {
        Self {
            uri: PipelineUri::new(id),
            model_uri: ModelUri::new(model),
            title: details.title,
            description: details.description,
            operation: details.operation,
            prompt_revision: details.prompt_revision,
            observation_width: details.observation_width,
            observation_height: details.observation_height,
        }
    }
    pub fn id(&self) -> &PipelineId {
        self.uri.id()
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
struct PipelineViewWire {
    id: PipelineId,
    uri: PipelineUri,
    title: String,
    description: String,
    operation: PipelineOperation,
    model_uri: ModelUri,
    prompt_revision: String,
    observation_width: u32,
    observation_height: u32,
}
impl TryFrom<PipelineViewWire> for PipelineView {
    type Error = ReasonContractError;
    fn try_from(wire: PipelineViewWire) -> Result<Self, Self::Error> {
        if wire.uri.id() != &wire.id {
            return Err(ReasonContractError::InvalidRelationship(
                "pipeline ID and URI",
            ));
        }
        Ok(Self::new(
            wire.id,
            wire.model_uri.id().clone(),
            PipelineDetails {
                title: wire.title,
                description: wire.description,
                operation: wire.operation,
                prompt_revision: wire.prompt_revision,
                observation_width: wire.observation_width,
                observation_height: wire.observation_height,
            },
        ))
    }
}
impl From<PipelineView> for PipelineViewWire {
    fn from(view: PipelineView) -> Self {
        Self {
            id: view.id().clone(),
            uri: view.uri,
            title: view.title,
            description: view.description,
            operation: view.operation,
            model_uri: view.model_uri,
            prompt_revision: view.prompt_revision,
            observation_width: view.observation_width,
            observation_height: view.observation_height,
        }
    }
}
