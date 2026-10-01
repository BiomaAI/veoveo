//! Model-independent input admission. Model-specific formatting belongs to the client.
use crate::EmbeddingError;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EmbeddingText(String);
impl EmbeddingText {
    pub const MAX_BYTES: usize = 16 * 1024;
    pub fn new(value: impl Into<String>) -> Result<Self, EmbeddingError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > Self::MAX_BYTES {
            return Err(EmbeddingError(
                "embedding text requires 1..=16384 UTF-8 bytes",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for EmbeddingText {
    type Error = EmbeddingError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingText> for String {
    fn from(value: EmbeddingText) -> Self {
        value.0
    }
}
impl JsonSchema for EmbeddingText {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmbeddingText".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string", "minLength":1, "maxLength":16384})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EmbeddingTask(String);
impl EmbeddingTask {
    pub const MAX_BYTES: usize = 1024;
    pub fn new(value: impl Into<String>) -> Result<Self, EmbeddingError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.len() > Self::MAX_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(EmbeddingError(
                "embedding task requires 1..=1024 printable UTF-8 bytes",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for EmbeddingTask {
    type Error = EmbeddingError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingTask> for String {
    fn from(value: EmbeddingTask) -> Self {
        value.0
    }
}
impl JsonSchema for EmbeddingTask {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmbeddingTask".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type":"string", "minLength":1, "maxLength":1024})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<EmbeddingText>", into = "Vec<EmbeddingText>")]
pub struct EmbeddingBatch(Vec<EmbeddingText>);
impl EmbeddingBatch {
    pub const MAX_INPUTS: usize = 32;
    pub const MAX_BYTES: usize = 128 * 1024;
    pub fn new(texts: Vec<EmbeddingText>) -> Result<Self, EmbeddingError> {
        if texts.is_empty() || texts.len() > Self::MAX_INPUTS {
            return Err(EmbeddingError("embedding batch requires 1..=32 inputs"));
        }
        if texts.iter().map(|text| text.as_str().len()).sum::<usize>() > Self::MAX_BYTES {
            return Err(EmbeddingError("embedding batch exceeds 131072 UTF-8 bytes"));
        }
        Ok(Self(texts))
    }
    pub fn texts(&self) -> &[EmbeddingText] {
        &self.0
    }
}
impl TryFrom<Vec<EmbeddingText>> for EmbeddingBatch {
    type Error = EmbeddingError;
    fn try_from(value: Vec<EmbeddingText>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingBatch> for Vec<EmbeddingText> {
    fn from(value: EmbeddingBatch) -> Self {
        value.0
    }
}
impl JsonSchema for EmbeddingBatch {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmbeddingBatch".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let item = generator.subschema_for::<EmbeddingText>();
        json_schema!({"type":"array", "minItems":1, "maxItems":32, "items":item})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingPriority {
    Interactive,
    Bulk,
}
