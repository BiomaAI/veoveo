/// Well-known surface roots (contract C18, C19). These literals must match
/// `veoveo_mcp_contract::ServerResourceUris::new("recording")`; a unit test
/// below pins that equivalence.
pub const DOCS_URI: &str = "recording://docs";
pub const CONTRACT_URI: &str = "recording://contract";
pub const DOC_TEMPLATE: &str = "recording://docs/{doc_id}";

pub const CATALOG_URI: &str = "recording://catalog";
pub const RECORDING_TEMPLATE: &str = "recording://recordings/{recording_id}";
pub const SEGMENTS_TEMPLATE: &str = "recording://recordings/{recording_id}/segments";

pub fn doc_uri(doc_id: &str) -> String {
    format!("recording://docs/{doc_id}")
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("recording", uri)
}

pub fn recording_uri(recording_id: &str) -> String {
    format!("recording://recordings/{recording_id}")
}

pub fn segments_uri(recording_id: &str) -> String {
    format!("recording://recordings/{recording_id}/segments")
}

pub fn parse_recording_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("recording://recordings/")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn parse_segments_uri(uri: &str) -> Option<&str> {
    let value = uri
        .strip_prefix("recording://recordings/")?
        .strip_suffix("/segments")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = veoveo_mcp_contract::ServerResourceUris::new("recording");
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), "recording://docs/agents");
        assert_eq!(parse_doc("recording://docs/agents"), Some("agents"));
        assert_eq!(parse_doc("recording://docs"), None);
        assert_eq!(parse_doc("recording://docs/agents/extra"), None);
    }

    #[test]
    fn uri_shapes_do_not_overlap() {
        assert_eq!(
            parse_recording_uri("recording://recordings/abc"),
            Some("abc")
        );
        assert_eq!(
            parse_recording_uri("recording://recordings/abc/segments"),
            None
        );
        assert_eq!(
            parse_segments_uri("recording://recordings/abc/segments"),
            Some("abc")
        );
    }
}
