//! Installation-owned client assertions; the public fixture key stays on loopback.
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use url::Host;
use url::Url;
use veoveo_oauth_client::{ClientAssertionKeyId, ClientAssertionSigner};

pub(super) struct ClientSigningKey {
    pub signer: ClientAssertionSigner,
}

pub(super) fn resolve(
    token_url: &str,
    path: Option<&Path>,
    id: Option<&str>,
) -> Result<ClientSigningKey> {
    let url = Url::parse(token_url).context("invalid token endpoint")?;
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
    {
        bail!(
            "client assertions require HTTPS or loopback HTTP without URL credentials or fragments"
        );
    }
    match (path, id) {
        (None, None) if loopback => Ok(ClientSigningKey {
            signer: ClientAssertionSigner::from_rsa_key(
                ClientAssertionKeyId::parse(super::tokens::CONFORMANCE_KEY_ID)?,
                super::tokens::conformance_encoding_key()?,
            ),
        }),
        (Some(path), Some(id))
            if !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) =>
        {
            let mut file =
                File::open(path).context("cannot open the service client private key")?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 16 * 1024 {
                bail!("service client private key must be a regular PEM file of at most 16 KiB");
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    bail!("service client private key must be readable only by its owner");
                }
            }
            let mut bytes = Vec::new();
            file.by_ref().take(16 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 16 * 1024 {
                bail!("service client private key exceeds 16 KiB");
            }
            let key = ClientAssertionSigner::from_rsa_pem(ClientAssertionKeyId::parse(id)?, &bytes);
            bytes.fill(0);
            Ok(ClientSigningKey {
                signer: key.context("invalid RSA service client private key")?,
            })
        }
        _ => bail!(
            "provide both --client-key-file and --client-key-id for installation authentication"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn installed_key_loading_checks_private_file_and_signs_with_selected_identity() {
        use std::{fs, io::Write, os::unix::fs::OpenOptionsExt};
        let path = std::env::temp_dir().join(format!("veoveo-client-key-{}", uuid::Uuid::new_v4()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        let endpoint = "https://installation.example/oauth/token";
        assert!(resolve(endpoint, Some(&path), Some("client-key")).is_err());
        file.write_all(super::super::tokens::conformance_private_key_pem().as_bytes())
            .unwrap();
        let signing = resolve(endpoint, Some(&path), Some("client-key")).unwrap();
        let token = signing
            .signer
            .diagnostic_assertion(&"client".parse().unwrap(), endpoint, 1, 2, "diagnostic")
            .unwrap();
        assert_eq!(
            jsonwebtoken::decode_header(token.expose_secret())
                .unwrap()
                .kid
                .as_deref(),
            Some("client-key")
        );
        assert_eq!(
            signing.signer.public_jwk().unwrap().algorithm,
            super::super::tokens::conformance_jwks().unwrap().keys[0].algorithm
        );
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(resolve(endpoint, Some(&path), Some("client-key")).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, b"invalid PEM").unwrap();
        assert!(resolve(endpoint, Some(&path), Some("client-key")).is_err());
        fs::write(&path, vec![b'x'; 16 * 1024 + 1]).unwrap();
        assert!(resolve(endpoint, Some(&path), Some("client-key")).is_err());
        assert!(resolve(endpoint, Some(&path), Some("bad/key")).is_err());
    }

    #[test]
    fn public_fixture_signing_is_confined_to_loopback() {
        for url in [
            "http://127.0.0.1:8788/oauth/token",
            "http://localhost:8788/oauth/token",
            "https://[::1]/oauth/token",
        ] {
            assert!(resolve(url, None, None).is_ok(), "{url}");
        }
        for url in [
            "https://veoveo.bioma.ai/oauth/token",
            "http://veoveo.bioma.ai/oauth/token",
            "https://localhost.example/oauth/token",
            "http://127.0.0.1@outside.example/token",
            "ftp://localhost/token",
        ] {
            assert!(resolve(url, None, None).is_err(), "{url}");
        }
        assert!(resolve("http://localhost/token", None, Some("installed")).is_err());
    }
}
