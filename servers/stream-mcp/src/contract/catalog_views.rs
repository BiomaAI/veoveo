//! Catalog identities and profile/model relationships belong to Stream.
use super::{
    ModelFormat, ModelId, ModelUri, PerceptionOperation, PipelineId, PipelineProfile, PipelineUri,
    StreamContractError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct PipelineDetails {
    pub title: String,
    pub description: String,
    pub supports_recording_replay: bool,
    pub supports_live_input: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PipelineViewWire", into = "PipelineViewWire")]
pub struct PipelineView {
    pub uri: PipelineUri,
    pub title: String,
    pub description: String,
    profile: PipelineProfile,
    model_uri: Option<ModelUri>,
    pub supports_recording_replay: bool,
    pub supports_live_input: bool,
}
impl PipelineView {
    pub fn pass_through(id: PipelineId, details: PipelineDetails) -> Self {
        Self::build(id, PipelineProfile::PassThrough, None, details)
    }
    pub fn perception(
        id: PipelineId,
        model: ModelId,
        operation: PerceptionOperation,
        tracking: bool,
        details: PipelineDetails,
    ) -> Self {
        Self::build(
            id,
            PipelineProfile::Perception {
                operation,
                tracking,
            },
            Some(ModelUri::new(model)),
            details,
        )
    }
    fn build(
        id: PipelineId,
        profile: PipelineProfile,
        model_uri: Option<ModelUri>,
        details: PipelineDetails,
    ) -> Self {
        Self {
            uri: PipelineUri::new(id),
            title: details.title,
            description: details.description,
            profile,
            model_uri,
            supports_recording_replay: details.supports_recording_replay,
            supports_live_input: details.supports_live_input,
        }
    }
    pub fn id(&self) -> &PipelineId {
        self.uri.id()
    }
    pub fn profile(&self) -> &PipelineProfile {
        &self.profile
    }
    pub fn model_uri(&self) -> Option<&ModelUri> {
        self.model_uri.as_ref()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct PipelineViewWire {
    id: PipelineId,
    uri: PipelineUri,
    title: String,
    description: String,
    profile: PipelineProfile,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_uri: Option<ModelUri>,
    supports_recording_replay: bool,
    supports_live_input: bool,
}
impl TryFrom<PipelineViewWire> for PipelineView {
    type Error = StreamContractError;
    fn try_from(wire: PipelineViewWire) -> Result<Self, Self::Error> {
        if wire.uri.id() != &wire.id {
            return Err(StreamContractError::InvalidRelationship(
                "pipeline ID and URI",
            ));
        }
        if matches!(wire.profile, PipelineProfile::Perception { .. }) != wire.model_uri.is_some() {
            return Err(StreamContractError::InvalidRelationship(
                "pipeline profile and model",
            ));
        }
        Ok(Self::build(
            wire.id,
            wire.profile,
            wire.model_uri,
            PipelineDetails {
                title: wire.title,
                description: wire.description,
                supports_recording_replay: wire.supports_recording_replay,
                supports_live_input: wire.supports_live_input,
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
            profile: view.profile,
            model_uri: view.model_uri,
            supports_recording_replay: view.supports_recording_replay,
            supports_live_input: view.supports_live_input,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ModelViewWire", into = "ModelViewWire")]
pub struct ModelView {
    pub uri: ModelUri,
    pub title: String,
    pub description: String,
    pub format: ModelFormat,
}
impl ModelView {
    pub fn new(id: ModelId, title: String, description: String, format: ModelFormat) -> Self {
        Self {
            uri: ModelUri::new(id),
            title,
            description,
            format,
        }
    }
    pub fn id(&self) -> &ModelId {
        self.uri.id()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct ModelViewWire {
    id: ModelId,
    uri: ModelUri,
    title: String,
    description: String,
    format: ModelFormat,
}
impl TryFrom<ModelViewWire> for ModelView {
    type Error = StreamContractError;
    fn try_from(wire: ModelViewWire) -> Result<Self, Self::Error> {
        if wire.uri.id() != &wire.id {
            return Err(StreamContractError::InvalidRelationship("model ID and URI"));
        }
        Ok(Self::new(
            wire.id,
            wire.title,
            wire.description,
            wire.format,
        ))
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
        }
    }
}
