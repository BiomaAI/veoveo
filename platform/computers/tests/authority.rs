use chrono::{TimeDelta, Utc};
use serde::Deserialize;
use veoveo_computers::{ComputerActor, ComputerError};
use veoveo_mcp_contract::*;

#[derive(Deserialize)]
struct Fixture {
    actor: Principal,
    authority: InvocationAuthority,
    request_context: GatewayRequestContext,
}

#[test]
fn admission_requires_verified_computer_context_for_every_invocation_mode() {
    let fixtures: Vec<Fixture> = serde_json::from_str(include_str!(
        "../../../testing/fixtures/gateway-request-context.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let now = Utc::now();
        let identity = GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new("operator").unwrap(),
            server: ServerSlug::new("computers").unwrap(),
            actor: fixture.actor,
            authority: fixture.authority,
            request_context: Some(fixture.request_context),
            jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + TimeDelta::minutes(1),
        };
        let admitted = ComputerActor::from_verified(&identity).unwrap();
        assert_eq!(admitted.owner().principal_key, identity.actor.id.as_str());
        assert_eq!(admitted.owner().authority, identity.authority);
        for mismatch in [
            "missing",
            "audience",
            "expired_assertion",
            "expired_source",
            "not_yet_valid",
            "tenant",
        ] {
            let mut wrong = identity.clone();
            match mismatch {
                "missing" => wrong.request_context = None,
                "audience" => wrong.server = ServerSlug::new("artifact").unwrap(),
                "expired_assertion" => wrong.expires_at = now - TimeDelta::seconds(1),
                "expired_source" => {
                    wrong
                        .request_context
                        .as_mut()
                        .unwrap()
                        .access_token
                        .expires_at = now - TimeDelta::seconds(1)
                }
                "not_yet_valid" => wrong.not_before = now + TimeDelta::seconds(60),
                _ => wrong.authority.tenant = TenantId::new("foreign").unwrap(),
            }
            assert!(
                matches!(
                    ComputerActor::from_verified(&wrong),
                    Err(ComputerError::Forbidden)
                ),
                "{mismatch}"
            );
        }
    }
}
