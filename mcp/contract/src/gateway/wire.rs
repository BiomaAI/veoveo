use url::{Host, Url};
use veoveo_types::identifier_syntax::validate_token_text;

use super::*;

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

pub(super) fn validate_principal_display_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() || value.trim() != value {
        return Err(IdentifierError::new(
            value,
            "must be non-empty without leading or trailing whitespace",
        ));
    }
    if value.len() > 256 {
        return Err(IdentifierError::new(
            value,
            "must be at most 256 UTF-8 bytes",
        ));
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

pub(super) fn validate_oauth_endpoint_url(value: &str) -> Result<(), IdentifierError> {
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
        "https" if url.host().is_some() => Ok(()),
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
                "http:// OAuth endpoints must use a loopback host and explicit non-zero port",
            ))
        }
        _ => Err(IdentifierError::new(
            value,
            "must use https:// or local loopback http://",
        )),
    }
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
