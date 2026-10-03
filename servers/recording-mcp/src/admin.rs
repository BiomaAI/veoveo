//! The crate documents embedded at build time.
//!
//! The shared host serves them at `recording://docs`, `recording://docs/{doc_id}`,
//! `recording://contract` and the administrative `{mount}/admin/docs` routes
//! (contract C18-C21).

use std::sync::LazyLock;

use veoveo_mcp_contract::docs::ServerDocs;

/// The crate documents embedded at build time.
pub static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("recording"));

#[cfg(test)]
mod tests {
    use veoveo_mcp_contract::docs::{
        ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN, parse_compliance,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_manual_declares_the_well_known_surface_met() {
        assert_eq!(SERVER_DOCS.server(), "recording");
        SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        let compliance = parse_compliance(SERVER_DOCS.agent_manual().expect("agent manual"));
        for id in ["C18", "C19", "C20", "C21"] {
            let item = compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
    }
}
