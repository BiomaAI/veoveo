//! Gateway internal authentication for hosted servers.
//!
//! The middleware verifies the gateway-signed internal assertion on every request
//! and places the verified [`GatewayInternalIdentity`] and the raw
//! [`ForwardedBearer`] into the request extensions. Handlers read them back with
//! [`gateway_identity`], [`forwarded_bearer`], and [`plane_caller`]. A request that
//! reached a handler without the middleware fails closed with
//! [`GATEWAY_ROUTING_REQUIRED`].

use std::{fmt, sync::Arc};

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rmcp::{ErrorData, RoleServer, service::RequestContext};

use crate::{
    GATEWAY_ROUTING_REQUIRED, GatewayInternalIdentity, GatewayInternalTokenVerifier, PlaneCaller,
    ServerSlug,
};

/// The gateway bearer this server received. Servers forward it to the shared
/// Artifact plane on the caller's behalf; it is never logged or displayed.
#[derive(Clone, PartialEq, Eq)]
pub struct ForwardedBearer(String);

impl ForwardedBearer {
    /// Wraps a bearer the middleware did not produce, such as a test fixture.
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for ForwardedBearer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ForwardedBearer(<redacted>)")
    }
}

/// Middleware state: the verifier bound to this server's audience.
#[derive(Clone)]
pub(super) struct InternalAuth {
    pub(super) verifier: Arc<GatewayInternalTokenVerifier>,
    pub(super) slug: ServerSlug,
}

pub(super) async fn authenticate(
    State(auth): State<InternalAuth>,
    mut request: Request,
    next: Next,
) -> Response {
    let (identity, bearer) = match verify(&auth.verifier, request.headers()) {
        Ok(verified) => verified,
        Err(reason) => {
            tracing::warn!(server = auth.slug.as_str(), reason, "rejected MCP request");
            return (StatusCode::UNAUTHORIZED, "invalid gateway authorization").into_response();
        }
    };
    request.extensions_mut().insert(bearer);
    request.extensions_mut().insert(identity);
    next.run(request).await
}

fn verify(
    verifier: &GatewayInternalTokenVerifier,
    headers: &HeaderMap,
) -> Result<(GatewayInternalIdentity, ForwardedBearer), &'static str> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or("missing internal authorization")?;
    let token = bearer_token(header)?;
    let identity = verifier
        .verify(token)
        .map_err(|_| "internal authorization did not verify")?;
    Ok((identity, ForwardedBearer(token.to_owned())))
}

fn bearer_token(header: &str) -> Result<&str, &'static str> {
    let Some((scheme, token)) = header.split_once(' ') else {
        return Err("missing bearer token");
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err("authorization scheme must be Bearer");
    }
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("bearer token contains invalid whitespace");
    }
    Ok(token)
}

fn routing_required() -> ErrorData {
    ErrorData::invalid_request(GATEWAY_ROUTING_REQUIRED, None)
}

fn http_parts(
    context: &RequestContext<RoleServer>,
) -> Result<&axum::http::request::Parts, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(routing_required)
}

/// The verified gateway identity for this request.
pub fn gateway_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, ErrorData> {
    http_parts(context)?
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(routing_required)
}

/// The gateway bearer for this request, for forwarding to the Artifact plane.
pub fn forwarded_bearer(
    context: &RequestContext<RoleServer>,
) -> Result<ForwardedBearer, ErrorData> {
    http_parts(context)?
        .extensions
        .get::<ForwardedBearer>()
        .cloned()
        .ok_or_else(routing_required)
}

/// The caller as the Artifact plane sees it: identity, memberships, and bearer.
pub fn plane_caller(context: &RequestContext<RoleServer>) -> Result<PlaneCaller, ErrorData> {
    Ok(PlaneCaller::from_gateway(
        gateway_identity(context)?,
        forwarded_bearer(context)?,
    ))
}

impl PlaneCaller {
    /// Builds the Artifact-plane caller from a verified identity and its bearer.
    pub fn from_gateway(identity: GatewayInternalIdentity, bearer: ForwardedBearer) -> Self {
        let memberships = identity.actor.group_memberships();
        Self {
            bearer_token: bearer.into_string(),
            identity,
            memberships,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bearer_token;

    #[test]
    fn bearer_tokens_require_the_bearer_scheme_and_no_whitespace() {
        assert_eq!(bearer_token("Bearer abc"), Ok("abc"));
        assert_eq!(bearer_token("bearer abc"), Ok("abc"));
        assert!(bearer_token("Basic abc").is_err());
        assert!(bearer_token("Bearer").is_err());
        assert!(bearer_token("Bearer a b").is_err());
        assert!(bearer_token("Bearer ").is_err());
    }

    #[test]
    fn forwarded_bearers_never_print_their_value() {
        let bearer = super::ForwardedBearer("secret".into());
        assert_eq!(format!("{bearer:?}"), "ForwardedBearer(<redacted>)");
    }
}
