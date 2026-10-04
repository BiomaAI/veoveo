use std::{sync::Arc, time::Instant};
use veoveo_gateway_contract::GatewayAction;
pub(super) use veoveo_mcp_contract::audit::{AdminOperationFailure, AdministrativeOperation};
use veoveo_mcp_contract::audit::{
    AuditActor, AuditAuthority, AuditDetail, AuditDraft, AuditOutcome, AuditPrincipalKind,
    AuditTarget, AuthenticationActivity,
};
pub(super) use veoveo_mcp_gateway::http::action_audit::{
    AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationStatus,
    authorize_gateway_action, record_gateway_operation_audit,
};
pub(super) use veoveo_mcp_gateway::http::auth_support::{
    AuthAuditTarget, auth_audit_error_response, internal_error_response,
};
use veoveo_mcp_gateway::http::auth_support::{audit_request, authentication_reason};

use axum::response::Response;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    AuthMethod, AuthOutcome, AuthReasonCode, GatewayControlPlane, GatewayProfile, GatewayProfileId,
    OAuthClientId, PolicyTarget, Principal, ResourceAuthorizationServer,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog, GatewayState};
use veoveo_types::PrincipalId;

use crate::runtime::{AdminState, current_catalog};

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
            action: action.into(),
            target: PolicyTarget::Gateway,
            operation,
            started_at,
        },
    )
    .await
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

pub(super) fn control_plane_sha256(control_plane: &GatewayControlPlane) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(control_plane)?;
    let digest = Sha256::digest(bytes);
    Ok(digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
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
    let catalog = current_catalog(&state.catalog);
    record_gateway_operation_audit(
        &state.gateway_state,
        profile,
        subject,
        &catalog,
        target,
        record,
    )
    .await
}

pub(super) struct AuthAuditRecord<'a> {
    pub(super) authorization_server: Option<&'a ResourceAuthorizationServer>,
    pub(super) client_id: Option<&'a OAuthClientId>,
    pub(super) principal: Option<&'a Principal>,
    pub(super) outcome: AuthOutcome,
    pub(super) reason: AuthReasonCode,
    pub(super) started_at: Instant,
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
            principal: PrincipalId::parse(format!("{}#{}", server.issuer, client))?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_audit_contract::AuditReason;
    use veoveo_gateway_contract::ProtectedResourceId;

    #[test]
    fn authentication_records_distinguish_revocation_from_revoked_credential_denial() {
        let principal: Principal = serde_json::from_value(serde_json::json!({
            "id": "verified-user", "kind": "user", "issuer": "https://idp.example",
            "subject": "user", "tenant": "tenant-a", "scopes": ["operator:use"],
            "data_labels": ["cui"]
        }))
        .unwrap();
        let resource = ProtectedResourceId::parse("https://gateway.example/mcp/operator").unwrap();
        let profile = GatewayProfileId::parse("operator").unwrap();
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
            ProtectedResourceId::parse("https://gateway.example/recording-ingest").unwrap();
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
