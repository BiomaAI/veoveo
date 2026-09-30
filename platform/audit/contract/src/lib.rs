//! Typed audit records shared by persistence, producers and readers.
//! This crate has no database driver, MCP runtime, HTTP client or async runtime.
mod activity;
mod destination;
pub use destination::*;
mod ids;
mod model;
mod query;
mod reader;
pub use activity::*;
pub use ids::*;
pub use model::*;
pub use query::*;
pub use reader::*;
mod integrity;
pub use integrity::*;

veoveo_types::scope_enum! {
    /// The audit domain owns this scope; protocol cores only carry its validated name.
    pub enum AuditScope { Read => "audit:read" }
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
            LazyLock::new(|| veoveo_types::RoleId::new("administrator").expect("declared role"));
        static AUDITOR: LazyLock<veoveo_types::RoleId> =
            LazyLock::new(|| veoveo_types::RoleId::new("auditor").expect("declared role"));
        match self {
            Self::Administrator => &ADMIN,
            Self::Auditor => &AUDITOR,
        }
    }
}
