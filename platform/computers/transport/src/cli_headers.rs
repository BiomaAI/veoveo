//! Stock adapter framing; only the Computers ledger can authenticate the token.
use crate::{Result, TransportError};
use axum::http::{HeaderMap, HeaderValue, header};

#[derive(Clone, Copy)]
pub enum CliCredentialFraming {
    Stock,
    Internal,
}

/// Return one sensitive Bearer header. No cookie authority crosses this boundary.
pub fn cli_authorization(
    headers: &HeaderMap,
    framing: CliCredentialFraming,
    query: Option<&str>,
) -> Result<HeaderValue> {
    if query.is_some()
        || headers.contains_key(header::ORIGIN)
        || headers.contains_key(header::SEC_WEBSOCKET_PROTOCOL)
        || headers.contains_key(header::SEC_WEBSOCKET_EXTENSIONS)
    {
        return Err(TransportError::Protocol);
    }
    let token = match framing {
        CliCredentialFraming::Internal => {
            if headers.contains_key(header::COOKIE)
                || headers.contains_key("cf-access-token")
                || headers.contains_key("cf-access-jwt-assertion")
            {
                return Err(TransportError::Protocol);
            }
            single(headers, "authorization")?
                .strip_prefix("Bearer ")
                .ok_or(TransportError::Protocol)?
        }
        CliCredentialFraming::Stock => {
            if headers.contains_key(header::AUTHORIZATION) {
                return Err(TransportError::Protocol);
            }
            let token = single(headers, "cf-access-token")?;
            if headers.contains_key("cf-access-jwt-assertion")
                && single(headers, "cf-access-jwt-assertion")? != token
            {
                return Err(TransportError::Protocol);
            }
            if headers.contains_key(header::COOKIE)
                && single(headers, "cookie")?.strip_prefix("CF_Authorization=") != Some(token)
            {
                return Err(TransportError::Protocol);
            }
            token
        }
    };
    if !valid_token(token) {
        return Err(TransportError::Protocol);
    }
    let mut value =
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| TransportError::Protocol)?;
    value.set_sensitive(true);
    Ok(value)
}
fn single<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str> {
    let mut values = headers.get_all(name).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => value.to_str().map_err(|_| TransportError::Protocol),
        _ => Err(TransportError::Protocol),
    }
}
fn valid_token(token: &str) -> bool {
    if token.len() != 107 {
        return false;
    }
    let Some((id, secret)) = token.strip_prefix("vcli1.").and_then(|s| s.split_once('.')) else {
        return false;
    };
    uuid::Uuid::parse_str(id).is_ok_and(|value| !value.is_nil() && value.to_string() == id)
        && secret.len() == 64
        && secret
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stock_redundancy_is_consistent_and_browser_authority_is_rejected() {
        let token = format!("vcli1.{}.{}", uuid::Uuid::new_v4(), "a".repeat(64));
        let mut stock = HeaderMap::new();
        stock.insert("cf-access-token", token.parse().unwrap());
        stock.insert("cf-access-jwt-assertion", token.parse().unwrap());
        stock.insert(
            header::COOKIE,
            format!("CF_Authorization={token}").parse().unwrap(),
        );
        let admitted = cli_authorization(&stock, CliCredentialFraming::Stock, None).unwrap();
        assert!(admitted.is_sensitive());
        assert!(cli_authorization(&stock, CliCredentialFraming::Internal, None).is_err());
        for (name, value) in [
            ("origin", "https://veoveo.test"),
            ("authorization", "Bearer ordinary"),
            ("cookie", "console=browser-authority"),
            ("cf-access-jwt-assertion", "different"),
            ("sec-websocket-protocol", "unexpected"),
            ("sec-websocket-extensions", "permessage-deflate"),
        ] {
            let mut headers = stock.clone();
            headers.insert(
                axum::http::HeaderName::from_static(name),
                value.parse().unwrap(),
            );
            assert!(cli_authorization(&headers, CliCredentialFraming::Stock, None).is_err());
        }
        stock.append("cf-access-token", token.parse().unwrap());
        assert!(cli_authorization(&stock, CliCredentialFraming::Stock, None).is_err());
        let mut internal = HeaderMap::new();
        internal.insert(header::AUTHORIZATION, admitted);
        assert!(cli_authorization(&internal, CliCredentialFraming::Internal, None).is_ok());
        assert!(cli_authorization(&internal, CliCredentialFraming::Internal, Some("")).is_err());
        internal.append(header::AUTHORIZATION, "Bearer other".parse().unwrap());
        assert!(cli_authorization(&internal, CliCredentialFraming::Internal, None).is_err());
    }
}
