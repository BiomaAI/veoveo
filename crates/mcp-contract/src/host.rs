use std::net::Ipv6Addr;

use crate::PublicDeployment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAuthority {
    host: String,
    port: Option<u16>,
}

impl HostAuthority {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }
}

pub fn parse_request_host_authority(value: &str) -> Option<HostAuthority> {
    if value.is_empty() || contains_invalid_authority_char(value) {
        return None;
    }
    if let Some(rest) = value.strip_prefix('[') {
        let (host, suffix) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        if suffix.is_empty() {
            return Some(normalize_authority(host, None));
        }
        let port = suffix.strip_prefix(':')?;
        return Some(normalize_authority(host, Some(parse_port(port)?)));
    }

    match value.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && !host.contains(':') => {
            Some(normalize_authority(host, Some(parse_port(port)?)))
        }
        Some(_) => None,
        None => Some(normalize_authority(value, None)),
    }
}

pub fn parse_allowed_host_authority(value: &str) -> Option<HostAuthority> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(authority) = parse_request_host_authority(value) {
        return Some(authority);
    }
    if value.parse::<Ipv6Addr>().is_ok() {
        return Some(normalize_authority(value, None));
    }
    None
}

pub fn host_authority_is_allowed(authority: &HostAuthority, allowed_hosts: &[String]) -> bool {
    allowed_hosts
        .iter()
        .filter_map(|allowed| parse_allowed_host_authority(allowed))
        .any(|allowed| {
            allowed.host == authority.host
                && match allowed.port {
                    Some(port) => authority.port == Some(port),
                    None => true,
                }
        })
}

pub fn public_allowed_hosts(
    deployment: &PublicDeployment,
    allow_loopback_hosts: bool,
) -> Vec<String> {
    let mut hosts = vec![deployment.host_authority().to_string()];
    if allow_loopback_hosts {
        hosts.extend([
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
        ]);
    }
    hosts
}

fn normalize_authority(host: &str, port: Option<u16>) -> HostAuthority {
    HostAuthority {
        host: host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase(),
        port,
    }
}

fn parse_port(value: &str) -> Option<u16> {
    if value.is_empty() {
        return None;
    }
    value.parse::<u16>().ok()
}

fn contains_invalid_authority_char(value: &str) -> bool {
    value.chars().any(char::is_whitespace) || value.contains(['/', '\\', '?', '#', '@'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_authority_parsing_is_strict() {
        assert_eq!(
            parse_request_host_authority("127.0.0.1:8788"),
            Some(HostAuthority {
                host: "127.0.0.1".to_string(),
                port: Some(8788),
            })
        );
        assert_eq!(
            parse_request_host_authority("veoveo.bioma.ai"),
            Some(HostAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: None,
            })
        );
        assert_eq!(
            parse_request_host_authority("[::1]:8788"),
            Some(HostAuthority {
                host: "::1".to_string(),
                port: Some(8788),
            })
        );
        assert_eq!(
            parse_request_host_authority("[::1]"),
            Some(HostAuthority {
                host: "::1".to_string(),
                port: None,
            })
        );
        assert_eq!(parse_request_host_authority(""), None);
        assert_eq!(parse_request_host_authority("127.0.0.1:not-a-port"), None);
        assert_eq!(parse_request_host_authority("127.0.0.1:"), None);
        assert_eq!(parse_request_host_authority("127.0.0.1:999999"), None);
        assert_eq!(parse_request_host_authority("::1"), None);
        assert_eq!(parse_request_host_authority("[::1"), None);
        assert_eq!(parse_request_host_authority("[::1]:not-a-port"), None);
        assert_eq!(parse_request_host_authority("veoveo.bioma.ai/path"), None);
        assert_eq!(parse_request_host_authority("veoveo.bioma.ai?x=1"), None);
        assert_eq!(parse_request_host_authority("user@veoveo.bioma.ai"), None);
        assert_eq!(parse_request_host_authority("veoveo .bioma.ai"), None);
    }

    #[test]
    fn allowed_authority_with_port_requires_same_port() {
        let allowed = vec!["veoveo.bioma.ai:8443".to_string()];

        assert!(host_authority_is_allowed(
            &HostAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: Some(8443),
            },
            &allowed
        ));
        assert!(!host_authority_is_allowed(
            &HostAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: Some(443),
            },
            &allowed
        ));
        assert!(!host_authority_is_allowed(
            &HostAuthority {
                host: "veoveo.bioma.ai".to_string(),
                port: None,
            },
            &allowed
        ));
    }

    #[test]
    fn allowed_authority_without_port_allows_any_port() {
        let allowed = vec!["127.0.0.1".to_string()];

        assert!(host_authority_is_allowed(
            &HostAuthority {
                host: "127.0.0.1".to_string(),
                port: Some(18799),
            },
            &allowed
        ));
        assert!(host_authority_is_allowed(
            &HostAuthority {
                host: "127.0.0.1".to_string(),
                port: None,
            },
            &allowed
        ));
    }

    #[test]
    fn allowed_ipv6_literal_can_be_configured_without_brackets() {
        assert_eq!(
            parse_allowed_host_authority("::1"),
            Some(HostAuthority {
                host: "::1".to_string(),
                port: None,
            })
        );
    }

    #[test]
    fn public_host_allowlist_uses_loopback_only_when_explicit() {
        let deployment = PublicDeployment::new("https://veoveo.bioma.ai").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, false),
            vec!["veoveo.bioma.ai"]
        );
        assert_eq!(
            public_allowed_hosts(&deployment, true),
            vec!["veoveo.bioma.ai", "localhost", "127.0.0.1", "::1"]
        );
    }
}
