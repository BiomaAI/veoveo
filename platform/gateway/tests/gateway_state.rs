use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
};

use chrono::{TimeDelta, Utc};
use secrecy::SecretString;
use uuid::Uuid;
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, AuthMethod, AuthOutcome, AuthReasonCode, AuthorizationServerId,
    GatewayAction, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
    GatewayJwtRevocation, GatewayProfileId, GatewayResourceSubscription, JwtId, McpMethodName,
    OAuthAuthorizationCode, OAuthClientId, OAuthRedirectUri, OAuthStateValue,
    OidcClientRegistrationId, OidcNonce, PkceCodeChallenge, PkceCodeChallengeMethod,
    PkceCodeVerifier, PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyTarget, Principal,
    PrincipalAuditAttributes, PrincipalId, PrincipalKind, ProtectedResourceId, ResourceUri,
    ScopeName, ServerSlug, TenantId, TokenIssuer, TokenSubject, TraceId,
};
use veoveo_mcp_gateway::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshRotationRequest,
    GatewayState, RefreshTokenDeliveryCipher,
};
use veoveo_platform_store::{
    GatewayAuditKind, GatewayRefreshTokenRecord, PlatformStore, StoreConfig, StoreCredentials,
};

#[tokio::test]
async fn gateway_correctness_state_is_shared_and_single_use_across_replicas() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let (bootstrap, runtime) = store_configs();
    let bootstrap_store = PlatformStore::connect(bootstrap).await.unwrap();
    bootstrap_store
        .replace_database_editor(
            "gateway_runtime",
            &SecretString::from("gateway-runtime-password"),
        )
        .await
        .unwrap();
    let first = GatewayState::new(PlatformStore::connect(runtime.clone()).await.unwrap());
    let second = GatewayState::new(PlatformStore::connect(runtime).await.unwrap());
    let now = Utc::now();
    let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
    let client_id = OAuthClientId::new("operator-console").unwrap();

    for (jwt_id, register_id_jag) in [("assertion-jti", false), ("id-jag-jti", true)] {
        let jwt_id = JwtId::new(jwt_id).unwrap();
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

    let profile = GatewayProfileId::new("operator").unwrap();
    let issuer = TokenIssuer::new("https://idp.example.com").unwrap();
    let revoked_jwt = JwtId::new("revoked-jwt").unwrap();
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
        owner: PrincipalId::new("https://idp.example.com#alice").unwrap(),
        upstream_server: ServerSlug::new("artifact").unwrap(),
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
        .issue_refresh_token(
            &authorization_server,
            &profile,
            &client_id,
            &principal,
            &principal.scopes,
            now,
        )
        .await
        .unwrap();
    let presented_refresh = issued_refresh.token.clone();
    let delivery_cipher = test_refresh_delivery_cipher();
    let left_refresh_audit = auth_audit_event("refresh-left", now, &profile, &principal);
    let right_refresh_audit = auth_audit_event("refresh-right", now, &profile, &principal);
    let left_duplicate_audit =
        duplicate_delivery_audit_event("refresh-left-duplicate", now, &profile, &principal);
    let right_duplicate_audit =
        duplicate_delivery_audit_event("refresh-right-duplicate", now, &profile, &principal);
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

    let delayed_replay_audit =
        auth_audit_event("refresh-delayed-replay", now, &profile, &principal);
    let delayed_duplicate_audit = duplicate_delivery_audit_event(
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
        auth_audit_event("refresh-revoked-successor", now, &profile, &principal);
    let revoked_successor_duplicate_audit = duplicate_delivery_audit_event(
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
        .query("SELECT * FROM gateway_refresh_token ORDER BY generation ASC;")
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

    let old_policy =
        policy_audit_event("policy-old", now - TimeDelta::days(2), &profile, &principal);
    first.record_audit_event(&old_policy).await.unwrap();
    let auth = auth_audit_event("auth-current", now, &profile, &principal);
    second.record_auth_audit_event(&auth).await.unwrap();
    assert_eq!(second.audit_counts().await.unwrap().policy_events, 1);
    assert_eq!(first.audit_counts().await.unwrap().auth_events, 3);
    assert_eq!(
        first
            .platform_store()
            .gateway_audit_events(GatewayAuditKind::Policy)
            .await
            .unwrap()
            .len(),
        1,
        "gateway policy evidence must live in canonical audit_event",
    );
    let outbox = first.platform_store().read_outbox(0, 100).await.unwrap();
    assert!(
        outbox
            .events
            .iter()
            .any(|event| event.event_type == "gateway.audit.recorded"),
        "canonical gateway audit writes must publish to the durable outbox",
    );
    for event_type in [
        "gateway.refresh_family.issued",
        "gateway.refresh_token.rotated",
        "gateway.refresh_family.revoked",
    ] {
        assert!(
            outbox
                .events
                .iter()
                .any(|event| event.event_type == event_type),
            "missing durable refresh outbox event {event_type}",
        );
    }

    let retention = second
        .delete_audit_events_before(now - TimeDelta::days(1))
        .await
        .unwrap();
    assert_eq!(retention.policy_events_deleted, 1);
    assert_eq!(retention.auth_events_deleted, 0);
    assert_eq!(first.audit_counts().await.unwrap().policy_events, 0);
    assert_eq!(first.audit_counts().await.unwrap().auth_events, 3);

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
        .query("SELECT * FROM gateway_refresh_token ORDER BY generation ASC;")
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
}

#[tokio::test]
async fn refresh_rotation_rolls_back_when_success_audit_cannot_commit() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let (bootstrap, runtime) = store_configs();
    let bootstrap_store = PlatformStore::connect(bootstrap).await.unwrap();
    bootstrap_store
        .replace_database_editor(
            "gateway_runtime",
            &SecretString::from("gateway-runtime-password"),
        )
        .await
        .unwrap();
    let state = GatewayState::new(PlatformStore::connect(runtime).await.unwrap());
    let now = Utc::now();
    let delivery_cipher = test_refresh_delivery_cipher();
    let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
    let profile = GatewayProfileId::new("operator").unwrap();
    let client_id = OAuthClientId::new("operator-console").unwrap();
    let principal = authorization_code(now, &profile, &client_id).principal;
    let issued = state
        .issue_refresh_token(
            &authorization_server,
            &profile,
            &client_id,
            &principal,
            &principal.scopes,
            now,
        )
        .await
        .unwrap();
    let duplicate_audit = auth_audit_event("duplicate-refresh-audit", now, &profile, &principal);
    let duplicate_delivery_audit = duplicate_delivery_audit_event(
        "duplicate-refresh-delivery-audit",
        now,
        &profile,
        &principal,
    );
    state
        .record_auth_audit_event(&duplicate_audit)
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
        .expect_err("duplicate success audit must roll back the refresh rotation");
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

    let retry_audit = auth_audit_event("refresh-delivery-retry", now, &profile, &principal);
    let retry_duplicate_audit = duplicate_delivery_audit_event(
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
}

#[tokio::test]
async fn consuming_a_successor_clears_its_delivery_envelope_atomically() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let (bootstrap, runtime) = store_configs();
    let bootstrap_store = PlatformStore::connect(bootstrap).await.unwrap();
    bootstrap_store
        .replace_database_editor(
            "gateway_runtime",
            &SecretString::from("gateway-runtime-password"),
        )
        .await
        .unwrap();
    let state = GatewayState::new(PlatformStore::connect(runtime).await.unwrap());
    let now = Utc::now();
    let delivery_cipher = test_refresh_delivery_cipher();
    let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
    let profile = GatewayProfileId::new("operator").unwrap();
    let client_id = OAuthClientId::new("operator-console").unwrap();
    let principal = authorization_code(now, &profile, &client_id).principal;
    let issued = state
        .issue_refresh_token(
            &authorization_server,
            &profile,
            &client_id,
            &principal,
            &principal.scopes,
            now,
        )
        .await
        .unwrap();
    let first_audit = auth_audit_event("eager-clear-first", now, &profile, &principal);
    let first_duplicate_audit =
        duplicate_delivery_audit_event("eager-clear-first-duplicate", now, &profile, &principal);
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

    let blocked_audit = auth_audit_event("eager-clear-blocked", now, &profile, &principal);
    state.record_auth_audit_event(&blocked_audit).await.unwrap();
    let blocked_duplicate_audit =
        duplicate_delivery_audit_event("eager-clear-blocked-duplicate", now, &profile, &principal);
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

    let consume_audit = auth_audit_event("eager-clear-consume", now, &profile, &principal);
    let consume_duplicate_audit =
        duplicate_delivery_audit_event("eager-clear-consume-duplicate", now, &profile, &principal);
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
}

#[tokio::test]
async fn public_client_revocation_is_bound_idempotent_and_family_wide() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }

    let (bootstrap, runtime) = store_configs();
    let bootstrap_store = PlatformStore::connect(bootstrap).await.unwrap();
    bootstrap_store
        .replace_database_editor(
            "gateway_runtime",
            &SecretString::from("gateway-runtime-password"),
        )
        .await
        .unwrap();
    let state = GatewayState::new(PlatformStore::connect(runtime).await.unwrap());
    let now = Utc::now();
    let delivery_cipher = test_refresh_delivery_cipher();
    let authorization_server = AuthorizationServerId::new("veoveo").unwrap();
    let profile = GatewayProfileId::new("operator").unwrap();
    let client_id = OAuthClientId::new("operator-console").unwrap();
    let principal = authorization_code(now, &profile, &client_id).principal;
    let issued = state
        .issue_refresh_token(
            &authorization_server,
            &profile,
            &client_id,
            &principal,
            &principal.scopes,
            now,
        )
        .await
        .unwrap();

    assert!(
        state
            .revoke_refresh_token_family(
                &issued.token,
                &authorization_server,
                &profile,
                &OAuthClientId::new("different-client").unwrap(),
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
    let rejected_audit = auth_audit_event("revoked-family-rotate", now, &profile, &principal);
    let rejected_duplicate_audit = duplicate_delivery_audit_event(
        "revoked-family-rotate-duplicate",
        now,
        &profile,
        &principal,
    );
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

    let outbox = state.platform_store().read_outbox(0, 100).await.unwrap();
    assert_eq!(
        outbox
            .events
            .iter()
            .filter(|event| event.event_type == "gateway.refresh_family.revoked")
            .count(),
        1,
        "idempotent revocation must publish one family-revoked event",
    );
}

fn store_configs() -> (StoreConfig, StoreConfig) {
    let endpoint = std::env::var("VEOVEO_SURREAL_ENDPOINT")
        .unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let namespace = std::env::var("VEOVEO_SURREAL_NAMESPACE")
        .unwrap_or_else(|_| "veoveo_gateway_integration".to_owned());
    let database_prefix =
        std::env::var("VEOVEO_SURREAL_DATABASE").unwrap_or_else(|_| "gateway_state".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USERNAME").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("{database_prefix}_{}", Uuid::now_v7().simple());
    let bootstrap = StoreConfig::builder(
        &endpoint,
        &namespace,
        &database,
        StoreCredentials::root(username, password),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();
    let runtime = StoreConfig::builder(
        endpoint,
        namespace,
        database,
        StoreCredentials::database("gateway_runtime", "gateway-runtime-password"),
    )
    .build()
    .unwrap();
    (bootstrap, runtime)
}

fn authorization_request(
    now: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    client_id: &OAuthClientId,
) -> GatewayAuthorizationRequest {
    GatewayAuthorizationRequest {
        idp_state: OAuthStateValue::new("integration-idp-state").unwrap(),
        profile: profile.clone(),
        oauth_client_id: client_id.clone(),
        oidc_client: OidcClientRegistrationId::new("enterprise").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
        client_state: Some(OAuthStateValue::new("client-state").unwrap()),
        requested_scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        code_challenge: PkceCodeChallenge::new("A".repeat(43)).unwrap(),
        code_challenge_method: PkceCodeChallengeMethod::S256,
        idp_code_verifier: PkceCodeVerifier::new("B".repeat(43)).unwrap(),
        idp_code_challenge: PkceCodeChallenge::new("C".repeat(43)).unwrap(),
        idp_code_challenge_method: PkceCodeChallengeMethod::S256,
        nonce: OidcNonce::new("integration-nonce").unwrap(),
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
        id: PrincipalId::new("https://idp.example.com#alice").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        subject: TokenSubject::new("alice").unwrap(),
        tenant: Some(TenantId::new("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(now),
    };
    GatewayAuthorizationCodeRecord {
        code: OAuthAuthorizationCode::new("D".repeat(43)).unwrap(),
        profile: profile.clone(),
        oauth_client_id: client_id.clone(),
        oidc_client: OidcClientRegistrationId::new("enterprise").unwrap(),
        redirect_uri: OAuthRedirectUri::new("https://veoveo.example/oauth/callback").unwrap(),
        client_state: None,
        scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        code_challenge: PkceCodeChallenge::new("E".repeat(43)).unwrap(),
        code_challenge_method: PkceCodeChallengeMethod::S256,
        principal,
        issued_at: now,
        expires_at: now + TimeDelta::minutes(5),
        consumed_at: None,
    }
}

fn policy_audit_event(
    id: &str,
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
) -> AuditEvent {
    let trace_id = TraceId::new(format!("trace-{id}")).unwrap();
    let target = PolicyTarget::Gateway;
    let decision = PolicyDecision {
        effect: PolicyEffect::Allow,
        reason: PolicyReasonCode::PolicyAllow,
        evaluated_at: timestamp,
        profile: profile.clone(),
        action: GatewayAction::AdminRead,
        target: target.clone(),
        principal: Some(principal.id.clone()),
        tenant: principal.tenant.clone(),
        policy_version: None,
        rule_id: None,
        trace_id: trace_id.clone(),
    };
    AuditEvent {
        event_id: TraceId::new(id).unwrap(),
        timestamp,
        trace_id,
        profile: profile.clone(),
        method: McpMethodName::new("admin/integration").unwrap(),
        action: GatewayAction::AdminRead,
        target,
        decision,
        principal: Some(principal.id.clone()),
        principal_attributes: Some(PrincipalAuditAttributes::from(principal)),
        tenant: principal.tenant.clone(),
        token_issuer: Some(principal.issuer.clone()),
        latency_ms: Some(1),
        metadata: BTreeMap::from([("test".to_owned(), "policy".to_owned())]),
    }
}

fn auth_audit_event(
    id: &str,
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
) -> AuthAuditEvent {
    AuthAuditEvent {
        event_id: TraceId::new(id).unwrap(),
        timestamp,
        trace_id: TraceId::new(format!("trace-{id}")).unwrap(),
        profile: profile.clone(),
        protected_resource: ProtectedResourceId::new("operator-resource").unwrap(),
        outcome: AuthOutcome::Allow,
        reason: AuthReasonCode::AuthAllow,
        method: AuthMethod::BearerJwt,
        principal: Some(principal.id.clone()),
        principal_attributes: Some(PrincipalAuditAttributes::from(principal)),
        tenant: principal.tenant.clone(),
        token_issuer: Some(principal.issuer.clone()),
        token_subject: Some(principal.subject.clone()),
        jwt_id: Some(JwtId::new("auth-jwt").unwrap()),
        latency_ms: Some(1),
        metadata: BTreeMap::from([("test".to_owned(), "auth".to_owned())]),
    }
}

fn duplicate_delivery_audit_event(
    id: &str,
    timestamp: chrono::DateTime<Utc>,
    profile: &GatewayProfileId,
    principal: &Principal,
) -> AuthAuditEvent {
    let mut event = auth_audit_event(id, timestamp, profile, principal);
    event.reason = AuthReasonCode::RefreshTokenDuplicateDelivery;
    event
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
    success_audit: &'a AuthAuditEvent,
    duplicate_delivery_audit: &'a AuthAuditEvent,
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
        .query("SELECT * FROM gateway_refresh_token WHERE generation = $generation;")
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
