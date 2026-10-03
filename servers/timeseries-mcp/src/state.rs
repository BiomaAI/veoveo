use std::collections::BTreeSet;
use veoveo_types::TaskId;

use veoveo_mcp_contract::GatewayProfileId;
use veoveo_types::{DataLabelId, PrincipalId, TenantId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOwner {
    pub task_id: TaskId,
    pub principal_id: PrincipalId,
    pub profile: GatewayProfileId,
    pub tenant: Option<TenantId>,
    pub data_labels: BTreeSet<DataLabelId>,
}
