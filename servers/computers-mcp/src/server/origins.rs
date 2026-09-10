//! Exact browser origins. Canonical HTTPS is required outside explicit loopback fixtures.
use crate::ApplicationError;
use axum::http::{HeaderMap, header::ORIGIN};
use std::collections::BTreeSet;

#[derive(Clone)]
pub struct BrowserOrigins(BTreeSet<String>);
impl BrowserOrigins {
    pub fn new(origins: Vec<String>) -> Result<Self, ApplicationError> {
        if origins.is_empty() || origins.len() > 64 {
            return Err(ApplicationError::Configuration);
        }
        let mut admitted = BTreeSet::new();
        for origin in origins {
            let url = url::Url::parse(&origin).map_err(|_| ApplicationError::Configuration)?;
            let loopback = match url.host() {
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                Some(url::Host::Domain(name)) => name == "localhost",
                None => false,
            };
            if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.origin().ascii_serialization() != origin
                || !admitted.insert(origin)
            {
                return Err(ApplicationError::Configuration);
            }
        }
        Ok(Self(admitted))
    }
    pub(crate) fn permits(&self, headers: &HeaderMap) -> bool {
        let mut origins = headers.get_all(ORIGIN).iter();
        let Some(origin) = origins.next().and_then(|h| h.to_str().ok()) else {
            return false;
        };
        origins.next().is_none() && self.0.contains(origin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_origins_reject_opaque_ambiguous_and_insecure_authority() {
        for bad in [
            "null",
            "http://example.com",
            "https://example.com/",
            "https://a@b",
            "https://a?b",
            "https://a#b",
            "https://EXAMPLE.com",
            "https://example.com:443",
            "http://localhost.evil",
            "file:///tmp",
        ] {
            assert!(BrowserOrigins::new(vec![bad.into()]).is_err(), "{bad}");
        }
        let origins = BrowserOrigins::new(vec![
            "https://veoveo.bioma.ai".into(),
            "http://127.0.0.1:1234".into(),
            "http://[::1]:1234".into(),
        ])
        .unwrap();
        let mut headers = HeaderMap::new();
        assert!(!origins.permits(&headers));
        headers.insert(ORIGIN, "https://veoveo.bioma.ai".parse().unwrap());
        assert!(origins.permits(&headers));
        headers.append(ORIGIN, "https://veoveo.bioma.ai".parse().unwrap());
        assert!(!origins.permits(&headers));
        assert!(BrowserOrigins::new(vec!["https://example.com".into(); 2]).is_err());
    }
}
