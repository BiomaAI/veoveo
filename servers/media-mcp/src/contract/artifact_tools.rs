use veoveo_artifact_contract::ArtifactMetadata;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactArgs {
    /// Media artifact resource URI, for example media://artifact/{artifact_id}.
    pub artifact_uri: super::MediaArtifactUri,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ArtifactOutput {
    pub artifact: ArtifactMetadata,
    pub inlined: bool,
}
