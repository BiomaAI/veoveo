use veoveo_artifact_contract::ArtifactId;
use veoveo_mcp_contract::ServerResourceUris;

/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris::new(SCHEME.clone())`; a unit test below pins that
/// equivalence.
pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("timeseries").expect("declared server resource scheme")
    });

pub const DOCS_URI: &str = "timeseries://docs";
pub const CONTRACT_URI: &str = "timeseries://contract";
pub const DOC_TEMPLATE: &str = "timeseries://docs/{doc_id}";
pub const ARTIFACT_TEMPLATE: &str = "timeseries://artifact/{artifact_id}";
/// The forecast app view. The first path segment is the server slug; the
/// gateway's ServerOwned projection rewrites it to the mounted slug, so the
/// URI is stable end to end.
pub const FORECAST_APP_URI: &str = "ui://timeseries/forecast.html";

fn timeseries_uris() -> ServerResourceUris {
    ServerResourceUris::new(SCHEME.clone())
}

pub fn doc_uri(doc_id: &str) -> String {
    timeseries_uris().doc_uri(doc_id)
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("timeseries", uri)
}

pub fn artifact_uri(artifact_id: ArtifactId) -> veoveo_artifact_contract::ArtifactUri {
    timeseries_uris().artifact_uri(artifact_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    timeseries_uris().parse_artifact_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_uri_round_trips() {
        let artifact_id = ArtifactId::new();
        let uri = artifact_uri(artifact_id);
        assert_eq!(uri.as_str(), format!("timeseries://artifact/{artifact_id}"));
        assert_eq!(parse_artifact_uri(uri.as_str()), Some(artifact_id));
        assert_eq!(parse_artifact_uri("timeseries://artifact/nope"), None);
    }

    #[test]
    fn well_known_uris_match_the_shared_conventions() {
        let conventions = timeseries_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), "timeseries://docs/agents");
        assert_eq!(parse_doc("timeseries://docs/agents"), Some("agents"));
        assert_eq!(parse_doc("timeseries://docs"), None);
        assert_eq!(parse_doc("timeseries://docs/agents/extra"), None);
    }
}
