//! Typed audit records shared by persistence, producers and readers.
//! This crate has no database driver, MCP runtime, HTTP client or async runtime.
mod activity;
mod destination;
pub use destination::*;
mod ids;
mod indexing;
pub use indexing::*;
mod knowledge;
mod model;
mod query;
mod reader;
pub use activity::*;
pub use ids::*;
pub use knowledge::*;
pub use model::*;
pub use query::*;
pub use reader::*;
mod integrity;
pub use integrity::*;

/// The audit domain owns this scope; protocol cores only carry its validated name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum AuditScope {
    #[vocabulary(rename = "audit:read")]
    Read,
}

#[derive(Debug, Clone, Copy)]
pub enum InstallationAuditRole {
    Administrator,
    Auditor,
}
impl InstallationAuditRole {
    pub fn id(self) -> &'static veoveo_types::RoleId {
        use std::sync::LazyLock;
        static ADMIN: LazyLock<veoveo_types::RoleId> =
            LazyLock::new(|| veoveo_types::RoleId::parse("administrator").expect("declared role"));
        static AUDITOR: LazyLock<veoveo_types::RoleId> =
            LazyLock::new(|| veoveo_types::RoleId::parse("auditor").expect("declared role"));
        match self {
            Self::Administrator => &ADMIN,
            Self::Auditor => &AUDITOR,
        }
    }
}

mod targets;
pub use targets::*;
mod decoder;
pub use decoder::*;
