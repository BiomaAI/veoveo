use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header::HOST, uri::Authority},
    middleware::Next,
    response::IntoResponse,
};

pub(super) type AllowedHosts = Arc<Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedAuthority {
    host: String,
    port: Option<u16>,
}

pub(super) async fn validate_host(
    State(allowed_hosts): State<AllowedHosts>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let Some(authority) = request_authority(&request) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if host_is_allowed(&authority, &allowed_hosts) {
        return next.run(request).await;
    }
    tracing::warn!(
        host = authority.host,
        port = authority.port,
        "rejected gateway request for untrusted host"
    );
    StatusCode::MISDIRECTED_REQUEST.into_response()
}

fn request_authority(request: &Request) -> Option<NormalizedAuthority> {
    if let Some(header) = request.headers().get(HOST) {
        return header.to_str().ok().and_then(parse_header_authority);
    }
    let authority = request.uri().authority()?;
    normalize_parsed_authority(authority)
}

fn parse_header_authority(value: &str) -> Option<NormalizedAuthority> {
    if has_malformed_port(value) {
        return None;
    }
    let authority = Authority::try_from(value).ok()?;
    normalize_parsed_authority(&authority)
}

fn parse_allowed_authority(value: &str) -> Option<NormalizedAuthority> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(authority) = Authority::try_from(value) {
        return normalize_parsed_authority(&authority);
    }
    Some(normalize_authority(value, None))
}

fn normalize_parsed_authority(authority: &Authority) -> Option<NormalizedAuthority> {
    let port = match authority.port() {
        Some(_) => Some(authority.port_u16()?),
        None => None,
    };
    Some(normalize_authority(authority.host(), port))
}

fn has_malformed_port(value: &str) -> bool {
    if let Some(rest) = value.strip_prefix('[') {
        let Some((_host, suffix)) = rest.split_once(']') else {
            return true;
        };
        if let Some(port) = suffix.strip_prefix(':') {
            return !valid_port(port);
        }
        return !suffix.is_empty();
    }
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    if host.contains(':') {
        return true;
    }
    !valid_port(port)
}

fn valid_port(value: &str) -> bool {
    !value.is_empty() && value.parse::<u16>().is_ok()
}

fn normalize_authority(host: &str, port: Option<u16>) -> NormalizedAuthority {
    NormalizedAuthority {
        host: host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase(),
        port,
    }
}

fn host_is_allowed(host: &NormalizedAuthority, allowed_hosts: &[String]) -> bool {
    allowed_hosts
        .iter()
        .filter_map(|allowed| parse_allowed_authority(allowed))
        .any(|allowed| {
            allowed.host == host.host
                && match allowed.port {
                    Some(port) => host.port == Some(port),
                    None => true,
                }
        })
}

#[cfg(test)]
mod tests {
    use super::{
        NormalizedAuthority, host_is_allowed, parse_allowed_authority, parse_header_authority,
    };

    #[test]
    fn normalizes_authority_host_values() {
        assert_eq!(
            parse_header_authority("127.0.0.1:8788"),
            Some(NormalizedAuthority {
                host: "127.0.0.1".to_string(),
                port: Some(8788),
            })
        );
        assert_eq!(
            parse_header_authority("veoveo.bioma.ai"),
            Some(NormalizedAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: None,
            })
        );
        assert_eq!(
            parse_header_authority("[::1]:8788"),
            Some(NormalizedAuthority {
                host: "::1".to_string(),
                port: Some(8788),
            })
        );
        assert_eq!(
            parse_header_authority("[::1]"),
            Some(NormalizedAuthority {
                host: "::1".to_string(),
                port: None,
            })
        );
        assert_eq!(parse_header_authority(""), None);
        assert_eq!(parse_header_authority("127.0.0.1:not-a-port"), None);
        assert_eq!(parse_header_authority("127.0.0.1:"), None);
        assert_eq!(parse_header_authority("127.0.0.1:999999"), None);
        assert_eq!(parse_header_authority("::1"), None);
        assert_eq!(parse_header_authority("[::1"), None);
        assert_eq!(parse_header_authority("[::1]:not-a-port"), None);
    }

    #[test]
    fn allowed_authority_with_port_requires_same_port() {
        let allowed = vec!["veoveo.bioma.ai:8443".to_string()];

        assert!(host_is_allowed(
            &NormalizedAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: Some(8443),
            },
            &allowed
        ));
        assert!(!host_is_allowed(
            &NormalizedAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: Some(443),
            },
            &allowed
        ));
        assert!(!host_is_allowed(
            &NormalizedAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: None,
            },
            &allowed
        ));
    }

    #[test]
    fn allowed_authority_without_port_allows_any_port() {
        let allowed = vec!["127.0.0.1".to_string()];

        assert!(host_is_allowed(
            &NormalizedAuthority {
                host: "127.0.0.1".to_string(),
                port: Some(18799),
            },
            &allowed
        ));
        assert!(host_is_allowed(
            &NormalizedAuthority {
                host: "127.0.0.1".to_string(),
                port: None,
            },
            &allowed
        ));
    }

    #[test]
    fn allowed_ipv6_literal_can_be_configured_without_brackets() {
        assert_eq!(
            parse_allowed_authority("::1"),
            Some(NormalizedAuthority {
                host: "::1".to_string(),
                port: None,
            })
        );
    }
}
