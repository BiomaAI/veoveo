//! Model, document and Artifact resource conventions.
//! Typed prediction and usage addresses belong to `crate::contract`.
//!
//! - `media://models`                  — compact model catalog index
//! - `media://model/{model_id}`        — full schema + pricing for one model
//! - `media://artifact/{artifact_id}`  — server-owned artifact metadata/content

use veoveo_artifact_contract::ArtifactId;
use veoveo_mcp_contract::ServerResourceUris;

pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("media").expect("declared server resource scheme")
    });

pub const MODELS_URI: &str = "media://models";
pub const STUDIO_APP_URI: &str = "ui://media/studio.html";
pub const MODEL_TEMPLATE: &str = "media://model/{model_id}";
pub const ARTIFACT_TEMPLATE: &str = "media://artifact/{artifact_id}";

/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris::new(SCHEME.clone())`; a unit test below pins the
/// equivalence.
pub const DOCS_URI: &str = "media://docs";
pub const CONTRACT_URI: &str = "media://contract";
pub const DOC_TEMPLATE: &str = "media://docs/{doc_id}";

fn media_uris() -> ServerResourceUris {
    ServerResourceUris::new(SCHEME.clone())
}

pub fn model_uri(model_id: &str) -> String {
    media_uris().model_uri(model_id)
}

pub fn artifact_uri(artifact_id: ArtifactId) -> veoveo_artifact_contract::ArtifactUri {
    media_uris().artifact_uri(artifact_id)
}

pub fn doc_uri(doc_id: &str) -> String {
    media_uris().doc_uri(doc_id)
}

pub fn parse_doc_uri(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("media", uri)
}

/// Parse a `media://model/{model_id}` URI. Model ids contain slashes
/// (e.g. `openai/gpt-image-2/edit`), so everything after the prefix is the id.
pub fn parse_model_uri(uri: &str) -> Option<&str> {
    media_uris().parse_model_uri(uri)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    media_uris().parse_artifact_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = media_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), conventions.doc_uri("agents"));
        assert_eq!(parse_doc_uri("media://docs/agents"), Some("agents"));
        assert_eq!(parse_doc_uri("media://docs"), None);
        assert_eq!(parse_doc_uri("media://docs/agents/extra"), None);
    }

    #[test]
    fn model_uri_round_trip() {
        let uri = model_uri("openai/gpt-image-2/edit");
        assert_eq!(uri, "media://model/openai/gpt-image-2/edit");
        assert_eq!(parse_model_uri(&uri), Some("openai/gpt-image-2/edit"));
    }

    #[test]
    fn artifact_uri_round_trip() {
        let artifact_id = ArtifactId::new();
        let uri = artifact_uri(artifact_id);
        assert_eq!(uri.as_str(), format!("media://artifact/{artifact_id}"));
        assert_eq!(parse_artifact_uri(uri.as_str()), Some(artifact_id));
        assert_eq!(parse_artifact_uri("media://artifact/not-a-sha"), None);
    }
}
