//! Requirement IDs and metadata; ownership is independent of runtime check IDs.
use serde::Serialize;

pub const CATALOG_REVISION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, veoveo_types::Vocabulary)]
#[schemars(transform = requirement_id_naming)]
pub enum RequirementId {
    #[vocabulary(rename = "C01")]
    C01,
    #[vocabulary(rename = "C02")]
    C02,
    #[vocabulary(rename = "C03")]
    C03,
    #[vocabulary(rename = "C04")]
    C04,
    #[vocabulary(rename = "C05")]
    C05,
    #[vocabulary(rename = "C06")]
    C06,
    #[vocabulary(rename = "C07")]
    C07,
    #[vocabulary(rename = "C08")]
    C08,
    #[vocabulary(rename = "C09")]
    C09,
    #[vocabulary(rename = "C10")]
    C10,
    #[vocabulary(rename = "C11")]
    C11,
    #[vocabulary(rename = "C12")]
    C12,
    #[vocabulary(rename = "C13")]
    C13,
    #[vocabulary(rename = "C14")]
    C14,
    #[vocabulary(rename = "C15")]
    C15,
    #[vocabulary(rename = "C16")]
    C16,
    #[vocabulary(rename = "C17")]
    C17,
    #[vocabulary(rename = "C18")]
    C18,
    #[vocabulary(rename = "C19")]
    C19,
    #[vocabulary(rename = "C20")]
    C20,
    #[vocabulary(rename = "C21")]
    C21,
    #[vocabulary(rename = "C22")]
    C22,
    #[vocabulary(rename = "C23")]
    C23,
    #[vocabulary(rename = "C24")]
    C24,
    #[vocabulary(rename = "C25")]
    C25,
    #[vocabulary(rename = "C26")]
    C26,
    #[vocabulary(rename = "C27")]
    C27,
    #[vocabulary(rename = "C28")]
    C28,
    #[vocabulary(rename = "C29")]
    C29,
    #[vocabulary(rename = "C30")]
    C30,
    #[vocabulary(rename = "C31")]
    C31,
    #[vocabulary(rename = "C32")]
    C32,
    #[vocabulary(rename = "C33")]
    C33,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum RequirementLevel {
    Must,
    #[vocabulary(rename = "must_when_adopted")]
    MustWhenAdopted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ApplicabilityCondition {
    #[vocabulary(rename = "knowledge_source")]
    KnowledgeSource,
}

#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct RequirementMetadata {
    pub id: RequirementId,
    pub level: RequirementLevel,
    pub text: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applicability: Option<ApplicabilityCondition>,
}

impl RequirementId {
    pub fn metadata(self) -> RequirementMetadata {
        let (level, text, applicability) = match self {
            Self::C01 => (
                RequirementLevel::Must,
                "Each capability uses the canonical MCP surface for its need per the Protocol Surface table.",
                None,
            ),
            Self::C02 => (
                RequirementLevel::Must,
                "Every tool declares input and output JSON Schemas; an addressable terminal product has one top-level canonical `resultUri`, while a no-product task omits it.",
                None,
            ),
            Self::C03 => (
                RequirementLevel::Must,
                "Durable operations are task-augmented tools on the shared task runtime.",
                None,
            ),
            Self::C04 => (
                RequirementLevel::Must,
                "Addressable state is exposed as resources or resource templates under the server's canonical scheme; growing collections use bounded domain-owned pages and exact reads do not scan the full collection.",
                None,
            ),
            Self::C05 => (
                RequirementLevel::Must,
                "The server is not flattened to a tool-only convenience surface.",
                None,
            ),
            Self::C06 => (
                RequirementLevel::Must,
                "Compatibility helpers are additive projections reusing canonical models, policy, audit, tasks, and URIs.",
                None,
            ),
            Self::C07 => (
                RequirementLevel::Must,
                "Tool input schemas use the supported JSON Schema 2020-12 profile, close controlled argument objects, and pass the shared document and reference-expansion bounds.",
                None,
            ),
            Self::C08 => (
                RequirementLevel::Must,
                "Schemas are generated through ordinary rmcp/Schemars or official SDK/Pydantic machinery.",
                None,
            ),
            Self::C09 => (
                RequirementLevel::Must,
                "Controlled shapes use strong domain types; raw JSON only at open boundaries.",
                None,
            ),
            Self::C10 => (
                RequirementLevel::Must,
                "Shared mechanics, including the final 8 MiB serialized JSON response cap, come from `veoveo_mcp_contract`, not reimplementation.",
                None,
            ),
            Self::C11 => (
                RequirementLevel::Must,
                "Artifact and recording operations use the forwarded internal identity; durable Artifact reads may use the service's explicitly issued bounded task-read capability.",
                None,
            ),
            Self::C12 => (
                RequirementLevel::Must,
                "Administrative HTTP exists only under the canonical mount.",
                None,
            ),
            Self::C13 => (RequirementLevel::Must, "No private control database.", None),
            Self::C14 => (RequirementLevel::Must, "No private byte route.", None),
            Self::C15 => (
                RequirementLevel::Must,
                "The server ships as an OCI image with a versioned Helm chart.",
                None,
            ),
            Self::C16 => (
                RequirementLevel::Must,
                "The gateway entry is registered in the typed control plane with routes, capabilities, and policy.",
                None,
            ),
            Self::C17 => (
                RequirementLevel::Must,
                "The registration and crate documents state the contract revision.",
                None,
            ),
            Self::C18 => (
                RequirementLevel::Must,
                "Docs resources are served under `{scheme}://docs`.",
                None,
            ),
            Self::C19 => (
                RequirementLevel::Must,
                "The contract declaration resource is served at `{scheme}://contract`.",
                None,
            ),
            Self::C20 => (
                RequirementLevel::Must,
                "The admin mount serves `docs/llms.txt` and document bodies.",
                None,
            ),
            Self::C21 => (
                RequirementLevel::Must,
                "Served documents are embedded at build time from the crate.",
                None,
            ),
            Self::C22 => (
                RequirementLevel::Must,
                "`DESIGN.md` exists beside the crate and pins the domain profile.",
                None,
            ),
            Self::C23 => (
                RequirementLevel::Must,
                "`AGENTS.md` exists beside the crate with the required sections.",
                None,
            ),
            Self::C24 => (RequirementLevel::Must, "The crate is named `*-mcp`.", None),
            Self::C25 => (
                RequirementLevel::Must,
                "Every network endpoint uses Streamable HTTP; stdio exists only inside a local bridge that owns its child.",
                None,
            ),
            Self::C26 => (
                RequirementLevel::Must,
                "Streamable HTTP is stateless, requires final per-request protocol metadata, returns JSON terminal responses, and rejects session, reconnect, DELETE, and replay surfaces.",
                None,
            ),
            Self::C27 => (
                RequirementLevel::Must,
                "`subscriptions/listen` uses an authorized accepted filter, a request-scoped sink, bounded backpressure, and a shared restart-safe event source; legacy subscribe and unsubscribe are absent.",
                None,
            ),
            Self::C28 => (
                RequirementLevel::Must,
                "Tool, prompt, and resource list-change capabilities are declared independently and match emitted notifications.",
                None,
            ),
            Self::C29 => (
                RequirementLevel::Must,
                "Ordinary hosted servers and the gateway tolerate load-balanced replica changes; singleton workloads name the real exclusive state or GPU owner.",
                None,
            ),
            Self::C30 => (
                RequirementLevel::Must,
                "Requests with equivalent upstream transport security share one catalog-revision-scoped HTTP connection pool and TLS trust store without sharing request authority.",
                None,
            ),
            Self::C31 => (
                RequirementLevel::Must,
                "Readiness calls Discover and required list methods, compares the observed surface with installation allow/require policy, and fails closed on mismatch.",
                None,
            ),
            Self::C32 => (
                RequirementLevel::MustWhenAdopted,
                "A server that declares `ai.veoveo/knowledge-source` publishes its `{slug}.docs` collection and satisfies K01–K10 for every declared collection under the [knowledge extension](../knowledge-extension/DESIGN.md). A server that does not declare the extension marks C32 not applicable.",
                Some(ApplicabilityCondition::KnowledgeSource),
            ),
            Self::C33 => (
                RequirementLevel::Must,
                "Discovery identifiers, controlled DTO fields and values, adopted extension metadata and selected owner schema evidence satisfy the naming classes; narrow owner, JWT, frozen and external declarations require source review.",
                None,
            ),
        };
        RequirementMetadata {
            id: self,
            level,
            text,
            applicability,
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequirementCatalog {
    pub catalog_revision: u32,
    pub contract_revision: u32,
    /// Actual Rust Unicode White_Space admission rule for blank author notes.
    pub note_whitespace: String,
    pub requirements: Vec<RequirementMetadata>,
}

pub fn requirement_catalog() -> RequirementCatalog {
    RequirementCatalog {
        catalog_revision: CATALOG_REVISION,
        contract_revision: super::CONTRACT_REVISION,
        note_whitespace: (0..=0x10ffff)
            .filter_map(char::from_u32)
            .filter(|character| character.is_whitespace())
            .collect(),
        requirements: RequirementId::ALL.iter().map(|id| id.metadata()).collect(),
    }
}

fn requirement_id_naming(schema: &mut schemars::Schema) {
    let profile = veoveo_types::ScalarNaming::owner(module_path!(), "requirement-catalog-identity")
        .expect("static owner naming declaration");
    *schema = veoveo_types::scalar_schema(schema.clone(), profile)
        .expect("owned format schema is scalar");
}
