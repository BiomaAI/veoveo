//! Embedded server documents and the contract self-declaration.
//!
//! Implements the Well-Known Surface of `mcp/contract/DESIGN.md` (C18-C21):
//! documents embedded at build time from the server crate, the
//! machine-readable contract declaration served at `{scheme}://contract`, and
//! llms.txt rendering for the administrative mount. Servers obtain the
//! document set with the [`server_docs!`](crate::server_docs) macro so the
//! deployed binary serves the manual of exactly the version it was built
//! from.

use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Serialize;

pub mod catalog;
#[cfg(feature = "runtime")]
mod knowledge;
mod profile;
pub use catalog::{CATALOG_REVISION, RequirementId};
pub use profile::{
    COMPLIANCE_END, COMPLIANCE_START, ComplianceError, ComplianceItem, ComplianceProfile,
    ComplianceStatus, ContractDeclaration, render_compliance, verify_manual,
};
#[doc(hidden)]
pub use veoveo_macros::embedded_document;
pub use veoveo_mcp_knowledge_extension as knowledge_extension;

/// The normative contract revision this crate implements.
pub const CONTRACT_REVISION: u32 = 4;

/// Identifier of the required agent manual document.
pub const DOC_ID_AGENTS: &str = "agents";

/// Identifier of the required domain design document.
pub const DOC_ID_DESIGN: &str = "design";

/// Section headers every server `AGENTS.md` must contain (C23).
pub const REQUIRED_AGENT_SECTIONS: [&str; 4] = [
    "## Purpose",
    "## Invariants",
    "## Build And Test",
    "## Contract Compliance",
];

/// One document embedded from the server crate at build time.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ServerDoc {
    pub id: &'static str,
    pub title: &'static str,
    #[serde(skip)]
    pub body: &'static str,
    #[serde(skip)]
    pub digest: veoveo_types::Sha256Digest,
}

/// The embedded document set a server serves under `{scheme}://docs`.
#[derive(Debug, Clone)]
pub struct ServerDocs {
    server: &'static str,
    docs: Vec<ServerDoc>,
    declaration: OnceLock<ContractDeclaration>,
    profile: Option<ComplianceProfile>,
}

impl ServerDocs {
    pub fn new(server: &'static str) -> Self {
        Self {
            server,
            docs: Vec::new(),
            declaration: OnceLock::new(),
            profile: None,
        }
    }

    pub fn with_doc(mut self, id: &'static str, title: &'static str, body: &'static str) -> Self {
        self.docs.push(ServerDoc {
            id,
            title,
            body,
            digest: knowledge_extension::content_digest(body),
        });
        self
    }

    /// Production documents use a digest emitted by the compile-time macro.
    pub fn with_embedded_doc(
        mut self,
        id: &'static str,
        title: &'static str,
        embedded: (&'static str, [u8; 32]),
    ) -> Self {
        self.docs.push(ServerDoc {
            id,
            title,
            body: embedded.0,
            digest: veoveo_types::Sha256Digest::from_bytes(embedded.1),
        });
        self
    }

    /// Admit the owner profile and compare its projection with embedded manual bytes.
    pub fn with_profile_json(mut self, bytes: &str) -> Result<Self, ComplianceError> {
        let profile: ComplianceProfile = serde_json::from_str(bytes).map_err(|error| {
            profile::invalid(format!("owner profile JSON failed admission: {error}"))
        })?;
        if profile.server().as_str() != self.server {
            return Err(profile::invalid("owner profile/server identity mismatch"));
        }
        let manual = self
            .agent_manual()
            .ok_or_else(|| profile::invalid("missing agent manual"))?;
        verify_manual(manual, &profile)?;
        self.declaration = OnceLock::new();
        self.profile = Some(profile);
        Ok(self)
    }

    pub fn admitted_profile(&self) -> Result<&ComplianceProfile, ComplianceError> {
        let profile = self.profile.as_ref().ok_or_else(|| {
            profile::invalid("served documents require an admitted owner profile")
        })?;
        verify_manual(
            self.agent_manual()
                .ok_or_else(|| profile::invalid("missing agent manual"))?,
            profile,
        )?;
        Ok(profile)
    }
    pub fn profile(&self) -> &ComplianceProfile {
        self.admitted_profile()
            .expect("served documents require an admitted owner profile")
    }

    pub fn server(&self) -> &'static str {
        self.server
    }

    pub fn doc(&self, id: &str) -> Option<&ServerDoc> {
        self.docs.iter().find(|doc| doc.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ServerDoc> {
        self.docs.iter()
    }

    /// The llms.txt index served at `{mount}/admin/docs/llms.txt` (C20).
    pub fn llms_txt(&self) -> String {
        let mut out = format!(
            "# {}\n\n> Veoveo MCP server documents. Contract revision {}.\n\n## Docs\n\n",
            self.server, CONTRACT_REVISION
        );
        for doc in &self.docs {
            out.push_str(&format!("- [{}]({})\n", doc.title, doc.id));
        }
        out
    }

    /// The agent manual embedded from the crate `AGENTS.md`, when present.
    pub fn agent_manual(&self) -> Option<&'static str> {
        self.doc(DOC_ID_AGENTS).map(|doc| doc.body)
    }

    /// Returns the declaration built once from this document set.
    ///
    /// Discover and the list methods are the only authority for the observed
    /// running protocol surface. The embedded contract resource declares only
    /// repository-owned revision and compliance evidence.
    pub fn contract_declaration(&self) -> &ContractDeclaration {
        self.declaration
            .get_or_init(|| ContractDeclaration::from_docs(self))
    }
}

/// Embeds the crate's `AGENTS.md` and `DESIGN.md` as its served document set
/// (C18, C21). Invoke from the server crate so the paths resolve against that
/// crate's manifest directory.
#[macro_export]
macro_rules! server_docs {
    ($server:expr) => {
        $crate::docs::ServerDocs::new($server)
            .with_embedded_doc(
                $crate::docs::DOC_ID_AGENTS,
                "Agent work manual",
                $crate::docs::embedded_document!("AGENTS.md"),
            )
            .with_embedded_doc(
                $crate::docs::DOC_ID_DESIGN,
                "Domain design",
                $crate::docs::embedded_document!("DESIGN.md"),
            )
            .with_profile_json(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/contract-compliance.json"
            )))
            .expect("embedded owner compliance profile and manual must agree")
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llms_txt_lists_every_document() {
        let docs = ServerDocs::new("example")
            .with_doc(DOC_ID_AGENTS, "Agent work manual", "body")
            .with_doc(DOC_ID_DESIGN, "Domain design", "body");
        let index = docs.llms_txt();
        assert!(index.starts_with("# example\n"));
        assert!(index.contains("- [Agent work manual](agents)"));
        assert!(index.contains("- [Domain design](design)"));
        assert!(index.contains(&format!("Contract revision {CONTRACT_REVISION}")));
    }

    #[test]
    fn embedded_profile_and_manual_must_agree() {
        let bytes = include_str!("../testdata/compliance-example.json");
        let manual = include_str!("../testdata/compliance-example.md");
        let docs = ServerDocs::new("example")
            .with_doc(DOC_ID_AGENTS, "Agent work manual", manual)
            .with_profile_json(bytes)
            .unwrap();
        let declaration = docs.contract_declaration();
        assert_eq!(declaration.compliance().len(), 32);
        assert!(std::ptr::eq(declaration, docs.contract_declaration()));
        assert_eq!(
            serde_json::to_value(declaration).unwrap(),
            serde_json::to_value(docs.profile()).unwrap()
        );
        assert!(
            ServerDocs::new("foreign")
                .with_doc(DOC_ID_AGENTS, "Agent work manual", manual)
                .with_profile_json(bytes)
                .is_err()
        );
        assert!(
            ServerDocs::new("example")
                .with_doc(DOC_ID_AGENTS, "Agent work manual", "altered manual")
                .with_profile_json(bytes)
                .is_err()
        );
    }
}
