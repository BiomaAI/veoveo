use std::{sync::Arc, time::Instant};
use veoveo_mcp_contract::audit::AdministrativeAccess;
pub(super) use veoveo_mcp_contract::audit::{AdminOperationFailure, AdministrativeOperation};
use veoveo_mcp_contract::audit::{
    AuditActor, AuditAuthority, AuditDetail, AuditDraft, AuditOutcome, AuditPrincipalKind,
    AuditReason, AuditRequest, AuditTarget, AuthenticationActivity,
};

use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, header::WWW_AUTHENTICATE},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    AuthMethod, AuthOutcome, AuthReasonCode, GatewayAction, GatewayControlPlane, GatewayProfile,
    GatewayProfileId, OAuthClientId, PolicyDecision, PolicyEffect, PolicyTarget, Principal,
    ProtectedResourceId, ResourceAuthorizationServer, TraceId,
};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, GatewayCatalog, GatewayState, PolicyRequest, www_authenticate_challenge,
};
use veoveo_types::PrincipalId;

use crate::runtime::{AdminState, ProfileAuthState, current_catalog};

pub(super) async fn authorize_admin_request(
    state: &AdminState,
    profile_id: &GatewayProfileId,
    subject: AuthenticatedSubject,
    action: GatewayAction,
    operation: AdministrativeOperation,
    started_at: Instant,
) -> std::result::Result<(Arc<GatewayCatalog>, GatewayProfile, AuthenticatedSubject), Box<Response>>
{
    authorize_admin_target_request(
        state,
        profile_id,
        subject,
        AdminAuthorizationRequest {
            audit_target: None,
            action,
            target: PolicyTarget::Gateway,
            operation,
            started_at,
        },
    )
    .await
}

pub(super) struct AdminAuthorizationRequest {
    pub(super) audit_target: Option<AuditTarget>,
    pub(super) action: GatewayAction,
    pub(super) target: PolicyTarget,
    pub(super) operation: AdministrativeOperation,
    pub(super) started_at: Instant,
}

pub(super) async fn authorize_admin_target_request(
    state: &AdminState,
    profile_id: &GatewayProfileId,
    subject: AuthenticatedSubject,
    request: AdminAuthorizationRequest,
) -> std::result::Result<(Arc<GatewayCatalog>, GatewayProfile, AuthenticatedSubject), Box<Response>>
{
    let catalog = current_catalog(&state.catalog);
    authorize_gateway_action(&state.gateway_state, catalog, profile_id, subject, request).await
}

pub(super) async fn authorize_gateway_action(
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
        action: request.action,
        target: &request.target,
        trace_id: &trace_id,
    });
    if let Err(err) = record_admin_audit(
        gateway,
        &profile,
        &subject,
        AdminAuditRecord {
            action: request.action,
            target: request.audit_target.unwrap_or(
                veoveo_mcp_gateway::audit::mcp_audit_target(&request.target)
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

pub(super) fn control_plane_sha256(control_plane: &GatewayControlPlane) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(control_plane)?;
    let digest = Sha256::digest(bytes);
    Ok(digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}

struct AdminAuditRecord {
    action: GatewayAction,
    target: AuditTarget,
    decision: PolicyDecision,
    operation: AdministrativeOperation,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AdminOperationStatus {
    Succeeded,
    Rejected,
    Failed,
}

pub(super) struct AdminOperationAuditRecord {
    pub(super) audit_target: Option<AuditTarget>,
    pub(super) action: GatewayAction,
    pub(super) operation: AdministrativeOperation,
    pub(super) started_at: Instant,
    pub(super) status: AdminOperationStatus,
    pub(super) failure: Option<AdminOperationFailure>,
}

pub(super) async fn record_admin_operation_audit(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    record: AdminOperationAuditRecord,
) -> anyhow::Result<()> {
    record_admin_target_operation_audit(state, profile, subject, PolicyTarget::Gateway, record)
        .await
}

pub(super) async fn record_admin_target_operation_audit(
    state: &AdminState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    record: AdminOperationAuditRecord,
) -> anyhow::Result<()> {
    record_gateway_operation_audit(&state.gateway_state, profile, subject, target, record).await
}

pub(super) async fn record_gateway_operation_audit(
    gateway: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
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
        None => veoveo_mcp_gateway::audit::mcp_audit_target(&target)?,
    };
    let draft = AuditDraft::builder(
        subject.audit.clone(),
        target,
        AuditDetail::AdminCompletion {
            operation: record.operation,
            access: administrative_access(record.action),
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

pub(super) fn managed_instance_audit_target(
    tenant: &veoveo_types::TenantId,
    instance: &veoveo_types::AgentManagedInstanceId,
) -> AuditTarget {
    use veoveo_types::{ResourceUriBuilder, UriSegment};
    AuditTarget::PlatformResource {
        uri: ResourceUriBuilder::new("veoveo://agent-instances")
            .expect("declared route")
            .segment(UriSegment::new(instance.to_string()).expect("checked instance"))
            .query_pair("tenant", tenant.as_str())
            .expect("checked tenant")
            .build()
            .expect("typed instance address"),
    }
}

pub(super) fn agent_definition_audit_target(
    tenant: &veoveo_types::TenantId,
    definition: &veoveo_types::AgentDefinitionId,
) -> AuditTarget {
    use veoveo_types::{ResourceUriBuilder, UriSegment};
    AuditTarget::PlatformResource {
        uri: ResourceUriBuilder::new("veoveo://agent-definitions")
            .expect("declared route")
            .segment(UriSegment::new(definition.to_string()).expect("checked definition"))
            .query_pair("tenant", tenant.as_str())
            .expect("checked tenant")
            .build()
            .expect("typed definition address"),
    }
}

pub(super) fn agent_management_operation(
    action: GatewayAction,
) -> anyhow::Result<AdministrativeOperation> {
    Ok(match action {
        GatewayAction::AgentDefinitionsRead => AdministrativeOperation::AgentDefinitionsRead,
        GatewayAction::AgentDefinitionsReadContent => {
            AdministrativeOperation::AgentDefinitionsReadContent
        }
        GatewayAction::AgentDefinitionsCreate => AdministrativeOperation::AgentDefinitionsCreate,
        GatewayAction::AgentDefinitionsEdit => AdministrativeOperation::AgentDefinitionsEdit,
        GatewayAction::AgentDefinitionsPublish => AdministrativeOperation::AgentDefinitionsPublish,
        GatewayAction::AgentDefinitionsUse => AdministrativeOperation::AgentDefinitionsUse,
        GatewayAction::AgentDefinitionsControl => AdministrativeOperation::AgentDefinitionsControl,
        GatewayAction::AgentDefinitionsArchive => AdministrativeOperation::AgentDefinitionsArchive,
        GatewayAction::AgentDefinitionsTransfer => {
            AdministrativeOperation::AgentDefinitionsTransfer
        }
        GatewayAction::AgentInstancesDeploy => AdministrativeOperation::AgentInstancesDeploy,
        GatewayAction::AgentInstancesControl => AdministrativeOperation::AgentInstancesControl,
        _ => anyhow::bail!("action does not belong to agent management"),
    })
}

fn administrative_access(action: GatewayAction) -> AdministrativeAccess {
    match action {
        GatewayAction::AdminRead
        | GatewayAction::AgentsRead
        | GatewayAction::AgentDefinitionsRead
        | GatewayAction::AgentDefinitionsReadContent
        | GatewayAction::ArtifactRead => AdministrativeAccess::Read,
        _ => AdministrativeAccess::Write,
    }
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
            access: administrative_access(record.action),
        },
        if record.decision.effect == PolicyEffect::Allow {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        },
        veoveo_mcp_gateway::audit::policy_reason(record.decision.reason),
    )
    .actor(subject.audit_actor()?)
    .authority(subject.audit_authority(&profile.id))
    .latency_ms(u64::try_from(record.started_at.elapsed().as_millis())?)
    .build()?;
    gateway.record_audit(draft).await
}

pub(super) async fn record_auth_audit(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    subject: Option<&AuthenticatedSubject>,
    started_at: Instant,
) -> anyhow::Result<()> {
    if outcome == AuthOutcome::Allow {
        return Ok(());
    }
    record_resource_auth_audit(
        &state.gateway_state,
        AuthAuditTarget::from(profile),
        outcome,
        reason,
        subject,
        started_at,
    )
    .await
}

pub(super) async fn record_resource_auth_audit(
    gateway_state: &GatewayState,
    target: AuthAuditTarget<'_>,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    subject: Option<&AuthenticatedSubject>,
    started_at: Instant,
) -> anyhow::Result<()> {
    // Successful bearer verification belongs to the action/window record.
    if outcome == AuthOutcome::Allow {
        return Ok(());
    }
    let request = subject
        .map(|subject| subject.audit.clone())
        .unwrap_or_else(audit_request);
    let mut draft = AuditDraft::builder(
        request,
        target.audit_target()?,
        AuditDetail::Authentication {
            activity: AuthenticationActivity::CredentialDenial,
            method: AuthMethod::BearerJwt,
            reason,
        },
        AuditOutcome::Denied,
        authentication_reason(outcome, reason),
    )
    .latency_ms(u64::try_from(started_at.elapsed().as_millis())?);
    if let Some(subject) = subject {
        draft = draft.actor(subject.audit_actor()?);
        if let Some(profile) = target.profile {
            draft = draft.authority(subject.audit_authority(profile));
        }
    }
    gateway_state.record_audit(draft.build()?).await
}

pub(super) struct AuthAuditRecord<'a> {
    pub(super) authorization_server: Option<&'a ResourceAuthorizationServer>,
    pub(super) client_id: Option<&'a OAuthClientId>,
    pub(super) principal: Option<&'a Principal>,
    pub(super) outcome: AuthOutcome,
    pub(super) reason: AuthReasonCode,
    pub(super) started_at: Instant,
}

#[derive(Clone, Copy)]
pub(super) struct AuthAuditTarget<'a> {
    pub(super) profile: Option<&'a GatewayProfileId>,
    pub(super) protected_resource: &'a ProtectedResourceId,
}

impl AuthAuditTarget<'_> {
    fn audit_target(self) -> anyhow::Result<AuditTarget> {
        Ok(match self.profile {
            Some(profile) => AuditTarget::Profile {
                profile: profile.clone(),
            },
            None => AuditTarget::PlatformResource {
                uri: veoveo_types::ResourceUri::new(self.protected_resource.to_string())?,
            },
        })
    }
}

impl<'a> From<&'a GatewayProfile> for AuthAuditTarget<'a> {
    fn from(profile: &'a GatewayProfile) -> Self {
        Self {
            profile: Some(&profile.id),
            protected_resource: &profile.protected_resource,
        }
    }
}

pub(super) async fn record_token_auth_audit<'a>(
    gateway_state: &GatewayState,
    target: impl Into<AuthAuditTarget<'a>>,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    let target = target.into();
    let draft = authentication_draft(target, record, AuthMethod::ClientCredentialsPrivateKeyJwt)?;
    gateway_state.record_audit(draft).await
}

pub(super) async fn record_id_jag_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    gateway_state
        .record_audit(authentication_draft(
            profile.into(),
            record,
            AuthMethod::EnterpriseManagedIdJag,
        )?)
        .await
}
pub(super) async fn record_oidc_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    gateway_state
        .record_audit(authentication_draft(
            profile.into(),
            record,
            AuthMethod::OidcAuthorizationCodePkce,
        )?)
        .await
}
pub(super) async fn record_refresh_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<()> {
    gateway_state
        .record_audit(refresh_auth_audit_event(profile, record)?)
        .await
}
pub(super) fn refresh_auth_audit_event(
    profile: &GatewayProfile,
    record: AuthAuditRecord<'_>,
) -> anyhow::Result<AuditDraft> {
    authentication_draft(profile.into(), record, AuthMethod::RefreshToken)
}
fn audit_request() -> AuditRequest {
    veoveo_mcp_gateway::request_observation::RequestObservation::current()
        .map(|request| request.audit)
        .unwrap_or_else(AuditRequest::background)
}
fn authentication_reason(outcome: AuthOutcome, reason: AuthReasonCode) -> AuditReason {
    if outcome == AuthOutcome::Allow && reason == AuthReasonCode::RefreshTokenRevoked {
        return AuditReason::Accepted;
    }
    match reason {
        AuthReasonCode::AuthAllow | AuthReasonCode::RefreshTokenDuplicateDelivery => {
            AuditReason::Accepted
        }
        AuthReasonCode::RefreshTokenReplay
        | AuthReasonCode::ClientAssertionReplay
        | AuthReasonCode::IdentityAssertionReplay => AuditReason::Replay,
        AuthReasonCode::RefreshTokenRevoked | AuthReasonCode::TokenRevoked => AuditReason::Revoked,
        AuthReasonCode::MissingAuthorizationHeader => AuditReason::Unauthenticated,
        AuthReasonCode::PolicyDenied | AuthReasonCode::InvalidScope => AuditReason::PolicyDenied,
        AuthReasonCode::IdentityProviderUnavailable
        | AuthReasonCode::AuthorizationServerUnavailable
        | AuthReasonCode::TokenSigningKeyUnavailable
        | AuthReasonCode::AuthStateUnavailable => AuditReason::Unavailable,
        _ => AuditReason::InvalidCredential,
    }
}
fn authentication_draft(
    target: AuthAuditTarget<'_>,
    record: AuthAuditRecord<'_>,
    method: AuthMethod,
) -> anyhow::Result<AuditDraft> {
    let allowed = record.outcome == AuthOutcome::Allow;
    let activity = match record.reason {
        AuthReasonCode::RefreshTokenDuplicateDelivery => AuthenticationActivity::DuplicateRefresh,
        AuthReasonCode::RefreshTokenReplay
        | AuthReasonCode::ClientAssertionReplay
        | AuthReasonCode::IdentityAssertionReplay => AuthenticationActivity::Replay,
        AuthReasonCode::RefreshTokenRevoked if allowed => AuthenticationActivity::Revoke,
        _ if !allowed => AuthenticationActivity::CredentialDenial,
        _ if method == AuthMethod::RefreshToken => AuthenticationActivity::Refresh,
        _ => AuthenticationActivity::Issue,
    };
    let audit_target = target
        .profile
        .cloned()
        .map(|profile| AuditTarget::Profile { profile })
        .or_else(|| {
            record
                .client_id
                .cloned()
                .map(|client| AuditTarget::Client { client })
        })
        .map_or_else(|| target.audit_target(), Ok)?;
    let mut draft = AuditDraft::builder(
        audit_request(),
        audit_target,
        AuditDetail::Authentication {
            activity,
            method,
            reason: record.reason,
        },
        if allowed {
            AuditOutcome::Succeeded
        } else {
            AuditOutcome::Denied
        },
        authentication_reason(record.outcome, record.reason),
    )
    .authority(AuditAuthority {
        profile: target.profile.cloned(),
        scopes: record
            .principal
            .map(|principal| principal.scopes.clone())
            .unwrap_or_default(),
        data_labels: record
            .principal
            .map(|principal| principal.data_labels.clone())
            .unwrap_or_default(),
        ..Default::default()
    })
    .latency_ms(u64::try_from(record.started_at.elapsed().as_millis())?);
    if let Some(principal) = record.principal {
        draft = draft.actor(AuditActor {
            principal: principal.id.clone(),
            kind: match principal.kind {
                veoveo_mcp_contract::PrincipalKind::User => AuditPrincipalKind::User,
                veoveo_mcp_contract::PrincipalKind::Service => AuditPrincipalKind::Service,
            },
            tenant: principal.tenant.clone(),
            oauth_client: record.client_id.cloned(),
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        });
    } else if allowed
        && let (Some(server), Some(client)) = (record.authorization_server, record.client_id)
    {
        draft = draft.actor(AuditActor {
            principal: PrincipalId::new(format!("{}#{}", server.issuer, client))?,
            kind: AuditPrincipalKind::Service,
            tenant: None,
            oauth_client: Some(client.clone()),
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        });
    }
    Ok(draft.build()?)
}

pub(super) fn auth_audit_error_response(err: anyhow::Error) -> Response {
    tracing::error!("failed to record gateway auth audit event: {err:#}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub(super) fn internal_error_response(err: impl std::fmt::Display) -> Response {
    tracing::error!("gateway internal error: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub(super) fn unauthorized(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    reason: &'static str,
) -> Response {
    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource/mcp/{}",
        state.public_base_url, profile.id
    );
    let challenge = www_authenticate_challenge(&metadata_url, &profile.required_scopes);
    let Ok(challenge) = HeaderValue::from_str(&challenge) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, challenge);
    tracing::debug!(profile = %profile.id, reason, "gateway authorization challenge");
    (
        StatusCode::UNAUTHORIZED,
        headers,
        "authorization required for gateway profile",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_records_distinguish_revocation_from_revoked_credential_denial() {
        let principal: Principal = serde_json::from_value(serde_json::json!({
            "id": "verified-user", "kind": "user", "issuer": "https://idp.example",
            "subject": "user", "tenant": "tenant-a", "scopes": ["operator:use"],
            "data_labels": ["cui"]
        }))
        .unwrap();
        let resource = ProtectedResourceId::new("https://gateway.example/mcp/operator").unwrap();
        let profile = GatewayProfileId::new("operator").unwrap();
        for (outcome, expected_outcome, expected_reason, activity) in [
            (
                AuthOutcome::Allow,
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
                AuthenticationActivity::Revoke,
            ),
            (
                AuthOutcome::Deny,
                AuditOutcome::Denied,
                AuditReason::Revoked,
                AuthenticationActivity::CredentialDenial,
            ),
        ] {
            let draft = authentication_draft(
                AuthAuditTarget {
                    profile: Some(&profile),
                    protected_resource: &resource,
                },
                AuthAuditRecord {
                    authorization_server: None,
                    client_id: None,
                    principal: Some(&principal),
                    outcome,
                    reason: AuthReasonCode::RefreshTokenRevoked,
                    started_at: Instant::now(),
                },
                AuthMethod::RefreshToken,
            )
            .unwrap();
            assert_eq!(draft.outcome(), expected_outcome);
            assert_eq!(draft.reason(), expected_reason);
            assert_eq!(
                draft.detail(),
                &AuditDetail::Authentication {
                    activity,
                    method: AuthMethod::RefreshToken,
                    reason: AuthReasonCode::RefreshTokenRevoked,
                }
            );
            assert_eq!(draft.authority().scopes, principal.scopes);
            assert_eq!(draft.authority().data_labels, principal.data_labels);
        }
    }

    #[test]
    fn anonymous_token_denial_identifies_its_protected_resource() {
        let resource =
            ProtectedResourceId::new("https://gateway.example/recording-ingest").unwrap();
        let draft = authentication_draft(
            AuthAuditTarget {
                profile: None,
                protected_resource: &resource,
            },
            AuthAuditRecord {
                authorization_server: None,
                client_id: None,
                principal: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::MissingAuthorizationHeader,
                started_at: Instant::now(),
            },
            AuthMethod::ClientCredentialsPrivateKeyJwt,
        )
        .unwrap();
        assert!(draft.actor().is_none());
        assert_eq!(draft.outcome(), AuditOutcome::Denied);
        assert_eq!(
            draft.target(),
            &AuditTarget::PlatformResource {
                uri: veoveo_types::ResourceUri::new(resource.to_string()).unwrap(),
            }
        );
    }
}
