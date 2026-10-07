use veoveo_artifact_contract::ArtifactMetadata;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactArgs {
    /// Media artifact resource URI, for example media://artifact/{artifact_id}.
    pub artifact_uri: super::MediaArtifactUri,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ArtifactOutput")]
pub struct ArtifactOutputValue {
    pub artifact: ArtifactMetadata,
    pub inlined: bool,
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ArtifactOutputValue", into = "ArtifactOutputValue")]
pub struct ArtifactOutput(veoveo_types::Checked<ArtifactOutputValue>);
impl std::ops::Deref for ArtifactOutput {
    type Target = ArtifactOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ArtifactOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ArtifactOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ArtifactOutputValue::json_schema(generator)
    }
}
impl TryFrom<ArtifactOutputValue> for ArtifactOutput {
    type Error = super::ModelCatalogError;
    fn try_from(value: ArtifactOutputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ArtifactOutput> for ArtifactOutputValue {
    fn from(value: ArtifactOutput) -> Self {
        value.0.into_inner()
    }
}
impl ArtifactOutputValue {
    pub fn build(self) -> Result<ArtifactOutput, super::ModelCatalogError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ArtifactOutputValue {
    type Error = super::ModelCatalogError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.artifact.download_url.is_some()
            || super::MediaArtifactUri::parse(self.artifact.artifact_uri.as_str()).is_err()
            || (self.inlined
                && (self.artifact.byte_len > 3 * 1024 * 1024
                    || !self
                        .artifact
                        .mime_type
                        .as_deref()
                        .is_some_and(|m| m.starts_with("image/"))))
        {
            return Err(super::ModelCatalogError);
        }
        Ok(())
    }
}
