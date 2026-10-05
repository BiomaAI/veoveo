//! Caller authority shared by Recording grant and projection admission.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_types::{DataLabelId, PolicyVersion};

use veoveo_platform_store::{PrincipalId, TenantId, WorkContextId};

/// Supplied by the authenticated service after policy admission.
#[derive(Clone, Debug)]
pub struct RecordingAccessScope {
    pub tenant_id: TenantId,
    pub actor_id: PrincipalId,
    pub work_context_id: WorkContextId,
    pub policy_revision: PolicyVersion,
    pub data_labels: BTreeSet<DataLabelId>,
}

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
pub(super) struct ScopeBindings {
    tenant: RecordId,
    actor: RecordId,
    context: RecordId,
    policy: String,
    labels: Vec<String>,
}

impl RecordingAccessScope {
    pub(super) fn bindings(&self) -> ScopeBindings {
        ScopeBindings {
            tenant: self.tenant_id.record_id(),
            actor: self.actor_id.record_id(),
            context: self.work_context_id.record_id(),
            policy: self.policy_revision.to_string(),
            labels: self.data_labels.iter().map(ToString::to_string).collect(),
        }
    }
}
