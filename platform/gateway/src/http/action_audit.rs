use super::auth_support::internal_error_response;
use crate::{AuthenticatedSubject, GatewayCatalog, GatewayState, PolicyRequest};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{sync::Arc, time::Instant};
use veoveo_gateway_contract::{ActionAccess, GatewayAction, PolicyAction};
pub use veoveo_mcp_contract::audit::{AdminOperationFailure, AdministrativeOperation};
use veoveo_mcp_contract::{
    GatewayProfile, GatewayProfileId, PolicyDecision, PolicyEffect, PolicyTarget, TraceId,
    audit::{
        AdministrativeAccess, AuditDetail, AuditDraft, AuditOutcome, AuditReason, AuditTarget,
    },
};
pub struct AdminAuthorizationRequest {
    pub audit_target: Option<AuditTarget>,
    pub action: PolicyAction,
    pub target: PolicyTarget,
    pub operation: AdministrativeOperation,
    pub started_at: Instant,
}

pub async fn authorize_gateway_action(
    gateway: &GatewayState,
    catalog: Arc<GatewayCatalog>,
    profile_id: &GatewayProfileId,
    subject: AuthenticatedSubject,
    request: AdminAuthorizationRequest,
) -> std::result::Result<(Arc<GatewayCatalog>, GatewayProfile, AuthenticatedSubject), Box<Response>>
{
    let Some(profile) = catalog.profile(profile_id).cloned() else {
        return Err(Box::new(StatusCode::NOT_FOUND.into_response()));
    };
    let trace_id = match TraceId::new(subject.audit.trace_id.to_string()) {
        Ok(trace_id) => trace_id,
        Err(err) => return Err(Box::new(internal_error_response(err))),
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: profile_id,
        action: request.action.clone(),
        target: &request.target,
        trace_id: &trace_id,
    });
    if let Err(err) = record_admin_audit(
        gateway,
        &profile,
        &subject,
        AdminAuditRecord {
            access: administrative_access(catalog.registry(), &request.action)
                .map_err(|error| Box::new(internal_error_response(error)))?,
            target: request.audit_target.unwrap_or(
                crate::audit::policy_audit_target(catalog.registry(), &request.target)
                    .map_err(|error| Box::new(internal_error_response(error)))?,
            ),
            decision: decision.clone(),
            operation: request.operation,
            started_at: request.started_at,
        },
    )
    .await
    {
        return Err(Box::new(internal_error_response(err)));
    }
    if decision.effect != PolicyEffect::Allow {
        tracing::warn!(
            profile = %profile_id,
            principal = %subject.principal.id,
            action = ?request.action,
            reason = ?decision.reason,
            "gateway admin request denied"
        );
        return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
    }

    Ok((catalog, profile, subject))
}

struct AdminAuditRecord {
    access: AdministrativeAccess,
    target: AuditTarget,
    decision: PolicyDecision,
    operation: AdministrativeOperation,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub enum AdminOperationStatus {
    Succeeded,
    Rejected,
    Failed,
}

pub struct AdminOperationAuditRecord {
    pub audit_target: Option<AuditTarget>,
    pub action: PolicyAction,
    pub operation: AdministrativeOperation,
    pub started_at: Instant,
    pub status: AdminOperationStatus,
    pub failure: Option<AdminOperationFailure>,
}

pub async fn record_gateway_operation_audit(
    gateway: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    catalog: &GatewayCatalog,
    target: PolicyTarget,
    record: AdminOperationAuditRecord,
) -> anyhow::Result<()> {
    let (outcome, reason) = match record.status {
        AdminOperationStatus::Succeeded => (AuditOutcome::Succeeded, AuditReason::Accepted),
        AdminOperationStatus::Rejected => (AuditOutcome::Failed, AuditReason::InvalidRequest),
        AdminOperationStatus::Failed => (AuditOutcome::Failed, AuditReason::InternalFailure),
    };
    let target = match record.audit_target {
        Some(target) => target,
        None => crate::audit::policy_audit_target(catalog.registry(), &target)?,
    };
    let draft = AuditDraft::builder(
        subject.audit.clone(),
        target,
        AuditDetail::AdminCompletion {
            operation: record.operation,
            access: administrative_access(catalog.registry(), &record.action)?,
            failure: record.failure,
        },
        outcome,
        reason,
    )
    .actor(subject.audit_actor()?)
    .authority(subject.audit_authority(&profile.id))
    .latency_ms(u64::try_from(record.started_at.elapsed().as_millis())?)
    .build()?;
    gateway.audit_writer().await.record_completion(draft).await;
    Ok(())
}

fn administrative_access(
    registry: &veoveo_gateway_contract::CatalogRegistry,
    action: &PolicyAction,
) -> anyhow::Result<AdministrativeAccess> {
    Ok(match action {
        PolicyAction::Kernel(GatewayAction::AdminRead | GatewayAction::ArtifactRead) => {
            AdministrativeAccess::Read
        }
        PolicyAction::Kernel(_) => AdministrativeAccess::Write,
        PolicyAction::Registered(handle) => {
            registry.check_action(handle)?;
            match registry
                .descriptor(handle.name())
                .ok_or_else(|| anyhow::anyhow!("owner action audit descriptor is unbound"))?
                .access
            {
                ActionAccess::Read => AdministrativeAccess::Read,
                ActionAccess::Write => AdministrativeAccess::Write,
            }
        }
    })
}

async fn record_admin_audit(
    gateway: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    record: AdminAuditRecord,
) -> anyhow::Result<()> {
    let draft = AuditDraft::builder(
        subject.audit.clone(),
        record.target,
        AuditDetail::AdminAdmission {
            operation: record.operation,
            access: record.access,
        },
        if record.decision.effect == PolicyEffect::Allow {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        },
        crate::audit::policy_reason(record.decision.reason),
    )
    .actor(subject.audit_actor()?)
    .authority(subject.audit_authority(&profile.id))
    .latency_ms(u64::try_from(record.started_at.elapsed().as_millis())?)
    .build()?;
    gateway.record_audit(draft).await
}
