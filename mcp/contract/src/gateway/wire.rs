use url::{Host, Url};

use super::*;

pub(super) fn validate_path_id(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(
            value,
            "must not be empty and must contain lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

pub(super) fn validate_gateway_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

pub(super) fn validate_compatibility_helper_id(value: &str) -> Result<(), IdentifierError> {
    let Some((namespace, helper)) = value.split_once('.') else {
        return Err(IdentifierError::new(
            value,
            "must be `{namespace}.{helper}` using gateway-safe identifiers",
        ));
    };
    if namespace.contains('.') || helper.contains('.') {
        return Err(IdentifierError::new(
            value,
            "must contain exactly one dot separator",
        ));
    }
    validate_gateway_name(namespace)?;
    validate_gateway_name(helper)?;
    Ok(())
}

pub(super) fn validate_uri_scheme(value: &str) -> Result<(), IdentifierError> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(IdentifierError::new(value, "must not be empty"));
    };
    if !first.is_ascii_lowercase() {
        return Err(IdentifierError::new(
            value,
            "must start with a lowercase ASCII letter",
        ));
    }
    if !bytes.all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.'
    }) {
        return Err(IdentifierError::new(
            value,
            "must follow URI scheme syntax with lowercase ASCII characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_token_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_oauth_state_value(value: &str) -> Result<(), IdentifierError> {
    validate_token_text(value)?;
    if value.len() > 512 {
        return Err(IdentifierError::new(value, "must be at most 512 bytes"));
    }
    Ok(())
}

pub(super) fn validate_oauth_authorization_code(value: &str) -> Result<(), IdentifierError> {
    validate_pkce_code_token(value)
}

pub(super) fn validate_pkce_code_token(value: &str) -> Result<(), IdentifierError> {
    if !(43..=128).contains(&value.len()) {
        return Err(IdentifierError::new(value, "must be 43 to 128 bytes"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
    {
        return Err(IdentifierError::new(
            value,
            "must contain only ASCII letters, digits, hyphen, period, underscore, or tilde",
        ));
    }
    Ok(())
}

pub(super) fn validate_claim_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::new(
            value,
            "must not contain control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_mount_path(value: &str) -> Result<(), IdentifierError> {
    if !value.starts_with('/') || value.len() == 1 {
        return Err(IdentifierError::new(
            value,
            "must be an absolute path with at least one segment",
        ));
    }
    if value.ends_with('/') {
        return Err(IdentifierError::new(value, "must not end with slash"));
    }
    if value.contains("//") || value.contains(['?', '#']) {
        return Err(IdentifierError::new(
            value,
            "must not contain empty segments, query, or fragment",
        ));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_https_url(value: &str) -> Result<(), IdentifierError> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    let url = Url::parse(value).map_err(|_| IdentifierError::new(value, "must be a valid URL"))?;
    if url.scheme() != "https" {
        return Err(IdentifierError::new(value, "must use https://"));
    }
    if url.host().is_none() {
        return Err(IdentifierError::new(value, "must include a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.fragment().is_some() {
        return Err(IdentifierError::new(value, "must not contain a fragment"));
    }
    Ok(())
}

pub(super) fn validate_upstream_url(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    let url = Url::parse(value).map_err(|_| IdentifierError::new(value, "must be a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(IdentifierError::new(value, "must use http:// or https://"));
    }
    if url.host().is_none() {
        return Err(IdentifierError::new(value, "must include a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(IdentifierError::new(
            value,
            "must not contain a query or fragment",
        ));
    }
    Ok(())
}

pub(super) fn validate_oauth_redirect_uri(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    let url = Url::parse(value).map_err(|_| IdentifierError::new(value, "must be a valid URL"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.fragment().is_some() {
        return Err(IdentifierError::new(value, "must not contain a fragment"));
    }
    match url.scheme() {
        "https" => {
            if url.host().is_none() {
                return Err(IdentifierError::new(value, "must include a host"));
            }
            Ok(())
        }
        "http" => {
            let is_loopback = match url.host() {
                Some(Host::Domain(host)) => host == "localhost",
                Some(Host::Ipv4(addr)) => addr.is_loopback(),
                Some(Host::Ipv6(addr)) => addr.is_loopback(),
                None => false,
            };
            if is_loopback && url.port().is_some_and(|port| port != 0) {
                return Ok(());
            }
            Err(IdentifierError::new(
                value,
                "http:// redirect URIs must use loopback host and explicit non-zero port",
            ))
        }
        _ => Err(IdentifierError::new(
            value,
            "must use https:// or local loopback http://",
        )),
    }
}

pub(super) fn validate_local_file_path(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("file://")
    {
        return Err(IdentifierError::new(
            value,
            "must be a local filesystem path, not a URL",
        ));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_resource_uri(value: &str) -> Result<(), IdentifierError> {
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(IdentifierError::new(
            value,
            "must be an absolute server-owned resource URI",
        ));
    };
    validate_uri_scheme(scheme)?;
    if rest.is_empty() || rest.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must include a non-empty path and no whitespace/control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_uri_template(value: &str) -> Result<(), IdentifierError> {
    validate_resource_uri(value)?;
    let parts = parse_simple_resource_uri_template(value)?;
    if !parts
        .iter()
        .any(|part| matches!(part, ResourceUriTemplatePart::Variable(_)))
    {
        return Err(IdentifierError::new(
            value,
            "must include at least one URI-template variable",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResourceUriTemplatePart<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

pub(super) fn parse_simple_resource_uri_template(
    value: &str,
) -> Result<Vec<ResourceUriTemplatePart<'_>>, IdentifierError> {
    let mut parts = Vec::new();
    let mut remaining = value;
    let mut last_was_variable = false;
    while !remaining.is_empty() {
        let next_open = remaining.find('{');
        let next_close = remaining.find('}');
        match (next_open, next_close) {
            (None, None) => {
                parts.push(ResourceUriTemplatePart::Literal(remaining));
                break;
            }
            (None, Some(_)) => {
                return Err(IdentifierError::new(
                    value,
                    "must use balanced simple {variable} expressions",
                ));
            }
            (Some(open), Some(close)) if close < open => {
                return Err(IdentifierError::new(
                    value,
                    "must use balanced simple {variable} expressions",
                ));
            }
            (Some(open), _) if open > 0 => {
                let (literal, rest) = remaining.split_at(open);
                parts.push(ResourceUriTemplatePart::Literal(literal));
                remaining = rest;
                last_was_variable = false;
            }
            (Some(_), _) => {
                let close = remaining[1..]
                    .find('}')
                    .map(|index| index + 1)
                    .ok_or_else(|| {
                        IdentifierError::new(
                            value,
                            "must use balanced simple {variable} expressions",
                        )
                    })?;
                let variable = &remaining[1..close];
                if last_was_variable {
                    return Err(IdentifierError::new(
                        value,
                        "must separate URI-template variables with literal text",
                    ));
                }
                validate_path_id(variable).map_err(|_| {
                    IdentifierError::new(
                        value,
                        "template variables must be simple lowercase identifiers",
                    )
                })?;
                parts.push(ResourceUriTemplatePart::Variable(variable));
                remaining = &remaining[close + 1..];
                last_was_variable = true;
            }
        }
    }
    Ok(parts)
}

pub(super) fn resource_uri_template_matches(template: &str, uri: &str) -> bool {
    let Ok(parts) = parse_simple_resource_uri_template(template) else {
        return false;
    };
    let mut remaining = uri;
    for (index, part) in parts.iter().enumerate() {
        match part {
            ResourceUriTemplatePart::Literal(literal) => {
                let Some(next) = remaining.strip_prefix(literal) else {
                    return false;
                };
                remaining = next;
            }
            ResourceUriTemplatePart::Variable(_) => {
                let next_literal = parts[index + 1..].iter().find_map(|part| match part {
                    ResourceUriTemplatePart::Literal(literal) => Some(*literal),
                    ResourceUriTemplatePart::Variable(_) => None,
                });
                let value = if let Some(next_literal) = next_literal {
                    let Some(end) = remaining.find(next_literal) else {
                        return false;
                    };
                    let value = &remaining[..end];
                    remaining = &remaining[end..];
                    value
                } else {
                    let value = remaining;
                    remaining = "";
                    value
                };
                if value.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
                    return false;
                }
            }
        }
    }
    remaining.is_empty()
}
