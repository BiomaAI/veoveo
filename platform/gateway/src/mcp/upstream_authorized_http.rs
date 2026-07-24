use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderName, HeaderValue};
use chrono::{TimeDelta, Utc};
use futures::stream::BoxStream;
use rmcp::{
    model::ClientJsonRpcMessage,
    transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, SseError, StreamableHttpClient,
        StreamableHttpError, StreamableHttpPostResponse,
    },
};
use sse_stream::Sse;
use thiserror::Error;
use veoveo_mcp_contract::{
    GatewayInternalTokenIssuer, GatewayProfileId, InternalTokenError, InvocationAuthority,
    Principal, ServerSlug,
};

const INTERNAL_REQUEST_TOKEN_TTL_SECONDS: i64 = 60;

/// HTTP authorization owned by one gateway-to-server MCP session.
///
/// Streamable HTTP may issue POST, GET/reconnect, and DELETE requests long
/// after initialization. A static JWT cannot safely represent that session:
/// it expires under a live notification stream and then also prevents the
/// transport from deleting its server-side session. This client signs a
/// short-lived assertion for each HTTP request while retaining one MCP
/// session and one immutable invocation authority.
#[derive(Clone)]
pub(super) struct GatewayAuthorizedHttpClient {
    http: reqwest::Client,
    issuer: GatewayInternalTokenIssuer,
    profile: GatewayProfileId,
    server: ServerSlug,
    actor: Principal,
    authority: InvocationAuthority,
}

impl std::fmt::Debug for GatewayAuthorizedHttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayAuthorizedHttpClient")
            .field("profile", &self.profile)
            .field("server", &self.server)
            .field("actor", &self.actor.id)
            .field("authority", &self.authority)
            .finish_non_exhaustive()
    }
}

impl GatewayAuthorizedHttpClient {
    pub(super) fn new(
        http: reqwest::Client,
        issuer: GatewayInternalTokenIssuer,
        profile: GatewayProfileId,
        server: ServerSlug,
        actor: Principal,
        authority: InvocationAuthority,
    ) -> Self {
        Self {
            http,
            issuer,
            profile,
            server,
            actor,
            authority,
        }
    }

    fn issue_bearer_token(&self) -> Result<String, GatewayAuthorizedHttpError> {
        let expires_at = Utc::now() + TimeDelta::seconds(INTERNAL_REQUEST_TOKEN_TTL_SECONDS);
        self.issuer
            .issue(
                self.profile.clone(),
                self.server.clone(),
                self.actor.clone(),
                self.authority.clone(),
                expires_at,
            )
            .map(|issued| issued.bearer_token)
            .map_err(GatewayAuthorizedHttpError::InternalToken)
    }
}

#[derive(Debug, Error)]
pub(super) enum GatewayAuthorizedHttpError {
    #[error("failed to issue gateway internal request token: {0}")]
    InternalToken(InternalTokenError),
    #[error("upstream HTTP request failed: {0}")]
    Http(reqwest::Error),
}

impl StreamableHttpClient for GatewayAuthorizedHttpClient {
    type Error = GatewayAuthorizedHttpError;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _static_auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let bearer_token = self
            .issue_bearer_token()
            .map_err(StreamableHttpError::Client)?;
        <reqwest::Client as StreamableHttpClient>::post_message(
            &self.http,
            uri,
            message,
            session_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(map_reqwest_transport_error)
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        _static_auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let bearer_token = self
            .issue_bearer_token()
            .map_err(StreamableHttpError::Client)?;
        <reqwest::Client as StreamableHttpClient>::delete_session(
            &self.http,
            uri,
            session_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(map_reqwest_transport_error)
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        last_event_id: Option<String>,
        _static_auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let bearer_token = self
            .issue_bearer_token()
            .map_err(StreamableHttpError::Client)?;
        <reqwest::Client as StreamableHttpClient>::get_stream(
            &self.http,
            uri,
            session_id,
            last_event_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(map_reqwest_transport_error)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_reqwest_transport_error(
    error: StreamableHttpError<reqwest::Error>,
) -> StreamableHttpError<GatewayAuthorizedHttpError> {
    match error {
        StreamableHttpError::Sse(error) => StreamableHttpError::Sse(error),
        StreamableHttpError::Io(error) => StreamableHttpError::Io(error),
        StreamableHttpError::Client(error) => {
            StreamableHttpError::Client(GatewayAuthorizedHttpError::Http(error))
        }
        StreamableHttpError::UnexpectedEndOfStream => StreamableHttpError::UnexpectedEndOfStream,
        StreamableHttpError::UnexpectedServerResponse(response) => {
            StreamableHttpError::UnexpectedServerResponse(response)
        }
        StreamableHttpError::UnexpectedContentType(content_type) => {
            StreamableHttpError::UnexpectedContentType(content_type)
        }
        StreamableHttpError::ServerDoesNotSupportSse => {
            StreamableHttpError::ServerDoesNotSupportSse
        }
        StreamableHttpError::ServerDoesNotSupportDeleteSession => {
            StreamableHttpError::ServerDoesNotSupportDeleteSession
        }
        StreamableHttpError::TokioJoinError(error) => StreamableHttpError::TokioJoinError(error),
        StreamableHttpError::Deserialize(error) => StreamableHttpError::Deserialize(error),
        StreamableHttpError::TransportChannelClosed => StreamableHttpError::TransportChannelClosed,
        StreamableHttpError::MissingSessionIdInResponse => {
            StreamableHttpError::MissingSessionIdInResponse
        }
        StreamableHttpError::AuthRequired(AuthRequiredError {
            www_authenticate_header,
            ..
        }) => StreamableHttpError::AuthRequired(AuthRequiredError::new(www_authenticate_header)),
        StreamableHttpError::InsufficientScope(InsufficientScopeError {
            www_authenticate_header,
            required_scope,
            ..
        }) => StreamableHttpError::InsufficientScope(InsufficientScopeError::new(
            www_authenticate_header,
            required_scope,
        )),
        StreamableHttpError::ReservedHeaderConflict(header) => {
            StreamableHttpError::ReservedHeaderConflict(header)
        }
        StreamableHttpError::SessionExpired => StreamableHttpError::SessionExpired,
        other => StreamableHttpError::UnexpectedServerResponse(other.to_string().into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use veoveo_mcp_contract::{
        AccessSubject, GatewayInternalSigningKey, InvocationProvenance, PolicyVersion, PrincipalId,
        PrincipalKind, ScopeName, TenantId, TokenIssuer, TokenSubject, WorkContextId,
        WorkContextMembershipLevel, WorkContextOutputPolicy,
    };

    use super::*;

    const TEST_SIGNING_KEY_DER_B64: &str =
        "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";

    #[test]
    fn each_http_request_receives_a_fresh_short_lived_assertion() {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            GatewayInternalSigningKey::new(
                "veoveo-internal-1",
                base64::engine::general_purpose::STANDARD
                    .decode(TEST_SIGNING_KEY_DER_B64)
                    .unwrap(),
            )
            .unwrap(),
        );
        let actor_id = PrincipalId::new("https://identity.example#operator").unwrap();
        let actor = Principal {
            id: actor_id.clone(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://identity.example").unwrap(),
            subject: TokenSubject::new("operator").unwrap(),
            tenant: Some(TenantId::new("tenant").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let authority = InvocationAuthority {
            work_context: WorkContextId::new("mission").unwrap(),
            tenant: TenantId::new("tenant").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(actor_id.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: actor_id,
            },
        };
        let client = GatewayAuthorizedHttpClient::new(
            reqwest::Client::new(),
            issuer,
            GatewayProfileId::new("operator").unwrap(),
            ServerSlug::new("uav-sim").unwrap(),
            actor,
            authority,
        );

        let first = client.issue_bearer_token().unwrap();
        let second = client.issue_bearer_token().unwrap();
        assert_ne!(first, second);

        for token in [first, second] {
            let payload = token.split('.').nth(1).unwrap();
            let claims: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
            assert_eq!(
                claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(),
                INTERNAL_REQUEST_TOKEN_TTL_SECONDS
            );
            assert_eq!(claims["server"], "uav-sim");
            assert_eq!(claims["profile"], "operator");
        }
    }
}
