//! Embedding identity and vector admission without a transport or inference engine.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod input;
mod profile;
mod space;
pub use input::{EmbeddingBatch, EmbeddingPriority, EmbeddingTask, EmbeddingText};
pub use profile::*;
pub use space::{
    EmbeddingMaxInputTokens, EmbeddingNormalization, EmbeddingPooling, EmbeddingPrecision,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingError(pub &'static str);
impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for EmbeddingError {}

#[veoveo_types::id(text(EmbeddingIds))]
pub struct EmbeddingModelId(String);
#[veoveo_types::id(text(EmbeddingIds))]
pub struct EmbeddingModelRevision(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct EmbeddingDimension(u16);
impl EmbeddingDimension {
    pub const MAX: u16 = 8192;
    pub fn new(value: u16) -> Result<Self, EmbeddingError> {
        if !(1..=Self::MAX).contains(&value) {
            return Err(EmbeddingError("embedding dimension must be 1..=8192"));
        }
        Ok(Self(value))
    }
    pub const fn get(self) -> u16 {
        self.0
    }
}
impl JsonSchema for EmbeddingDimension {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmbeddingDimension".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = u16::json_schema(generator);
        schema.insert("minimum".into(), 1.into());
        schema.insert("maximum".into(), Self::MAX.into());
        schema
    }
}
impl TryFrom<u16> for EmbeddingDimension {
    type Error = EmbeddingError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingDimension> for u16 {
    fn from(value: EmbeddingDimension) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingSpace {
    pub model: EmbeddingModelId,
    pub revision: EmbeddingModelRevision,
    pub dimension: EmbeddingDimension,
    pub pooling: EmbeddingPooling,
    pub normalization: EmbeddingNormalization,
    pub precision: EmbeddingPrecision,
    pub max_input_tokens: EmbeddingMaxInputTokens,
}
impl EmbeddingSpace {
    pub fn revision(&self) -> veoveo_types::Sha256Digest {
        profile::digest(b"veoveo.ai/embedding-space/v1", self)
    }
}

/// A vector is admitted with the space that produced it, never as an unlabelled array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "VectorWire", into = "VectorWire")]
pub struct EmbeddingVector(veoveo_types::Checked<VectorWire>);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VectorWire {
    space: EmbeddingSpace,
    profile_id: EmbeddingExecutionProfileId,
    values: Vec<f32>,
}
impl EmbeddingVector {
    pub fn new(
        runtime: &QualifiedEmbeddingRuntime,
        values: Vec<f32>,
    ) -> Result<Self, EmbeddingError> {
        VectorWire {
            space: runtime.space().clone(),
            profile_id: runtime.profile().id().clone(),
            values,
        }
        .try_into()
    }
    pub fn space(&self) -> &EmbeddingSpace {
        &self.0.space
    }
    pub fn values(&self) -> &[f32] {
        &self.0.values
    }
    pub fn profile_id(&self) -> &EmbeddingExecutionProfileId {
        &self.0.profile_id
    }
}
impl veoveo_types::Check for VectorWire {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        validate_embedding_values(&self.space, &self.values)
    }
}

/// Numeric admission shared by production vectors and verification-only candidate measurements.
pub fn validate_embedding_values(
    space: &EmbeddingSpace,
    values: &[f32],
) -> Result<(), EmbeddingError> {
    if values.len() != usize::from(space.dimension.get()) || values.iter().any(|v| !v.is_finite()) {
        return Err(EmbeddingError(
            "embedding vector has the wrong dimension or nonfinite values",
        ));
    }
    let norm: f64 = values.iter().map(|v| f64::from(*v).powi(2)).sum();
    if (norm - 1.).abs() > 0.002 {
        return Err(EmbeddingError("embedding vector must be L2 normalized"));
    }
    Ok(())
}
impl TryFrom<VectorWire> for EmbeddingVector {
    type Error = EmbeddingError;
    fn try_from(value: VectorWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<EmbeddingVector> for VectorWire {
    fn from(value: EmbeddingVector) -> Self {
        value.0.into_inner()
    }
}

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct EmbeddingIds;
impl IdProfile for EmbeddingIds {
    type Error = EmbeddingError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_embedding_id(value));
}

fn validate_embedding_id(value: &str) -> Result<(), EmbeddingError> {
    if value.is_empty()
        || value.len() > 256
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(EmbeddingError(
            "embedding identity requires 1..=256 printable bytes",
        ));
    }
    Ok(())
}
