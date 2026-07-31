use std::{fs, path::Path};

use anyhow::{Context, bail};

/// Installation-owned roots added to the platform verifier used by Console
/// outbound clients. The standard trust store remains active.
#[derive(Default)]
pub(crate) struct OutboundTrust {
    extra_roots: Vec<reqwest::Certificate>,
}

impl OutboundTrust {
    pub(crate) fn from_bundle_path(path: Option<&Path>) -> anyhow::Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let pem = fs::read(path)
            .with_context(|| format!("reading Console outbound CA bundle {}", path.display()))?;
        Self::from_pem_bundle(&pem)
            .with_context(|| format!("loading Console outbound CA bundle {}", path.display()))
    }

    fn from_pem_bundle(pem: &[u8]) -> anyhow::Result<Self> {
        let extra_roots = reqwest::Certificate::from_pem_bundle(pem)
            .context("Console outbound CA bundle must contain valid PEM certificates")?;
        if extra_roots.is_empty() {
            bail!("Console outbound CA bundle contains no certificates");
        }
        Ok(Self { extra_roots })
    }

    #[cfg(test)]
    pub(crate) fn for_test_pem_bundle(pem: &[u8]) -> anyhow::Result<Self> {
        Self::from_pem_bundle(pem)
    }

    pub(crate) fn client_builder(&self) -> reqwest::ClientBuilder {
        reqwest::Client::builder().tls_certs_merge(self.extra_roots.iter().cloned())
    }
}

#[cfg(test)]
mod tests {
    use rcgen::generate_simple_self_signed;

    use super::*;

    #[test]
    fn outbound_ca_bundle_accepts_multiple_pem_roots() {
        let first = generate_simple_self_signed(vec!["first.internal".to_owned()]).unwrap();
        let second = generate_simple_self_signed(vec!["second.internal".to_owned()]).unwrap();
        let bundle = format!("{}\n{}", first.cert.pem(), second.cert.pem());
        let trust = OutboundTrust::from_pem_bundle(bundle.as_bytes()).unwrap();
        assert_eq!(trust.extra_roots.len(), 2);
        trust.client_builder().build().unwrap();
    }

    #[test]
    fn outbound_ca_bundle_rejects_empty_and_invalid_material() {
        for invalid in [b"".as_slice(), b"not a PEM certificate".as_slice()] {
            assert!(OutboundTrust::from_pem_bundle(invalid).is_err());
        }
    }
}
