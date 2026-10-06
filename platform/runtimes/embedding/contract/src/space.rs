//! Model configuration that defines a vector space, independent of serving provenance.
use crate::EmbeddingError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum EmbeddingPooling {
    #[vocabulary(rename = "last_token")]
    LastToken,
    #[vocabulary(rename = "mean")]
    Mean,
    #[vocabulary(rename = "cls")]
    Cls,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum EmbeddingNormalization {
    #[vocabulary(rename = "l2")]
    L2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum EmbeddingPrecision {
    #[vocabulary(rename = "float32")]
    Float32,
    #[vocabulary(rename = "float16")]
    Float16,
    #[vocabulary(rename = "bfloat16")]
    Bfloat16,
    #[vocabulary(rename = "int8")]
    Int8,
    #[vocabulary(rename = "int4")]
    Int4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct EmbeddingMaxInputTokens(u32);
impl EmbeddingMaxInputTokens {
    pub const MAX: u32 = 131_072;
    pub fn new(value: u32) -> Result<Self, EmbeddingError> {
        if !(1..=Self::MAX).contains(&value) {
            return Err(EmbeddingError("maximum input tokens must be 1..=131072"));
        }
        Ok(Self(value))
    }
    pub const fn get(self) -> u32 {
        self.0
    }
}
impl TryFrom<u32> for EmbeddingMaxInputTokens {
    type Error = EmbeddingError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<EmbeddingMaxInputTokens> for u32 {
    fn from(value: EmbeddingMaxInputTokens) -> Self {
        value.0
    }
}

impl JsonSchema for EmbeddingMaxInputTokens {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmbeddingMaxInputTokens".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = u32::json_schema(generator);
        schema.insert("minimum".into(), 1.into());
        schema.insert("maximum".into(), Self::MAX.into());
        schema
    }
}
