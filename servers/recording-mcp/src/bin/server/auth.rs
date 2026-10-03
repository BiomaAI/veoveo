//! The Artifact-read authority Recording forwards for playback and sealing.
use axum::http::HeaderMap;
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};

pub(super) const ARTIFACT_READ_AUTHORIZATION_HEADER: &str = "x-veoveo-artifact-read-authorization";

pub(super) fn artifact_caller(
    identity: GatewayInternalIdentity,
    headers: &HeaderMap,
) -> Result<PlaneCaller, &'static str> {
    let bearer = bearer_from_name(headers, ARTIFACT_READ_AUTHORIZATION_HEADER)?;
    Ok(PlaneCaller {
        memberships: identity.actor.group_memberships(),
        identity,
        bearer_token: bearer.to_owned(),
    })
}

pub(super) fn artifact_caller_from_context(
    context: &RequestContext<RoleServer>,
    identity: GatewayInternalIdentity,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    artifact_caller(identity, &parts.headers)
        .map_err(|_| McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None))
}

fn bearer_from_name(
    headers: &HeaderMap,
    name: impl axum::http::header::AsHeaderName,
) -> Result<&str, &'static str> {
    let header = headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or("missing authorization")?;
    let Some((scheme, token)) = header.split_once(' ') else {
        return Err("missing bearer token");
    };
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.chars().any(char::is_whitespace)
    {
        return Err("invalid bearer token");
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_read_bearer_parser_is_strict() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ARTIFACT_READ_AUTHORIZATION_HEADER,
            "Bearer one.two.three".parse().unwrap(),
        );
        assert_eq!(
            bearer_from_name(&headers, ARTIFACT_READ_AUTHORIZATION_HEADER),
            Ok("one.two.three")
        );
        headers.insert(
            ARTIFACT_READ_AUTHORIZATION_HEADER,
            "Basic one.two.three".parse().unwrap(),
        );
        assert!(bearer_from_name(&headers, ARTIFACT_READ_AUTHORIZATION_HEADER).is_err());
    }
}
