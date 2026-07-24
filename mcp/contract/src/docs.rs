//! Embedded server documents and the contract self-declaration.
//!
//! Implements the Well-Known Surface of [`mcp/contract/DESIGN.md`] (C18-C21):
//! documents embedded at build time from the server crate, the
//! machine-readable contract declaration served at `{scheme}://contract`, and
//! llms.txt rendering for the administrative mount. Servers obtain the
//! document set with the [`server_docs!`](crate::server_docs) macro so the
//! deployed binary serves the manual of exactly the version it was built
//! from.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The normative contract revision this crate implements.
pub const CONTRACT_REVISION: u32 = 2;

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

/// Stable identifiers of the compliance checklist in `DESIGN.md`.
pub const CHECKLIST_IDS: [&str; 29] = [
    "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09", "C10", "C11", "C12", "C13",
    "C14", "C15", "C16", "C17", "C18", "C19", "C20", "C21", "C22", "C23", "C24", "C25", "C26",
    "C27", "C28", "C29",
];

/// One document embedded from the server crate at build time.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ServerDoc {
    pub id: &'static str,
    pub title: &'static str,
    #[serde(skip)]
    pub body: &'static str,
}

/// The embedded document set a server serves under `{scheme}://docs`.
#[derive(Debug, Clone)]
pub struct ServerDocs {
    server: &'static str,
    docs: Vec<ServerDoc>,
}

impl ServerDocs {
    pub fn new(server: &'static str) -> Self {
        Self {
            server,
            docs: Vec::new(),
        }
    }

    pub fn with_doc(mut self, id: &'static str, title: &'static str, body: &'static str) -> Self {
        self.docs.push(ServerDoc { id, title, body });
        self
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
            out.push_str(&format!("- [{}](docs/{})\n", doc.title, doc.id));
        }
        out
    }

    /// The agent manual embedded from the crate `AGENTS.md`, when present.
    pub fn agent_manual(&self) -> Option<&'static str> {
        self.doc(DOC_ID_AGENTS).map(|doc| doc.body)
    }
}

/// Declared status of one checklist item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceStatus {
    Met,
    Pending,
}

/// One checklist item as declared in a server's `Contract Compliance` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ComplianceItem {
    pub id: String,
    pub status: ComplianceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The protocol surface a server advertises, as stable name lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityInventory {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_templates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<String>,
}

/// The machine-readable declaration served at `{scheme}://contract` (C19).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContractDeclaration {
    pub server: String,
    pub contract_revision: u32,
    pub compliance: Vec<ComplianceItem>,
    #[serde(default)]
    pub capabilities: CapabilityInventory,
}

impl ContractDeclaration {
    /// Builds the declaration from the embedded agent manual so the served
    /// declaration and the crate `AGENTS.md` cannot diverge.
    pub fn from_docs(docs: &ServerDocs, capabilities: CapabilityInventory) -> Self {
        let compliance = docs
            .agent_manual()
            .map(parse_compliance)
            .unwrap_or_default();
        Self {
            server: docs.server().to_string(),
            contract_revision: CONTRACT_REVISION,
            compliance,
            capabilities,
        }
    }
}

/// Parses `- Cnn: met` and `- Cnn: pending — reason` lines from the
/// `## Contract Compliance` section of an agent manual.
pub fn parse_compliance(manual: &str) -> Vec<ComplianceItem> {
    let mut in_section = false;
    let mut items = Vec::new();
    for line in manual.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed == "## Contract Compliance";
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(entry) = trimmed.strip_prefix("- C") else {
            continue;
        };
        let Some((number, rest)) = entry.split_once(':') else {
            continue;
        };
        let id = format!("C{}", number.trim());
        let rest = rest.trim();
        let (status, remainder) = if let Some(remainder) = rest.strip_prefix("met") {
            (ComplianceStatus::Met, remainder)
        } else if let Some(remainder) = rest.strip_prefix("pending") {
            (ComplianceStatus::Pending, remainder)
        } else {
            continue;
        };
        let note = remainder.trim_start_matches([' ', '\u{2014}', '-']).trim();
        items.push(ComplianceItem {
            id,
            status,
            note: (!note.is_empty()).then(|| note.to_string()),
        });
    }
    items
}

/// Embeds the crate's `AGENTS.md` and `DESIGN.md` as its served document set
/// (C18, C21). Invoke from the server crate so the paths resolve against that
/// crate's manifest directory.
#[macro_export]
macro_rules! server_docs {
    ($server:expr) => {
        $crate::docs::ServerDocs::new($server)
            .with_doc(
                $crate::docs::DOC_ID_AGENTS,
                "Agent work manual",
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/AGENTS.md")),
            )
            .with_doc(
                $crate::docs::DOC_ID_DESIGN,
                "Domain design",
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/DESIGN.md")),
            )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANUAL: &str = "# Example\n\n## Purpose\n\nText.\n\n## Contract Compliance\n\nContract revision: 2\n\n- C01: met\n- C02: pending — well-known surface not yet wired\n- C03: pending - unverified\n\n## Build And Test\n\n- cargo test\n";

    #[test]
    fn parses_met_and_pending_items_within_section_bounds() {
        let items = parse_compliance(MANUAL);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, "C01");
        assert_eq!(items[0].status, ComplianceStatus::Met);
        assert_eq!(items[0].note, None);
        assert_eq!(items[1].status, ComplianceStatus::Pending);
        assert_eq!(
            items[1].note.as_deref(),
            Some("well-known surface not yet wired")
        );
        assert_eq!(items[2].note.as_deref(), Some("unverified"));
    }

    #[test]
    fn llms_txt_lists_every_document() {
        let docs = ServerDocs::new("example")
            .with_doc(DOC_ID_AGENTS, "Agent work manual", "body")
            .with_doc(DOC_ID_DESIGN, "Domain design", "body");
        let index = docs.llms_txt();
        assert!(index.starts_with("# example\n"));
        assert!(index.contains("- [Agent work manual](docs/agents)"));
        assert!(index.contains("- [Domain design](docs/design)"));
        assert!(index.contains(&format!("Contract revision {CONTRACT_REVISION}")));
    }

    #[test]
    fn declaration_derives_from_the_embedded_manual() {
        let docs = ServerDocs::new("example").with_doc(DOC_ID_AGENTS, "Agent work manual", MANUAL);
        let declaration = ContractDeclaration::from_docs(&docs, CapabilityInventory::default());
        assert_eq!(declaration.server, "example");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        assert_eq!(declaration.compliance.len(), 3);
        let json = serde_json::to_string(&declaration).unwrap();
        let back: ContractDeclaration = serde_json::from_str(&json).unwrap();
        assert_eq!(back, declaration);
    }

    #[test]
    fn checklist_ids_are_dense_and_stable() {
        assert_eq!(CHECKLIST_IDS.len(), 24);
        for (index, id) in CHECKLIST_IDS.iter().enumerate() {
            assert_eq!(*id, format!("C{:02}", index + 1));
        }
    }
}
