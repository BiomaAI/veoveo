use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{TimeDelta, Utc};
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_mcp_contract::*;
use veoveo_types::{
    InvocationMode, InvocationProvenance, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

pub struct Signing {
    issuer: GatewayInternalTokenIssuer,
    pub trust: GatewayInternalTrustBundle,
}
impl Signing {
    pub fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let trust = GatewayInternalTrustBundle::from_json(
            &serde_json::json!({"keys":[{
                "kty":"OKP", "crv":"Ed25519", "x":URL_SAFE_NO_PAD.encode(key.public_key_raw()),
                "alg":"EdDSA", "use":"sig", "kid":"artifact-fixture"
            }]})
            .to_string(),
        )
        .unwrap();
        Self {
            issuer: GatewayInternalTokenIssuer::new(
                GATEWAY_INTERNAL_TOKEN_ISSUER.parse().unwrap(),
                GatewayInternalSigningKey::new("artifact-fixture", key.serialize_der()).unwrap(),
            ),
            trust,
        }
    }
    pub fn verifier(&self) -> GatewayInternalTokenVerifier {
        GatewayInternalTokenVerifier::new(
            GATEWAY_INTERNAL_TOKEN_ISSUER.parse().unwrap(),
            "artifact".parse().unwrap(),
            self.trust.clone(),
        )
    }
    pub fn caller(&self, name: &str, context: &str) -> PlaneCaller {
        let now = Utc::now();
        let actor = Principal {
            id: name.parse().unwrap(),
            kind: PrincipalKind::User,
            issuer: "https://artifact.fixture".parse().unwrap(),
            subject: name.parse().unwrap(),
            tenant: Some("artifact-fixture".parse().unwrap()),
            groups: Default::default(),
            group_roles: Default::default(),
            roles: Default::default(),
            scopes: Default::default(),
            data_labels: Default::default(),
            assurances: Default::default(),
            authenticated_at: Some(now),
        };
        let authority = veoveo_types::InvocationAuthority {
            tenant: actor.tenant.clone().unwrap(),
            work_context: context.parse().unwrap(),
            policy_revision: "r1".parse().unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            output_policy: WorkContextOutputPolicy {
                owner: veoveo_types::AccessSubject::Principal(actor.id.clone()),
                initial_grants: vec![],
                classification: None,
                data_labels: Default::default(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: actor.id.clone(),
            },
        };
        let request = GatewayRequestContext {
            format: veoveo_mcp_contract::GatewayRequestContextFormat::V2,
            audit: audit::AuditRequest::background(),
            principal: actor.clone(),
            access_token: AccessTokenSubject {
                managed_execution: None,
                issuer: actor.issuer.clone(),
                subject: actor.subject.clone(),
                oauth_client_id: "artifact-fixture".parse().unwrap(),
                session_family: None,
                audience: ProtectedResourceId::new("https://artifact.fixture/mcp/operator")
                    .unwrap(),
                work_context: authority.work_context.clone(),
                invocation_mode: InvocationMode::Direct,
                initiator: Some(actor.id.clone()),
                delegation_id: None,
                scopes: actor.scopes.clone(),
                jwt_id: Some(uuid::Uuid::now_v7().to_string().parse().unwrap()),
                issued_at: now,
                not_before: None,
                expires_at: now + TimeDelta::minutes(10),
            },
        };
        let token = self
            .issuer
            .issue(
                "operator".parse().unwrap(),
                "artifact".parse().unwrap(),
                actor,
                authority,
                Some(request),
                now + TimeDelta::minutes(10),
            )
            .unwrap();
        let identity = self.verifier().verify(&token.bearer_token).unwrap();
        PlaneCaller {
            memberships: identity.actor.group_memberships(),
            identity,
            bearer_token: token.bearer_token,
        }
    }
}
