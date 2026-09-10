#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use chrono::{TimeDelta, Utc};
use std::{collections::BTreeSet, num::NonZeroU32};
use veoveo_mcp_contract::*;
use veoveo_mcp_gateway::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, GatewayState, RefreshTokenDeliveryCipher,
};

#[tokio::test]
async fn session_binding_survives_rotation_and_rejects_cross_replica_revocation() {
    let db = fixture::TestDb::new().await;
    let first = GatewayState::new(db.a.clone());
    let second = GatewayState::new(db.b.clone());
    let profile = GatewayProfileId::new("operator").unwrap();
    let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
    let client = OAuthClientId::new("console").unwrap();
    let context = WorkContextId::new("mission").unwrap();
    let principal = Principal {
        id: PrincipalId::new("https://identity.example#one").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://identity.example").unwrap(),
        subject: TokenSubject::new("one").unwrap(),
        tenant: Some(TenantId::new("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: None,
    };
    let display = PrincipalDisplayName::new("Same Display Name").unwrap();
    let cipher = RefreshTokenDeliveryCipher::new(b"0123456789abcdef0123456789abcdef").unwrap();
    // Each case has its own real family, using the same two independent clients.
    for ending in ["logout", "replay", "expiry", "missing"] {
        let now = Utc::now();
        let issued = first
            .issue_refresh_token(GatewayRefreshIssueRequest {
                authorization_server: &authorization_server,
                profile: &profile,
                oauth_client_id: &client,
                work_context: &context,
                principal: &principal,
                principal_display_name: &display,
                scopes: &principal.scopes,
                now,
            })
            .await
            .unwrap();
        let token = AccessTokenSubject {
            issuer: TokenIssuer::new("https://veoveo.example").unwrap(),
            subject: principal.subject.clone(),
            oauth_client_id: client.clone(),
            session_family: Some(issued.grant.family_id.clone()),
            audience: ProtectedResourceId::new("https://veoveo.example/mcp/operator").unwrap(),
            work_context: context.clone(),
            invocation_mode: InvocationMode::Direct,
            initiator: Some(principal.id.clone()),
            delegation_id: None,
            scopes: principal.scopes.clone(),
            jwt_id: Some(JwtId::new("access-one").unwrap()),
            issued_at: now,
            not_before: None,
            expires_at: now + TimeDelta::minutes(15),
        };
        // Match JwtVerifier: the signed gateway issuer becomes Principal.issuer,
        // while the refresh family still holds the upstream OIDC issuer.
        let mut authenticated = principal.clone();
        authenticated.issuer = token.issuer.clone();
        assert!(
            second
                .access_token_session_valid(&profile, &authorization_server, &token, &authenticated)
                .await
                .unwrap()
        );
        let mut foreign = authenticated.clone();
        foreign.subject = TokenSubject::new("two").unwrap();
        assert!(
            !second
                .access_token_session_valid(&profile, &authorization_server, &token, &foreign)
                .await
                .unwrap()
        );
        foreign = authenticated.clone();
        foreign.tenant = Some(TenantId::new("tenant-b").unwrap());
        assert!(
            !second
                .access_token_session_valid(&profile, &authorization_server, &token, &foreign)
                .await
                .unwrap()
        );
        assert!(
            !second
                .access_token_session_valid(
                    &GatewayProfileId::new("other").unwrap(),
                    &authorization_server,
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        assert!(
            !second
                .access_token_session_valid(
                    &profile,
                    &AuthorizationServerId::new("other").unwrap(),
                    &token,
                    &authenticated
                )
                .await
                .unwrap()
        );
        for mismatch in ["client", "context", "scope"] {
            let mut bad = token.clone();
            match mismatch {
                "client" => bad.oauth_client_id = OAuthClientId::new("other").unwrap(),
                "context" => bad.work_context = WorkContextId::new("other").unwrap(),
                _ => {
                    bad.scopes.insert(ScopeName::new("admin:use").unwrap());
                }
            }
            assert!(
                !second
                    .access_token_session_valid(
                        &profile,
                        &authorization_server,
                        &bad,
                        &authenticated
                    )
                    .await
                    .unwrap()
            );
        }
        let success = audit(&profile, &principal, AuthReasonCode::AuthAllow);
        let duplicate = audit(
            &profile,
            &principal,
            AuthReasonCode::RefreshTokenDuplicateDelivery,
        );
        let request = |at| GatewayRefreshRotationRequest {
            authorization_server: &authorization_server,
            profile: &profile,
            oauth_client_id: &client,
            now: at,
            delivery_window: GatewayRefreshDeliveryWindow::from_seconds(
                NonZeroU32::new(1).unwrap(),
            )
            .unwrap(),
            delivery_cipher: &cipher,
            success_audit: &success,
            duplicate_delivery_audit: &duplicate,
        };
        let rotated = match first
            .rotate_refresh_token(&issued.token, request(now))
            .await
            .unwrap()
        {
            GatewayRefreshExchange::Rotated(rotated) => rotated,
            _ => panic!("rotation must succeed"),
        };
        assert_eq!(rotated.grant.family_id, issued.grant.family_id);
        assert!(
            second
                .access_token_session_valid(&profile, &authorization_server, &token, &authenticated)
                .await
                .unwrap()
        );
        match ending {
            "logout" => {
                first
                    .revoke_refresh_token_family(
                        &rotated.token,
                        &authorization_server,
                        &profile,
                        &client,
                        now,
                    )
                    .await
                    .unwrap()
                    .unwrap();
            }
            "replay" => assert!(matches!(
                first
                    .rotate_refresh_token(&issued.token, request(now + TimeDelta::seconds(2)))
                    .await
                    .unwrap(),
                GatewayRefreshExchange::ReplayDetected { .. }
            )),
            _ => {
                let family = veoveo_platform_store::gateway_refresh_family_record_id(
                    issued.grant.family_id.as_str().parse().unwrap(),
                );
                let sql = if ending == "expiry" {
                    "UPDATE $family SET expires_at = time::now() - 1s;"
                } else {
                    "DELETE $family;"
                };
                db.a.client()
                    .query(sql)
                    .bind(("family", family))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
        }
        // A still-unexpired signed access token cannot outlive committed family revocation.
        assert!(
            !second
                .access_token_session_valid(&profile, &authorization_server, &token, &authenticated)
                .await
                .unwrap()
        );
    }
}

fn audit(
    profile: &GatewayProfileId,
    principal: &Principal,
    reason: AuthReasonCode,
) -> AuthAuditEvent {
    AuthAuditEvent {
        event_id: TraceId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        timestamp: Utc::now(),
        trace_id: TraceId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        profile: Some(profile.clone()),
        protected_resource: ProtectedResourceId::new("https://veoveo.example/mcp/operator")
            .unwrap(),
        outcome: AuthOutcome::Allow,
        reason,
        method: AuthMethod::RefreshToken,
        principal: Some(principal.id.clone()),
        principal_attributes: Some(PrincipalAuditAttributes::from(principal)),
        tenant: principal.tenant.clone(),
        token_issuer: Some(principal.issuer.clone()),
        token_subject: Some(principal.subject.clone()),
        jwt_id: None,
        latency_ms: None,
        metadata: Default::default(),
    }
}
