#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use std::{collections::BTreeSet, num::NonZeroU32, time::Duration};
use veoveo_audit_contract::*;
use veoveo_gateway_contract::AuthorizationServerId;

use chrono::{TimeDelta, Utc};
use futures::future::join_all;
use veoveo_mcp_contract::{
    AuthMethod, AuthReasonCode, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
    GatewayJwtRevocation, GatewayProfileId, GatewayResourceSubscription, JwtId,
    OAuthAuthorizationCode, OAuthClientId, OAuthRedirectUri, OAuthStateValue,
    OidcClientRegistrationId, OidcNonce, PkceCodeChallenge, PkceCodeChallengeMethod,
    PkceCodeVerifier, Principal, PrincipalDisplayName, PrincipalKind, ServerSlug, TokenIssuer,
    TokenSubject,
};
use veoveo_mcp_gateway::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, GatewayState, RefreshTokenDeliveryCipher,
};
use veoveo_platform_store::GatewayRefreshTokenRecord;
use veoveo_types::{PrincipalId, ResourceUri, ScopeName, TenantId, WorkContextId};

#[tokio::test]
async fn concurrent_gateway_audit_writes_retry_transaction_conflicts() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let state = GatewayState::new(db.a.clone());
        let profile = GatewayProfileId::parse("admin").unwrap();
        let now = Utc::now();
        let principal = authorization_code(
            now,
            &profile,
            &OAuthClientId::parse("admin-console").unwrap(),
        )
        .principal;

        let results = join_all((0..12).map(|index| {
            let state = state.clone();
            let event = policy_draft(
                &format!("concurrent-policy-{index}"),
                now,
                &profile,
                &principal,
            );
            async move { state.record_audit(event).await }
        }))
        .await;
        for result in results {
            result.unwrap();
        }

        assert_eq!(
            audit_count(&state, &principal, AuditClass::ApiActivity).await,
            12
        );
    })
    .await
    .expect("concurrent_gateway_audit_writes_retry_transaction_conflicts exceeded three minutes");
}

#[tokio::test]
async fn gateway_correctness_state_is_shared_and_single_use_across_replicas() {
    tokio::time::timeout(Duration::from_secs(180), async {
    let db = fixture::TestDb::new().await;
    let first = GatewayState::new(db.a.clone());
    let second = GatewayState::new(db.b.clone());
    let now = Utc::now();
    let authorization_server = AuthorizationServerId::parse("veoveo").unwrap();
    let client_id = OAuthClientId::parse("operator-console").unwrap();

    for (jwt_id, register_id_jag) in [("assertion-jti", false), ("id-jag-jti", true)] {
        let jwt_id = JwtId::parse(jwt_id).unwrap();
        let expires_at = now + TimeDelta::minutes(5);
        let (left, right) = if register_id_jag {
            tokio::join!(
                first.record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now,
                ),
                second.record_id_jag_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now,
                ),
            )
        } else {
            tokio::join!(
                first.record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now,
                ),
                second.record_client_assertion_jti(
                    &authorization_server,
                    &client_id,
                    &jwt_id,
                    expires_at,
                    now,
                ),
            )
        };
        assert_eq!(
            usize::from(left.unwrap()) + usize::from(right.unwrap()),
            1,
            "exactly one replica must claim a replay identifier",
        );
    }

    let profile = GatewayProfileId::parse("operator").unwrap();
    let issuer = TokenIssuer::parse("https://idp.example.com").unwrap();
    let revoked_jwt = JwtId::parse("revoked-jwt").unwrap();
    let revocation = GatewayJwtRevocation {
        profile: profile.clone(),
        issuer: issuer.clone(),
        jwt_id: revoked_jwt.clone(),
        revoked_at: now,
        expires_at: now + TimeDelta::hours(1),
        reason: Some("integration-test".to_owned()),
    };
    first.record_jwt_revocation(&revocation).await.unwrap();
    assert_eq!(
        second
            .jwt_revocation(&profile, &issuer, &revoked_jwt, now)
            .await
            .unwrap(),
        Some(revocation),
    );

    let subscription = GatewayResourceSubscription {
        profile: profile.clone(),
        owner: PrincipalId::parse("https://idp.example.com#alice").unwrap(),
        upstream_server: ServerSlug::parse("artifact").unwrap(),
        resource_uri: ResourceUri::new("artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471").unwrap(),
        created_at: now,
        updated_at: now,
    };
    first
        .record_resource_subscription(&subscription)
        .await
        .unwrap();
    assert_eq!(
        second
            .resource_subscription(
                &subscription.profile,
                &subscription.owner,
                &subscription.upstream_server,
                &subscription.resource_uri,
            )
            .await
            .unwrap(),
        Some(subscription.clone()),
    );
    second
        .delete_resource_subscription(
            &subscription.profile,
            &subscription.owner,
            &subscription.upstream_server,
            &subscription.resource_uri,
        )
        .await
        .unwrap();
    assert!(
        first
            .resource_subscription(
                &subscription.profile,
                &subscription.owner,
                &subscription.upstream_server,
                &subscription.resource_uri,
            )
            .await
            .unwrap()
            .is_none(),
    );

    let request = authorization_request(now, &profile, &client_id);
    first.record_authorization_request(&request).await.unwrap();
    let (left, right) = tokio::join!(
        first.consume_authorization_request(&request.idp_state, now),
        second.consume_authorization_request(&request.idp_state, now),
    );
    assert_single_consumption(left.unwrap(), right.unwrap(), &request);

    let code = authorization_code(now, &profile, &client_id);
    first.record_authorization_code(&code).await.unwrap();
    let (left, right) = tokio::join!(
        first.consume_authorization_code(&code.code, now),
        second.consume_authorization_code(&code.code, now),
    );
    let consumed = [left.unwrap(), right.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(consumed.len(), 1, "authorization code must consume once");
    assert_eq!(consumed[0].consumed_at, Some(now));

    let principal = code.principal.clone();
    let issued_refresh = first
        .issue_refresh_token(GatewayRefreshIssueRequest {
            authorization_server: &authorization_server,
            profile: &profile,
            oauth_client_id: &client_id,
            work_context: &WorkContextId::parse("mission").unwrap(),
            principal: &principal,
            principal_display_name: &PrincipalDisplayName::new("Alice").unwrap(),
            scopes: &principal.scopes,
            now,
        })
        .await
        .unwrap();
    let presented_refresh = issued_refresh.token.clone();
    let delivery_cipher = test_refresh_delivery_cipher();
    let left_refresh_audit = auth_draft("refresh-left", now, &profile, &principal);
    let right_refresh_audit = auth_draft("refresh-right", now, &profile, &principal);
    let left_duplicate_audit =
        redelivery_draft("refresh-left-duplicate", now, &profile, &principal);
    let right_duplicate_audit =
        redelivery_draft("refresh-right-duplicate", now, &profile, &principal);
    let (left, right) = tokio::join!(
        first.rotate_refresh_token(
            &presented_refresh,
            refresh_rotation_request(
                &authorization_server,
                &profile,
                &client_id,
                now + TimeDelta::seconds(1),
                &delivery_cipher,
                &left_refresh_audit,
                &left_duplicate_audit,
            ),
        ),
        second.rotate_refresh_token(
            &presented_refresh,
            refresh_rotation_request(
                &authorization_server,
                &profile,
                &client_id,
                now + TimeDelta::seconds(1),
                &delivery_cipher,
                &right_refresh_audit,
                &right_duplicate_audit,
            ),
        ),
    );
    let mut rotated = None;
    let mut duplicate_delivery = None;
    for outcome in [left.unwrap(), right.unwrap()] {
        match outcome {
            GatewayRefreshExchange::Rotated(successor) => rotated = Some(successor),
            GatewayRefreshExchange::DuplicateDelivery(successor) => {
                duplicate_delivery = Some(successor);
            }
            GatewayRefreshExchange::ReplayDetected { .. } => {
                panic!("concurrent refresh exchange was treated as delayed replay")
            }
            GatewayRefreshExchange::Invalid => panic!("concurrent refresh exchange was invalid"),
        }
    }
    let rotated = rotated.expect("one replica must rotate the refresh token");
    let duplicate_delivery =
        duplicate_delivery.expect("one replica must receive the committed successor");
    assert_eq!(rotated.grant.generation, 1);
    assert_eq!(
        rotated.token.as_str(),
        duplicate_delivery.token.as_str(),
        "both replicas must deliver the identical successor refresh token",
    );
    assert_eq!(rotated.grant.family_id, duplicate_delivery.grant.family_id);

    let delayed_replay_audit = auth_draft("refresh-delayed-replay", now, &profile, &principal);
    let delayed_duplicate_audit = redelivery_draft(
        "refresh-delayed-replay-duplicate",
        now,
        &profile,
        &principal,
    );
    assert!(matches!(
        first
            .rotate_refresh_token(
                &presented_refresh,
                refresh_rotation_request(
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(6),
                    &delivery_cipher,
                    &delayed_replay_audit,
                    &delayed_duplicate_audit,
                ),
            )
            .await
            .unwrap(),
        GatewayRefreshExchange::ReplayDetected { .. }
    ));
    let revoked_successor_audit =
        auth_draft("refresh-revoked-successor", now, &profile, &principal);
    let revoked_successor_duplicate_audit = redelivery_draft(
        "refresh-revoked-successor-duplicate",
        now,
        &profile,
        &principal,
    );
    assert!(matches!(
        first
            .rotate_refresh_token(
                &rotated.token,
                refresh_rotation_request(
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(7),
                    &delivery_cipher,
                    &revoked_successor_audit,
                    &revoked_successor_duplicate_audit,
                ),
            )
            .await
            .unwrap(),
        GatewayRefreshExchange::Invalid
    ));
    let mut response = first
        .platform_store()
        .client()
        .query(include_str!("queries/gateway_state/gateway_correctness_state_is_shared_and_single_use_across_replicas/statement_1.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    let stored_tokens: Vec<GatewayRefreshTokenRecord> = response.take(0).unwrap();
    assert_eq!(stored_tokens.len(), 2);
    assert!(
        stored_tokens
            .iter()
            .all(|token| token.token_hash.len() == 64)
    );
    assert!(stored_tokens.iter().all(|token| {
        token.token_hash != presented_refresh.as_str() && token.token_hash != rotated.token.as_str()
    }));

    let old_policy = policy_draft("policy-old", now - TimeDelta::days(2), &profile, &principal);
    first.record_audit(old_policy).await.unwrap();
    let auth = auth_draft("auth-current", now, &profile, &principal);
    second.record_audit(auth).await.unwrap();
    assert_eq!(
        audit_count(&second, &principal, AuditClass::ApiActivity).await,
        1
    );
    assert_eq!(
        audit_count(&first, &principal, AuditClass::Authentication).await,
        3
    );
    let delivery_retention = second
        .prune_expired_refresh_tokens(now + TimeDelta::seconds(7))
        .await
        .unwrap();
    assert_eq!(delivery_retention.delivery_envelopes_deleted, 1);
    assert_eq!(delivery_retention.tokens_deleted, 0);
    assert_eq!(delivery_retention.families_deleted, 0);
    let mut response = first
        .platform_store()
        .client()
        .query(include_str!("queries/gateway_state/gateway_correctness_state_is_shared_and_single_use_across_replicas/statement_2.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    let retained_tokens: Vec<GatewayRefreshTokenRecord> = response.take(0).unwrap();
    assert!(
        retained_tokens.iter().all(|token| {
            token.delivery_envelope.is_none() && token.delivery_expires_at.is_none()
        })
    );

    let refresh_retention = second
        .prune_expired_refresh_tokens(now + TimeDelta::days(8))
        .await
        .unwrap();
    assert_eq!(refresh_retention.tokens_deleted, 2);
    assert_eq!(refresh_retention.families_deleted, 1);
    }).await.expect("gateway_correctness_state_is_shared_and_single_use_across_replicas exceeded three minutes");
}

#[tokio::test]
async fn refresh_rotation_rolls_back_when_success_audit_cannot_commit() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let state = GatewayState::new(db.a.clone());
        let now = Utc::now();
        let delivery_cipher = test_refresh_delivery_cipher();
        let authorization_server = AuthorizationServerId::parse("veoveo").unwrap();
        let profile = GatewayProfileId::parse("operator").unwrap();
        let client_id = OAuthClientId::parse("operator-console").unwrap();
        let principal = authorization_code(now, &profile, &client_id).principal;
        let issued = state
            .issue_refresh_token(GatewayRefreshIssueRequest {
                authorization_server: &authorization_server,
                profile: &profile,
                oauth_client_id: &client_id,
                work_context: &WorkContextId::parse("mission").unwrap(),
                principal: &principal,
                principal_display_name: &PrincipalDisplayName::new("Alice").unwrap(),
                scopes: &principal.scopes,
                now,
            })
            .await
            .unwrap();
        let duplicate_audit = auth_draft("duplicate-refresh-audit", now, &profile, &principal);
        let duplicate_delivery_audit = redelivery_draft(
            "duplicate-refresh-delivery-audit",
            now,
            &profile,
            &principal,
        );
        state
            .record_audit(conflicting_draft(&duplicate_audit))
            .await
            .unwrap();

        state
            .rotate_refresh_token(
                &issued.token,
                refresh_rotation_request(
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(1),
                    &delivery_cipher,
                    &duplicate_audit,
                    &duplicate_delivery_audit,
                ),
            )
            .await
            .expect_err("conflicting audit identity must roll back the refresh rotation");
        let preserved = state
            .refresh_token_grant(
                &issued.token,
                &authorization_server,
                &profile,
                &client_id,
                now + TimeDelta::seconds(2),
            )
            .await
            .unwrap()
            .expect("failed delivery must leave the presented refresh token usable");
        assert_eq!(preserved.generation, 0);

        let retry_audit = auth_draft("refresh-delivery-retry", now, &profile, &principal);
        let retry_duplicate_audit = redelivery_draft(
            "refresh-delivery-retry-duplicate",
            now,
            &profile,
            &principal,
        );
        let retry = state
            .rotate_refresh_token(
                &issued.token,
                refresh_rotation_request(
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(2),
                    &delivery_cipher,
                    &retry_audit,
                    &retry_duplicate_audit,
                ),
            )
            .await
            .unwrap();
        assert!(matches!(retry, GatewayRefreshExchange::Rotated(_)));
    })
    .await
    .expect("refresh_rotation_rolls_back_when_success_audit_cannot_commit exceeded three minutes");
}

#[tokio::test]
async fn consuming_a_successor_clears_its_delivery_envelope_atomically() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let state = GatewayState::new(db.a.clone());
        let now = Utc::now();
        let delivery_cipher = test_refresh_delivery_cipher();
        let authorization_server = AuthorizationServerId::parse("veoveo").unwrap();
        let profile = GatewayProfileId::parse("operator").unwrap();
        let client_id = OAuthClientId::parse("operator-console").unwrap();
        let principal = authorization_code(now, &profile, &client_id).principal;
        let issued = state
            .issue_refresh_token(GatewayRefreshIssueRequest {
                authorization_server: &authorization_server,
                profile: &profile,
                oauth_client_id: &client_id,
                work_context: &WorkContextId::parse("mission").unwrap(),
                principal: &principal,
                principal_display_name: &PrincipalDisplayName::new("Alice").unwrap(),
                scopes: &principal.scopes,
                now,
            })
            .await
            .unwrap();
        let first_audit = auth_draft("eager-clear-first", now, &profile, &principal);
        let first_duplicate_audit =
            redelivery_draft("eager-clear-first-duplicate", now, &profile, &principal);
        let successor = match state
            .rotate_refresh_token(
                &issued.token,
                refresh_rotation_request(
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(1),
                    &delivery_cipher,
                    &first_audit,
                    &first_duplicate_audit,
                ),
            )
            .await
            .unwrap()
        {
            GatewayRefreshExchange::Rotated(successor) => successor,
            outcome => panic!("first rotation returned {outcome:?}"),
        };

        let blocked_audit = auth_draft("eager-clear-blocked", now, &profile, &principal);
        state
            .record_audit(conflicting_draft(&blocked_audit))
            .await
            .unwrap();
        let blocked_duplicate_audit =
            redelivery_draft("eager-clear-blocked-duplicate", now, &profile, &principal);
        state
            .rotate_refresh_token(
                &successor.token,
                refresh_rotation_request(
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(2),
                    &delivery_cipher,
                    &blocked_audit,
                    &blocked_duplicate_audit,
                ),
            )
            .await
            .expect_err("failed successor consumption must roll back envelope clearing");
        let generation_one = stored_refresh_generation(&state, 1).await;
        assert!(generation_one.consumed_at.is_none());
        assert!(generation_one.delivery_envelope.is_some());
        assert!(generation_one.delivery_expires_at.is_some());

        let consume_audit = auth_draft("eager-clear-consume", now, &profile, &principal);
        let consume_duplicate_audit =
            redelivery_draft("eager-clear-consume-duplicate", now, &profile, &principal);
        assert!(matches!(
            state
                .rotate_refresh_token(
                    &successor.token,
                    refresh_rotation_request(
                        &authorization_server,
                        &profile,
                        &client_id,
                        now + TimeDelta::seconds(3),
                        &delivery_cipher,
                        &consume_audit,
                        &consume_duplicate_audit,
                    ),
                )
                .await
                .unwrap(),
            GatewayRefreshExchange::Rotated(_)
        ));
        let generation_one = stored_refresh_generation(&state, 1).await;
        assert!(generation_one.consumed_at.is_some());
        assert!(generation_one.delivery_envelope.is_none());
        assert!(generation_one.delivery_expires_at.is_none());
        let generation_two = stored_refresh_generation(&state, 2).await;
        assert!(generation_two.delivery_envelope.is_some());
    })
    .await
    .expect("consuming_a_successor_clears_its_delivery_envelope_atomically exceeded three minutes");
}

#[tokio::test]
async fn public_client_revocation_is_bound_idempotent_and_family_wide() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let state = GatewayState::new(db.a.clone());
        let now = Utc::now();
        let delivery_cipher = test_refresh_delivery_cipher();
        let authorization_server = AuthorizationServerId::parse("veoveo").unwrap();
        let profile = GatewayProfileId::parse("operator").unwrap();
        let client_id = OAuthClientId::parse("operator-console").unwrap();
        let principal = authorization_code(now, &profile, &client_id).principal;
        let issued = state
            .issue_refresh_token(GatewayRefreshIssueRequest {
                authorization_server: &authorization_server,
                profile: &profile,
                oauth_client_id: &client_id,
                work_context: &WorkContextId::parse("mission").unwrap(),
                principal: &principal,
                principal_display_name: &PrincipalDisplayName::new("Alice").unwrap(),
                scopes: &principal.scopes,
                now,
            })
            .await
            .unwrap();

        assert!(
            state
                .revoke_refresh_token_family(
                    &issued.token,
                    &authorization_server,
                    &profile,
                    &OAuthClientId::parse("different-client").unwrap(),
                    now + TimeDelta::seconds(1),
                )
                .await
                .unwrap()
                .is_none(),
            "a different public client must not revoke the family",
        );
        let revoked = state
            .revoke_refresh_token_family(
                &issued.token,
                &authorization_server,
                &profile,
                &client_id,
                now + TimeDelta::seconds(2),
            )
            .await
            .unwrap()
            .expect("owning public client revokes the refresh family");
        assert_eq!(revoked.family_id, issued.grant.family_id);
        assert!(
            state
                .revoke_refresh_token_family(
                    &issued.token,
                    &authorization_server,
                    &profile,
                    &client_id,
                    now + TimeDelta::seconds(3),
                )
                .await
                .unwrap()
                .is_some(),
            "repeated revocation is idempotently successful",
        );
        let rejected_audit = auth_draft("revoked-family-rotate", now, &profile, &principal);
        let rejected_duplicate_audit =
            redelivery_draft("revoked-family-rotate-duplicate", now, &profile, &principal);
        assert!(matches!(
            state
                .rotate_refresh_token(
                    &issued.token,
                    refresh_rotation_request(
                        &authorization_server,
                        &profile,
                        &client_id,
                        now + TimeDelta::seconds(4),
                        &delivery_cipher,
                        &rejected_audit,
                        &rejected_duplicate_audit,
                    ),
                )
                .await
                .unwrap(),
            GatewayRefreshExchange::Invalid
        ));
    })
    .await
    .expect("public_client_revocation_is_bound_idempotent_and_family_wide exceeded three minutes");
}

fn authorization_request(
    now: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    client_id: &OAuthClientId,
) -> GatewayAuthorizationRequest {
    GatewayAuthorizationRequest {
        idp_state: OAuthStateValue::parse("integration-idp-state").unwrap(),
        profile: profile.clone(),
        oauth_client_id: client_id.clone(),
        work_context: WorkContextId::parse("mission").unwrap(),
        oidc_client: OidcClientRegistrationId::parse("enterprise").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
        client_state: Some(OAuthStateValue::parse("client-state").unwrap()),
        requested_scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
        code_challenge: PkceCodeChallenge::parse("A".repeat(43)).unwrap(),
        code_challenge_method: PkceCodeChallengeMethod::S256,
        idp_code_verifier: PkceCodeVerifier::parse("B".repeat(43)).unwrap(),
        idp_code_challenge: PkceCodeChallenge::parse("C".repeat(43)).unwrap(),
        idp_code_challenge_method: PkceCodeChallengeMethod::S256,
        nonce: OidcNonce::parse("integration-nonce").unwrap(),
        created_at: now,
        expires_at: now + TimeDelta::minutes(5),
    }
}

fn authorization_code(
    now: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    client_id: &OAuthClientId,
) -> GatewayAuthorizationCodeRecord {
    let principal = Principal {
        id: PrincipalId::parse("https://idp.example.com#alice").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::parse("https://idp.example.com").unwrap(),
        subject: TokenSubject::parse("alice").unwrap(),
        tenant: Some(TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(now),
    };
    GatewayAuthorizationCodeRecord {
        code: OAuthAuthorizationCode::parse("D".repeat(43)).unwrap(),
        profile: profile.clone(),
        oauth_client_id: client_id.clone(),
        work_context: WorkContextId::parse("mission").unwrap(),
        oidc_client: OidcClientRegistrationId::parse("enterprise").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
        client_state: None,
        scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
        code_challenge: PkceCodeChallenge::parse("E".repeat(43)).unwrap(),
        code_challenge_method: PkceCodeChallengeMethod::S256,
        principal,
        principal_display_name: PrincipalDisplayName::new("Alice").unwrap(),
        issued_at: now,
        expires_at: now + TimeDelta::minutes(5),
        consumed_at: None,
    }
}

fn fixture_draft(
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
    detail: AuditDetail,
) -> AuditDraft {
    AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::Profile {
            profile: profile.clone(),
        },
        detail,
        AuditOutcome::Allowed,
        AuditReason::Accepted,
    )
    .actor(AuditActor {
        principal: principal.id.clone(),
        kind: AuditPrincipalKind::User,
        tenant: principal.tenant.clone(),
        oauth_client: None,
        session_family: None,
        delegating_principal: None,
        managed_agent: None,
    })
    .authority(AuditAuthority {
        profile: Some(profile.clone()),
        scopes: principal.scopes.clone(),
        data_labels: principal.data_labels.clone(),
        ..Default::default()
    })
    .occurred_at(timestamp)
    .latency_ms(1)
    .build()
    .unwrap()
}
fn policy_draft(
    _label: &str,
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
) -> AuditDraft {
    fixture_draft(
        timestamp,
        profile,
        principal,
        AuditDetail::Read {
            method: AuditReadMethod::Status,
        },
    )
}
fn auth_draft(
    _label: &str,
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
) -> AuditDraft {
    fixture_draft(
        timestamp,
        profile,
        principal,
        AuditDetail::Authentication {
            activity: AuthenticationActivity::Refresh,
            method: AuthMethod::RefreshToken,
            reason: AuthReasonCode::AuthAllow,
        },
    )
}
fn redelivery_draft(
    _label: &str,
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
) -> AuditDraft {
    fixture_draft(
        timestamp,
        profile,
        principal,
        AuditDetail::Authentication {
            activity: AuthenticationActivity::DuplicateRefresh,
            method: AuthMethod::RefreshToken,
            reason: AuthReasonCode::RefreshTokenDuplicateDelivery,
        },
    )
}
fn conflicting_draft(draft: &AuditDraft) -> AuditDraft {
    AuditDraft::builder(
        draft.request().clone(),
        draft.target().clone(),
        draft.detail().clone(),
        draft.outcome(),
        draft.reason(),
    )
    .identity(draft.id())
    .actor(draft.actor().unwrap().clone())
    .authority(draft.authority().clone())
    .occurred_at(draft.occurred_at())
    .latency_ms(99)
    .build()
    .unwrap()
}
async fn audit_count(state: &GatewayState, principal: &Principal, class: AuditClass) -> usize {
    let scope = AuditReadScope::new(principal.tenant.clone(), principal.tenant.is_none());
    let mut query = AuditQuery::new(scope.partitions().into_iter().next().unwrap());
    query.class = Some(class);
    query.limit = 1000;
    let page = state
        .platform_store()
        .audit_page(&scope, &query)
        .await
        .unwrap();
    assert!(page.next.is_none(), "fixture exceeds its audit page");
    page.records.len()
}

fn test_refresh_delivery_cipher() -> RefreshTokenDeliveryCipher {
    RefreshTokenDeliveryCipher::new(b"0123456789abcdef0123456789abcdef").unwrap()
}

fn refresh_rotation_request<'a>(
    authorization_server: &'a AuthorizationServerId,
    profile: &'a GatewayProfileId,
    oauth_client_id: &'a OAuthClientId,
    now: chrono::DateTime<Utc>,
    delivery_cipher: &'a RefreshTokenDeliveryCipher,
    success_audit: &'a AuditDraft,
    duplicate_delivery_audit: &'a AuditDraft,
) -> GatewayRefreshRotationRequest<'a> {
    GatewayRefreshRotationRequest {
        authorization_server,
        profile,
        oauth_client_id,
        now,
        delivery_window: GatewayRefreshDeliveryWindow::from_seconds(NonZeroU32::new(5).unwrap())
            .unwrap(),
        delivery_cipher,
        success_audit,
        duplicate_delivery_audit,
    }
}

async fn stored_refresh_generation(
    state: &GatewayState,
    generation: i64,
) -> GatewayRefreshTokenRecord {
    let mut response = state
        .platform_store()
        .client()
        .query(include_str!(
            "queries/gateway_state/stored_refresh_generation/statement_1.surql"
        ))
        .bind(("generation", generation))
        .await
        .unwrap()
        .check()
        .unwrap();
    let records: Vec<GatewayRefreshTokenRecord> = response.take(0).unwrap();
    assert_eq!(records.len(), 1);
    records.into_iter().next().unwrap()
}

fn assert_single_consumption<T: Clone + PartialEq + std::fmt::Debug>(
    left: Option<T>,
    right: Option<T>,
    expected: &T,
) {
    let consumed = [left, right].into_iter().flatten().collect::<Vec<_>>();
    assert_eq!(consumed, vec![expected.clone()]);
}
