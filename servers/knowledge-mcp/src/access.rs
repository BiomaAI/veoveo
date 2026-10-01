use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_mcp_contract::access::{AccessRequest, GroupMembership, decide};
use veoveo_mcp_knowledge_extension::{
    CollectionDescriptor, CollectionId, Observation, ReadGrant, ReadPolicy,
};
use veoveo_platform_store::knowledge::CandidateScope;
use veoveo_types::{
    AccessLevel, AccessSubject, DataLabelId, GatewayProfileId, PrincipalId, ResourceSelection,
    ResourceUri, ScopeName, TenantId, WorkContextId, WorkContextMembershipLevel,
};

/// Current authenticated authority, assembled by the gateway-facing adapter.
/// Collections are those exposed by the caller's current profile, never input from
/// a search request. Work contexts have read membership already resolved.
pub struct SearchCaller {
    pub principal: PrincipalId,
    pub tenant: TenantId,
    pub profile: GatewayProfileId,
    pub active_work_context: WorkContextId,
    pub collections: BTreeMap<CollectionId, ResourceSelection>,
    pub work_contexts: BTreeSet<WorkContextId>,
    pub memberships: BTreeSet<GroupMembership>,
    pub scopes: BTreeSet<ScopeName>,
    pub clearance: BTreeSet<DataLabelId>,
}
impl SearchCaller {
    /// Resolve search admission from one current policy snapshot and authenticated
    /// invocation. The transport proves identity/session freshness before calling.
    pub fn from_policy<'a>(
        catalog: &veoveo_policy::PolicyCatalog,
        principal: &veoveo_mcp_contract::Principal,
        profile: &GatewayProfileId,
        client: &veoveo_types::OAuthClientId,
        authority: &veoveo_types::InvocationAuthority,
        descriptors: impl IntoIterator<Item = &'a CollectionDescriptor>,
    ) -> Result<Self, crate::ServiceError> {
        let memberships = catalog
            .control_plane()
            .work_contexts
            .iter()
            .filter_map(|context| {
                context
                    .membership_for(principal, client)
                    .map(|level| (context.id.clone(), level))
            })
            .collect();
        Self::from_current_memberships(
            catalog,
            principal,
            principal,
            profile,
            authority,
            &memberships,
            descriptors,
        )
    }

    /// Transport-owned current membership; never deserialized from tool input.
    #[allow(
        clippy::too_many_arguments,
        reason = "signed actor and source policy remain distinct"
    )]
    pub(crate) fn from_current_memberships<'a>(
        catalog: &veoveo_policy::PolicyCatalog,
        principal: &veoveo_mcp_contract::Principal,
        actor: &veoveo_mcp_contract::Principal,
        profile: &GatewayProfileId,
        authority: &veoveo_types::InvocationAuthority,
        memberships: &BTreeMap<WorkContextId, WorkContextMembershipLevel>,
        descriptors: impl IntoIterator<Item = &'a CollectionDescriptor>,
    ) -> Result<Self, crate::ServiceError> {
        if principal.tenant.as_ref() != Some(&authority.tenant) || actor.tenant != principal.tenant
        {
            return Err(crate::ServiceError::AccessChanged);
        }
        let contexts = &catalog.control_plane().work_contexts;
        let current = contexts
            .iter()
            .find(|context| context.id == authority.work_context)
            .ok_or(crate::ServiceError::AccessChanged)?;
        if current.tenant != authority.tenant
            || current.policy_revision != authority.policy_revision
            || memberships.get(&authority.work_context) != Some(&authority.membership)
        {
            return Err(crate::ServiceError::AccessChanged);
        }
        // Evaluate once per source, then attach its selection to each admitted
        // collection. Search inputs may only narrow this set.
        let mut sources = BTreeMap::new();
        let collections = descriptors
            .into_iter()
            .filter_map(|descriptor| {
                if !descriptor.required_scopes().is_subset(&principal.scopes) {
                    return None;
                }
                let selection = sources
                    .entry(descriptor.collection().server().clone())
                    .or_insert_with(|| {
                        veoveo_policy::admit_resource_reads(
                            catalog,
                            principal,
                            profile,
                            descriptor.collection().server(),
                        )
                        .ok()
                    });
                selection
                    .clone()
                    .map(|selection| (descriptor.collection().clone(), selection))
            })
            .collect();
        Ok(Self {
            principal: actor.id.clone(),
            tenant: authority.tenant.clone(),
            profile: profile.clone(),
            active_work_context: authority.work_context.clone(),
            collections,
            work_contexts: contexts
                .iter()
                .filter(|context| {
                    context.tenant == authority.tenant && memberships.contains_key(&context.id)
                })
                .map(|context| context.id.clone())
                .collect(),
            memberships: actor.group_memberships(),
            scopes: actor.scopes.clone(),
            clearance: actor.data_labels.clone(),
        })
    }

    pub fn scope(&self, selected: &BTreeSet<CollectionId>) -> CandidateScope {
        CandidateScope {
            tenant: self.tenant.clone(),
            profile: self.profile.clone(),
            active_work_context: self.active_work_context.clone(),
            collections: self
                .collections
                .iter()
                .filter(|(id, _)| selected.is_empty() || selected.contains(*id))
                .map(|(id, selection)| (id.clone(), selection.clone()))
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
        uri: &ResourceUri,
        observation: &Observation,
        now: DateTime<Utc>,
    ) -> bool {
        if !self
            .collections
            .get(descriptor.collection())
            .is_some_and(|selection| selection.matches_uri(uri))
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
