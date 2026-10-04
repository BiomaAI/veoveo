//! Embedding identity and vector admission without a transport or inference engine.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::Sha256Digest;

mod input;
pub use input::{EmbeddingBatch, EmbeddingPriority, EmbeddingTask, EmbeddingText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingError(pub &'static str);
impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for EmbeddingError {}

#[derive(veoveo_types::Id, Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = EmbeddingError, validate = validate_embedding_id)]
pub struct EmbeddingModelId(String);
#[derive(veoveo_types::Id, Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = EmbeddingError, validate = validate_embedding_id)]
pub struct EmbeddingModelRevision(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "u16", into = "u16")]
pub struct EmbeddingDimension(u16);
impl EmbeddingDimension {
    pub fn new(value: u16) -> Result<Self, EmbeddingError> {
        if !(1..=8192).contains(&value) {
            return Err(EmbeddingError("embedding dimension must be 1..=8192"));
        }
        Ok(Self(value))
    }
    pub const fn get(self) -> u16 {
        self.0
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
    pub runtime_image: Sha256Digest,
}

/// A vector is admitted with the space that produced it, never as an unlabelled array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "VectorWire", into = "VectorWire")]
pub struct EmbeddingVector(veoveo_types::Checked<VectorWire>);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VectorWire {
    space: EmbeddingSpace,
    values: Vec<f32>,
}
impl EmbeddingVector {
    pub fn new(space: EmbeddingSpace, values: Vec<f32>) -> Result<Self, EmbeddingError> {
        VectorWire { space, values }.try_into()
    }
    pub fn space(&self) -> &EmbeddingSpace {
        &self.0.space
    }
    pub fn values(&self) -> &[f32] {
        &self.0.values
    }
}
impl veoveo_types::Check for VectorWire {
    type Error = EmbeddingError;
    fn check(&self) -> Result<(), Self::Error> {
        let wire = self;
        if wire.values.len() != usize::from(wire.space.dimension.get())
            || wire.values.iter().any(|v| !v.is_finite())
        {
            return Err(EmbeddingError(
                "embedding vector has the wrong dimension or nonfinite values",
            ));
        }
        let norm: f64 = wire.values.iter().map(|v| f64::from(*v).powi(2)).sum();
        if (norm - 1.0).abs() > 0.002 {
            return Err(EmbeddingError("embedding vector must be L2 normalized"));
        }
        Ok(())
    }
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
