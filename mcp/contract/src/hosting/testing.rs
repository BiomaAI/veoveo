//! In-process gateway for testing a hosted server without a deployment.
//!
//! Enable the `testing` feature in a server's dev-dependencies. [`for_domain`]
//! starts the ordinary [`HostedServer`] builder with a fixed test deployment and
//! the test signing key's trust bundle. The caller adds its handler and any
//! server routes, builds, and wraps the result in a [`TestGateway`]. Requests run
//! through the complete router: host validation, gateway authentication, the
//! stateless MCP transport and the domain.
//!
//! ```ignore
//! let gateway = TestGateway::new(
//!     testing::for_domain::<MyDomain>()
//!         .handler(|| Hosted::new(MyDomain::new()))
//!         .build(),
//! );
//! let tools = gateway.rpc("tools/list", json!({})).await;
//! assert_eq!(tools["result"]["tools"][0]["name"], "my_tool");
//! ```
//!
//! The signing key is a published test key. Production servers trust only the
//! JWKS they receive at startup, which never contains it.

use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{TimeDelta, Utc};
use tower::ServiceExt;
use veoveo_types::{
    AccessSubject, GroupId, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
    RoleId, ScopeName, TenantId, WorkContextId, WorkContextMembershipLevel,
    WorkContextOutputPolicy,
};

use super::{
    DomainServer, HostedServer, HostedServerBuilder, Missing, Provided, server::Deployment,
};
use crate::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayInternalTokenVerifier, GatewayInternalTrustBundle, GatewayProfileId, Principal,
    PrincipalKind, PublicDeployment, TokenIssuer, TokenSubject,
};

/// The host authority of the test deployment.
pub const TEST_HOST: &str = "veoveo.test";
/// The protocol version test requests declare.
pub const TEST_PROTOCOL_VERSION: &str = "2026-07-28";

const TEST_KEY_ID: &str = "test-key";
const PRIVATE_KEY_DER_B64: &str =
    "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
pub(crate) const PUBLIC_KEY_X: &str = "OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg";

/// The test signing key under `key_id`.
pub fn signing_key(key_id: &str) -> GatewayInternalSigningKey {
    GatewayInternalSigningKey::new(key_id, STANDARD.decode(PRIVATE_KEY_DER_B64).unwrap()).unwrap()
}

/// A trust bundle holding the test public key under `key_id`.
pub fn trust_bundle(key_id: &str) -> GatewayInternalTrustBundle {
    GatewayInternalTrustBundle::from_json(&format!(
        r#"{{"keys":[{{"kty":"OKP","crv":"Ed25519","x":"{PUBLIC_KEY_X}","alg":"EdDSA","use":"sig","kid":"{key_id}"}}]}}"#
    ))
    .unwrap()
}

/// The test user: `user-1` in `tenant-a`, operator, without data labels.
pub fn principal() -> Principal {
    Principal {
        id: PrincipalId::new("https://idp.example.com#user-1").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        subject: TokenSubject::new("user-1").unwrap(),
        tenant: Some(TenantId::new("tenant-a").unwrap()),
        groups: BTreeSet::from([GroupId::new("engineering").unwrap()]),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
        scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    }
}

/// The test user's direct invocation authority as owner of work context `mission`.
pub fn authority() -> InvocationAuthority {
    InvocationAuthority {
        work_context: WorkContextId::new("mission").unwrap(),
        tenant: TenantId::new("tenant-a").unwrap(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal().id),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        provenance: InvocationProvenance::Direct {
            initiator: principal().id,
        },
    }
}

/// Starts the builder for `D` on the test deployment, trusting the test key.
pub fn for_domain<D: DomainServer>() -> HostedServerBuilder<
    D,
    Provided<Deployment>,
    Provided<Arc<GatewayInternalTokenVerifier>>,
    Missing,
> {
    let deployment = PublicDeployment::new(format!("https://{TEST_HOST}")).unwrap();
    HostedServer::for_domain::<D>()
        .deployment(&deployment, false)
        .unwrap()
        .internal_trust(trust_bundle(TEST_KEY_ID))
        .unwrap()
}

/// Starts the builder for `D` under `/{slug}` on the test host as an internal
/// server, trusting the test key.
pub fn internal_for_domain<D: DomainServer>() -> HostedServerBuilder<
    D,
    Provided<Deployment>,
    Provided<Arc<GatewayInternalTokenVerifier>>,
    Missing,
> {
    HostedServer::for_domain::<D>()
        .internal([TEST_HOST.to_owned()])
        .internal_trust(trust_bundle(TEST_KEY_ID))
        .unwrap()
}

/// Starts the builder for `D` at the internal root on the test host, trusting
/// the test key.
pub fn internal_root_for_domain<D: DomainServer>() -> HostedServerBuilder<
    D,
    Provided<Deployment>,
    Provided<Arc<GatewayInternalTokenVerifier>>,
    Missing,
> {
    HostedServer::for_domain::<D>()
        .internal_root([TEST_HOST.to_owned()])
        .internal_trust(trust_bundle(TEST_KEY_ID))
        .unwrap()
}

/// A built server and the gateway calls that reach it.
pub struct TestGateway {
    router: Router,
    slug: crate::ServerSlug,
    mount: String,
}

impl TestGateway {
    pub fn new(server: HostedServer) -> Self {
        Self {
            slug: server.slug.clone(),
            mount: server.mount.prefix().to_owned(),
            router: server.router,
        }
    }

    /// A gateway assertion for the test user, addressed to this server.
    pub fn token(&self) -> String {
        GatewayInternalTokenIssuer::new(
            TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
            signing_key(TEST_KEY_ID),
        )
        .issue(
            GatewayProfileId::new("operations").unwrap(),
            self.slug.clone(),
            principal(),
            authority(),
            None,
            Utc::now() + TimeDelta::minutes(5),
        )
        .unwrap()
        .bearer_token
    }

    /// Sends a raw request to the router.
    pub async fn send(&self, request: Request<Body>) -> (StatusCode, String) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    /// A request builder for `path` under the server's mount, on the test host.
    pub fn request(&self, path: &str) -> axum::http::request::Builder {
        Request::builder()
            .uri(format!("{}{path}", self.mount))
            .header("host", TEST_HOST)
    }

    /// Calls `method` as the test user and returns the JSON-RPC response.
    pub async fn rpc(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let token = self.token();
        self.rpc_with(method, params, Some(&token)).await.1
    }

    /// Calls `method` with `bearer`, or with no Authorization header.
    pub async fn rpc_with(
        &self,
        method: &str,
        mut params: serde_json::Value,
        bearer: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        params["_meta"] = serde_json::json!({
            "io.modelcontextprotocol/protocolVersion": TEST_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientInfo": {"name": "test-gateway", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let mut request = self
            .request("/mcp")
            .method("POST")
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .header("mcp-protocol-version", TEST_PROTOCOL_VERSION)
            .header("mcp-method", method);
        if let Some(bearer) = bearer {
            request = request.header("authorization", format!("Bearer {bearer}"));
        }
        if let Some(name) = params["name"].as_str().or(params["uri"].as_str()) {
            request = request.header("mcp-name", name);
        }
        let body =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let (status, text) = self
            .send(request.body(Body::from(body.to_string())).unwrap())
            .await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
        )
    }
}
