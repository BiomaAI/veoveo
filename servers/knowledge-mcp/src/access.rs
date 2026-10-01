use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use veoveo_mcp_contract::access::{AccessRequest, GroupMembership, decide};
use veoveo_mcp_knowledge_extension::{
    CollectionDescriptor, CollectionId, Observation, ReadGrant, ReadPolicy,
};
use veoveo_platform_store::knowledge::CandidateScope;
use veoveo_types::{
    AccessLevel, AccessSubject, DataLabelId, GatewayProfileId, PrincipalId, ScopeName, TenantId,
    WorkContextId, WorkContextMembershipLevel,
};

/// Current authenticated authority, assembled by the gateway-facing adapter.
/// Collections are those exposed by the caller's current profile, never input from
/// a search request. Work contexts have read membership already resolved.
pub struct SearchCaller {
    pub principal: PrincipalId,
    pub tenant: TenantId,
    pub profile: GatewayProfileId,
    pub active_work_context: WorkContextId,
    pub collections: BTreeSet<CollectionId>,
    pub work_contexts: BTreeSet<WorkContextId>,
    pub memberships: BTreeSet<GroupMembership>,
    pub scopes: BTreeSet<ScopeName>,
    pub clearance: BTreeSet<DataLabelId>,
}
impl SearchCaller {
    pub fn scope(&self, selected: &BTreeSet<CollectionId>) -> CandidateScope {
        CandidateScope {
            tenant: self.tenant.clone(),
            profile: self.profile.clone(),
            active_work_context: self.active_work_context.clone(),
            collections: self
                .collections
                .iter()
                .filter(|id| selected.is_empty() || selected.contains(*id))
                .cloned()
                .collect(),
            work_contexts: self.work_contexts.clone(),
            scopes: self.scopes.clone(),
            clearance: self.clearance.clone(),
            subjects: std::iter::once(AccessSubject::Principal(self.principal.clone()))
                .chain(
                    self.memberships
                        .iter()
                        .map(|m| AccessSubject::Group(m.group.clone())),
                )
                .collect(),
        }
    }

    pub fn allows(
        &self,
        descriptor: &CollectionDescriptor,
        observation: &Observation,
        now: DateTime<Utc>,
    ) -> bool {
        if !self.collections.contains(descriptor.collection())
            || !descriptor.required_scopes().is_subset(&self.scopes)
            || observation.validate_collection(descriptor).is_err()
        {
            return false;
        }
        let Some(access) = observation.access() else {
            return true;
        };
        if access.expires_at.is_some_and(|expires| expires <= now) {
            return false;
        }
        let selected_member = access.work_context == self.active_work_context
            && self.work_contexts.contains(&access.work_context);
        let mut grants = access.grants.clone();
        grants.push(ReadGrant::new(access.owner.clone()));
        let context_read = match &access.read_policy {
            ReadPolicy::Tenant {} => {
                // The source explicitly shares with every admitted tenant caller.
                grants.push(ReadGrant::new(AccessSubject::Principal(
                    self.principal.clone(),
                )));
                false
            }
            ReadPolicy::Subjects {} => false,
            ReadPolicy::WorkContext {} => self.work_contexts.contains(&access.work_context),
            ReadPolicy::SelectedWorkContext {} => selected_member,
            ReadPolicy::SelectedWorkContextMembers {} => {
                grants.clear();
                selected_member
            }
            ReadPolicy::SubjectsInContext { profile } => {
                if access.work_context != self.active_work_context
                    || profile.as_ref().is_some_and(|p| p != &self.profile)
                {
                    return false;
                }
                false
            }
        };
        decide(&AccessRequest {
            now,
            caller_id: &self.principal,
            caller_tenant: Some(&self.tenant),
            caller_labels: &self.clearance,
            memberships: &self.memberships,
            resource_tenant: &access.tenant,
            resource_labels: &access.data_labels.iter().cloned().collect(),
            grants: &grants,
            context_membership: context_read.then_some(WorkContextMembershipLevel::Viewer),
            requested: AccessLevel::Read,
        })
        .is_allowed()
    }
}
