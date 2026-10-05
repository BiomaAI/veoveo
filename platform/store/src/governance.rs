use surrealdb::types::SurrealValue;

use crate::{
    InvocationAuthorityRecord, InvocationMode, PlatformStore, StoreError, TenantId, WorkContextId,
    WorkContextMembershipLevel, WorkContextRecord,
};

#[derive(Clone, Debug, SurrealValue)]
pub struct WorkContextSnapshot {
    pub context: WorkContextRecord,
    pub digest: String,
}

impl PlatformStore {
    /// Admission snapshot and policy digest are read in the same database request.
    pub async fn work_context_snapshot(
        &self,
        context: WorkContextId,
    ) -> Result<WorkContextSnapshot, StoreError> {
        let mut response = self
            .client()
            .query(include_str!("queries/governance/context_snapshot.surql"))
            .bind(("context", context.record_id()))
            .await?
            .check()?;
        response
            .take::<Option<WorkContextSnapshot>>(1)?
            .ok_or(StoreError::MissingRecord {
                operation: "work context admission snapshot",
            })
    }

    pub async fn work_context_by_key(
        &self,
        tenant_id: TenantId,
        context_key: &str,
    ) -> Result<Option<WorkContextRecord>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("queries/governance/work_context_by_key.surql"))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context_key", context_key.to_owned()))
            .await?
            .check()?;
        let contexts: Vec<WorkContextRecord> = response.take(0)?;
        Ok(contexts.into_iter().next())
    }
}

impl WorkContextRecord {
    pub fn membership_for_principal(
        &self,
        principal_key: &str,
    ) -> Option<WorkContextMembershipLevel> {
        self.memberships
            .iter()
            .filter(|rule| rule.principals.iter().any(|value| value == principal_key))
            .map(|rule| rule.level)
            .max_by_key(|level| membership_rank(*level))
    }

    pub fn membership_for_oauth_client(
        &self,
        oauth_client: &str,
    ) -> Option<WorkContextMembershipLevel> {
        self.memberships
            .iter()
            .filter(|rule| rule.oauth_clients.iter().any(|value| value == oauth_client))
            .map(|rule| rule.level)
            .max_by_key(|level| membership_rank(*level))
    }

    pub fn automated_authority(
        &self,
        membership: WorkContextMembershipLevel,
    ) -> InvocationAuthorityRecord {
        InvocationAuthorityRecord {
            context_key: self.context_key.clone(),
            membership,
            policy_revision: self.policy_revision.clone(),
            owner_kind: self.output_policy.owner_kind,
            owner_key: self.output_policy.owner_key.clone(),
            initial_grants: self.output_policy.initial_grants.clone(),
            classification: self.output_policy.classification.clone(),
            data_labels: self.output_policy.data_labels.clone(),
            invocation_mode: InvocationMode::Automated,
            initiator_key: None,
            delegation_id: None,
        }
    }
}

fn membership_rank(level: WorkContextMembershipLevel) -> u8 {
    match level {
        WorkContextMembershipLevel::Viewer => 0,
        WorkContextMembershipLevel::Contributor => 1,
        WorkContextMembershipLevel::Custodian => 2,
        WorkContextMembershipLevel::Owner => 3,
    }
}
