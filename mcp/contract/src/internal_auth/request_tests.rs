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
        TokenIssuer::new("veoveo-internal").unwrap(),
        tests::signing_key("key-1"),
    );
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new("veoveo-internal").unwrap(),
        ServerSlug::new("computers").unwrap(),
        tests::trust_bundle("key-1"),
    );
    for fixture in fixtures() {
        let issued = issuer
            .issue(
                GatewayProfileId::new("operator").unwrap(),
                ServerSlug::new("computers").unwrap(),
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
    let issuer =
        GatewayInternalTokenIssuer::new(TokenIssuer::new("veoveo-internal").unwrap(), key.clone());
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new("veoveo-internal").unwrap(),
        ServerSlug::new("computers").unwrap(),
        tests::trust_bundle("key-1"),
    );
    let issue = |context| {
        issuer.issue(
            GatewayProfileId::new("operator").unwrap(),
            ServerSlug::new("computers").unwrap(),
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
                    context.access_token.subject = crate::TokenSubject::new("foreign").unwrap()
                }
                "issuer" => {
                    context.access_token.issuer =
                        TokenIssuer::new("https://foreign.example").unwrap()
                }
                "tenant" => {
                    context.principal.tenant = Some(crate::TenantId::new("foreign").unwrap())
                }
                "context" => {
                    context.access_token.work_context =
                        crate::WorkContextId::new("foreign").unwrap()
                }
                "scope" => {
                    context
                        .access_token
                        .scopes
                        .insert(crate::ScopeName::new("admin:use").unwrap());
                }
                "mode" => {
                    context.access_token.invocation_mode =
                        if context.access_token.invocation_mode == crate::InvocationMode::Direct {
                            crate::InvocationMode::Automated
                        } else {
                            crate::InvocationMode::Direct
                        }
                }
                _ => actor.id = PrincipalId::new("foreign").unwrap(),
            }
            assert!(
                context.validate_for(&actor, &fixture.authority).is_err(),
                "{} {mismatch}",
                fixture.name
            );
        }
    }
}
