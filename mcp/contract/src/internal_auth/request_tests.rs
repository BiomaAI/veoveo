use super::*;
use chrono::TimeDelta;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    actor: Principal,
    authority: InvocationAuthority,
    request_context: GatewayRequestContext,
}
fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(include_str!(
        "../../../../testing/fixtures/gateway-request-context.json"
    ))
    .unwrap()
}

#[test]
fn all_invocation_modes_preserve_signed_source_and_session_context() {
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse("veoveo-internal").unwrap(),
        tests::signing_key("key-1"),
    );
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::parse("veoveo-internal").unwrap(),
        ServerSlug::parse("computers").unwrap(),
        tests::trust_bundle("key-1"),
    );
    for fixture in fixtures() {
        let issued = issuer
            .issue(
                GatewayProfileId::parse("operator").unwrap(),
                ServerSlug::parse("computers").unwrap(),
                fixture.actor,
                fixture.authority,
                Some(fixture.request_context.clone()),
                Utc::now() + TimeDelta::seconds(60),
            )
            .unwrap();
        let received = verifier.verify(&issued.bearer_token).unwrap();
        assert_eq!(
            received.request_context,
            Some(fixture.request_context),
            "{}",
            fixture.name
        );
    }
}

#[test]
fn source_expiry_caps_assertions_and_cannot_be_extended_by_a_signed_context() {
    let mut fixture = fixtures().remove(0);
    fixture.request_context.access_token.expires_at = Utc::now() + TimeDelta::seconds(10);
    let key = tests::signing_key("key-1");
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse("veoveo-internal").unwrap(),
        key.clone(),
    );
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::parse("veoveo-internal").unwrap(),
        ServerSlug::parse("computers").unwrap(),
        tests::trust_bundle("key-1"),
    );
    let issue = |context| {
        issuer.issue(
            GatewayProfileId::parse("operator").unwrap(),
            ServerSlug::parse("computers").unwrap(),
            fixture.actor.clone(),
            fixture.authority.clone(),
            Some(context),
            Utc::now() + TimeDelta::seconds(60),
        )
    };
    let issued = issue(fixture.request_context.clone()).unwrap();
    assert_eq!(
        issued.identity.expires_at,
        fixture.request_context.access_token.expires_at
    );
    verifier.verify(&issued.bearer_token).unwrap();
    let mut claims = GatewayInternalJwtClaims::from_identity(&issued.identity);
    claims.exp += 1;
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some("key-1".into());
    let forged = encode(
        &header,
        &claims,
        &EncodingKey::from_ed_der(&key.private_key_der),
    )
    .unwrap();
    assert!(matches!(
        verifier.verify(&forged),
        Err(InternalTokenError::InvalidRequestContext)
    ));
    fixture.request_context.access_token.expires_at = Utc::now() - TimeDelta::seconds(1);
    assert!(matches!(
        issue(fixture.request_context),
        Err(InternalTokenError::ExpiredDelegation)
    ));
}

#[test]
fn signed_context_rejects_mismatched_actor_tenant_scope_and_provenance() {
    for fixture in fixtures() {
        for mismatch in [
            "subject", "issuer", "tenant", "context", "scope", "mode", "actor",
        ] {
            let mut context = fixture.request_context.clone();
            let mut actor = fixture.actor.clone();
            match mismatch {
                "subject" => {
                    context.access_token.subject = crate::TokenSubject::parse("foreign").unwrap()
                }
                "issuer" => {
                    context.access_token.issuer =
                        TokenIssuer::parse("https://foreign.example").unwrap()
                }
                "tenant" => {
                    context.principal.tenant =
                        Some(veoveo_types::TenantId::parse("foreign").unwrap())
                }
                "context" => {
                    context.access_token.work_context =
                        veoveo_types::WorkContextId::parse("foreign").unwrap()
                }
                "scope" => {
                    context
                        .access_token
                        .scopes
                        .insert(veoveo_types::ScopeName::parse("admin:use").unwrap());
                }
                "mode" => {
                    context.access_token.invocation_mode = if context.access_token.invocation_mode
                        == veoveo_types::InvocationMode::Direct
                    {
                        veoveo_types::InvocationMode::Automated
                    } else {
                        veoveo_types::InvocationMode::Direct
                    }
                }
                _ => actor.id = PrincipalId::parse("foreign").unwrap(),
            }
            assert!(
                context.validate_for(&actor, &fixture.authority).is_err(),
                "{} {mismatch}",
                fixture.name
            );
        }
    }
}

#[test]
fn assertion_profiles_reject_mixed_versions_and_invalid_execution_attribution() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| {
            fixture
                .request_context
                .access_token
                .managed_execution
                .is_some()
        })
        .unwrap();
    let key = tests::signing_key("key-1");
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse("veoveo-internal").unwrap(),
        key.clone(),
    );
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::parse("veoveo-internal").unwrap(),
        ServerSlug::parse("computers").unwrap(),
        tests::trust_bundle("key-1"),
    );
    let issued = issuer
        .issue(
            GatewayProfileId::parse("operator").unwrap(),
            ServerSlug::parse("computers").unwrap(),
            fixture.actor.clone(),
            fixture.authority.clone(),
            Some(fixture.request_context.clone()),
            Utc::now() + TimeDelta::seconds(60),
        )
        .unwrap();
    let original: serde_json::Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(issued.bearer_token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some("key-1".into());
    for variant in 0..7 {
        let mut claims = original.clone();
        match variant {
            0 => {
                claims.as_object_mut().unwrap().remove("format");
            }
            1 => claims["format"] = serde_json::json!("veoveo.ai/gateway-internal-assertion/v1"),
            2 => {
                claims["request_context"]
                    .as_object_mut()
                    .unwrap()
                    .remove("format");
            }
            3 => {
                claims["request_context"]["format"] =
                    serde_json::json!("veoveo.ai/gateway-request-context/v1")
            }
            4 => {
                claims["request_context"]["access_token"]["managed_execution"]["generation"] =
                    serde_json::json!(0)
            }
            5 => {
                claims["request_context"]["access_token"]["managed_execution"]["dispatch_epoch"] =
                    serde_json::json!(-1)
            }
            _ => {
                claims["request_context"]["access_token"]["managed_agent"] =
                    serde_json::json!({"instance":"fixture-agent","generation":1,"epoch":2});
            }
        }
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_ed_der(&key.private_key_der),
        )
        .unwrap();
        assert!(verifier.verify(&token).is_err(), "{variant}");
    }
    let mut value = serde_json::to_value(&fixture.request_context).unwrap();
    value["access_token"]["session_family"] =
        serde_json::json!("019b7b88-7f03-7123-8123-abcdefabcdef");
    let context: GatewayRequestContext = serde_json::from_value(value).unwrap();
    assert!(
        context
            .validate_for(&fixture.actor, &fixture.authority)
            .is_err()
    );
}

#[test]
fn signed_execution_projection_preserves_frozen_audit_actor_shape() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| {
            fixture
                .request_context
                .access_token
                .managed_execution
                .is_some()
        })
        .unwrap();
    let projection = fixture
        .request_context
        .access_token
        .managed_execution
        .clone()
        .unwrap();
    let audit = fixture
        .request_context
        .audit_context(
            &fixture.actor,
            &fixture.authority,
            &GatewayProfileId::parse("operator").unwrap(),
        )
        .unwrap();
    assert_eq!(audit.actor.managed_agent, Some(projection));
    let actor = serde_json::to_value(audit.actor).unwrap();
    assert_eq!(actor["managed_agent"]["dispatch_epoch"], 2);
    assert!(actor.get("managed_execution").is_none());
}
