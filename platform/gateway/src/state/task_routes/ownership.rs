//! Durable actor/provenance identity, separate from current policy evaluation.
use std::collections::BTreeSet;

use surrealdb::types::{RecordId, SurrealValue};
use veoveo_mcp_contract::{InvocationAuthority, InvocationProvenance, Principal};
use veoveo_platform_store::{InvocationMode, PrincipalKind, StoreError};

use super::{GatewayState, GatewayTaskRouteRecord};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, SurrealValue)]
pub(crate) struct GatewayTaskOwnership {
    version: u8,
    principal_key: String,
    principal_kind: PrincipalKind,
    issuer: String,
    subject: String,
    invocation_mode: InvocationMode,
    initiator: Option<String>,
    delegation_id: Option<String>,
    data_labels: BTreeSet<String>,
}

impl GatewayTaskOwnership {
    pub(crate) fn from_invocation(actor: &Principal, authority: &InvocationAuthority) -> Self {
        let (invocation_mode, initiator, delegation_id) = match &authority.provenance {
            InvocationProvenance::Direct { initiator } => {
                (InvocationMode::Direct, Some(initiator.to_string()), None)
            }
            InvocationProvenance::Delegated {
                initiator,
                delegation_id,
            } => (
                InvocationMode::Delegated,
                Some(initiator.to_string()),
                Some(delegation_id.to_string()),
            ),
            InvocationProvenance::Automated => (InvocationMode::Automated, None, None),
        };
        Self {
            version: 1,
            principal_key: actor.id.to_string(),
            principal_kind: match actor.kind {
                veoveo_mcp_contract::PrincipalKind::User => PrincipalKind::User,
                veoveo_mcp_contract::PrincipalKind::Service => PrincipalKind::Service,
            },
            issuer: actor.issuer.to_string(),
            subject: actor.subject.to_string(),
            invocation_mode,
            initiator,
            delegation_id,
            data_labels: actor.data_labels.iter().map(ToString::to_string).collect(),
        }
    }

    pub(crate) fn allows(&self, actor: &Principal, authority: &InvocationAuthority) -> bool {
        let mut current = Self::from_invocation(actor, authority);
        if !self.data_labels.is_subset(&current.data_labels) {
            return false;
        }
        current.data_labels = self.data_labels.clone();
        self == &current
    }
}

#[derive(SurrealValue)]
struct RetainedOwnership {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    profile: RecordId,
    server: RecordId,
    ownership: GatewayTaskOwnership,
}

impl GatewayState {
    /// Version-0 first-party routes can recover their identity from the exact
    /// retained shared Task. This reads metadata only and never rewrites the Task.
    pub(crate) async fn task_route_ownership(
        &self,
        route: &GatewayTaskRouteRecord,
    ) -> Result<Option<GatewayTaskOwnership>, StoreError> {
        if let Some(ownership) = &route.ownership {
            return Ok(Some(ownership.clone()));
        }
        let Some(source) = &route.source_task else {
            return Ok(None);
        };
        let mut response = self
            .platform
            .client()
            .query(
                "SELECT tenant, owner, work_context, profile, server, {
                version: 1,
                principal_key: request.owner.principal_key,
                principal_kind: request.owner.principal_kind,
                issuer: request.owner.issuer,
                subject: request.owner.subject,
                invocation_mode: request.owner.authority.provenance.mode,
                initiator: request.owner.authority.provenance.initiator,
                delegation_id: request.owner.authority.provenance.delegation_id,
                data_labels: request.owner.data_labels
             } AS ownership FROM ONLY $source;",
            )
            .bind(("source", source.clone()))
            .await?
            .check()?;
        let retained: Option<RetainedOwnership> = response.take(0)?;
        Ok(retained
            .filter(|retained| {
                retained.tenant == route.tenant
                    && retained.owner == route.owner
                    && retained.work_context == route.work_context
                    && retained.profile == route.profile
                    && retained.server == route.server
            })
            .map(|retained| retained.ownership))
    }
}
