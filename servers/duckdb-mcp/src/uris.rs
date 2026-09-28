use veoveo_artifact_contract::ArtifactId;
use veoveo_mcp_contract::ServerResourceUris;

pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("duckdb").expect("declared server resource scheme")
    });

pub const DBS_ROOT_URI: &str = "duckdb://dbs";
pub const WORKBENCH_APP_URI: &str = "ui://duckdb/workbench.html";
pub const DB_TEMPLATE: &str = "duckdb://db/{db_id}";
pub const ARTIFACT_TEMPLATE: &str = "duckdb://artifact/{artifact_id}";

/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris::new(SCHEME.clone())`; a unit test below pins the
/// equivalence.
pub const DOCS_URI: &str = "duckdb://docs";
pub const CONTRACT_URI: &str = "duckdb://contract";
pub const DOC_TEMPLATE: &str = "duckdb://docs/{doc_id}";

fn duckdb_uris() -> ServerResourceUris {
    ServerResourceUris::new(SCHEME.clone())
}

pub fn db_uri(db_id: &str) -> String {
    format!("duckdb://db/{db_id}")
}

pub fn parse_db_uri(uri: &str) -> Option<&str> {
    let rest = uri.strip_prefix("duckdb://db/")?;
    if rest.is_empty() || rest.contains('/') {
        None
    } else {
        Some(rest)
    }
}

pub fn doc_uri(doc_id: &str) -> String {
    duckdb_uris().doc_uri(doc_id)
}

pub fn parse_doc_uri(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("duckdb", uri)
}

pub fn artifact_uri(artifact_id: ArtifactId) -> veoveo_artifact_contract::ArtifactUri {
    duckdb_uris().artifact_uri(artifact_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    duckdb_uris().parse_artifact_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_uri_round_trips() {
        let uri = db_uri("robot_metrics");
        assert_eq!(uri, "duckdb://db/robot_metrics");
        assert_eq!(parse_db_uri(&uri), Some("robot_metrics"));
        assert_eq!(parse_db_uri("duckdb://db/"), None);
        assert_eq!(parse_db_uri("duckdb://db/a/b"), None);
        assert_eq!(parse_db_uri("duckdb://dbs"), None);
    }

    #[test]
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = duckdb_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), conventions.doc_uri("agents"));
        assert_eq!(parse_doc_uri("duckdb://docs/agents"), Some("agents"));
        assert_eq!(parse_doc_uri("duckdb://docs"), None);
        assert_eq!(parse_doc_uri("duckdb://docs/agents/extra"), None);
    }

    #[test]
    fn artifact_uri_round_trips() {
        let artifact_id = ArtifactId::new();
        let uri = artifact_uri(artifact_id);
        assert_eq!(uri.as_str(), format!("duckdb://artifact/{artifact_id}"));
        assert_eq!(parse_artifact_uri(uri.as_str()), Some(artifact_id));
    }
}
