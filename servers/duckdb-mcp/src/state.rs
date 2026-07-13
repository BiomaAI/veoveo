use std::collections::BTreeSet;

use veoveo_mcp_contract::{DataLabelId, GatewayProfileId, PrincipalId, TenantId};

use crate::contract::DuckDbDatabaseId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOwner {
    pub task_id: String,
    pub principal_id: PrincipalId,
    pub profile: GatewayProfileId,
    pub tenant: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
}

/// Derived identity for one owner-scoped mutable analytical database file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseOwner {
    pub db_id: DuckDbDatabaseId,
    pub principal_id: PrincipalId,
    pub profile: GatewayProfileId,
    pub tenant: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
    pub file_path: String,
}
