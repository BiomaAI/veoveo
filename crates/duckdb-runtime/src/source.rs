use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use reqwest::{Url, blocking::Client, header};
use tempfile::{Builder, TempDir};

#[derive(Debug, Clone)]
pub struct HttpsSourcePolicy {
    allowed_hosts: BTreeSet<String>,
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub max_bytes: u64,
    pub max_redirects: usize,
}

impl HttpsSourcePolicy {
    pub fn deny_network() -> Self {
        Self::new(std::iter::empty::<String>())
    }

    pub fn new(hosts: impl IntoIterator<Item = String>) -> Self {
        Self {
            allowed_hosts: hosts
                .into_iter()
                .map(|host| host.trim().to_ascii_lowercase())
                .filter(|host| !host.is_empty())
                .collect(),
            connect_timeout: Duration::from_secs(5),
            total_timeout: Duration::from_secs(60),
            max_bytes: 256 * 1024 * 1024,
            max_redirects: 5,
        }
    }

    pub fn allowed_hosts(&self) -> impl Iterator<Item = &str> {
        self.allowed_hosts.iter().map(String::as_str)
    }

    fn resolve_public(&self, url: &Url) -> Result<(String, SocketAddr)> {
        if url.scheme() != "https" {
            bail!("source URI must use https");
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("source URI must not contain credentials");
        }
        let host = url
            .host_str()
            .context("source URI must contain a host")?
            .to_ascii_lowercase();
        if !self.allowed_hosts.contains(&host) {
            bail!("source host `{host}` is not allowed");
        }
        let port = url
            .port_or_known_default()
            .context("source URI has no port")?;
        let addresses = (host.as_str(), port)
            .to_socket_addrs()
            .with_context(|| format!("resolving source host `{host}`"))?
            .collect::<Vec<_>>();
        if addresses.iter().any(|address| !is_public_ip(address.ip())) {
            bail!("source host `{host}` resolves to a private or reserved address");
        }
        let address = addresses
            .into_iter()
            .next()
            .with_context(|| format!("source host `{host}` has no public address"))?;
        Ok((host, address))
    }
}

pub struct AuthorizedArtifact<'a> {
    /// Bytes returned only after the artifact plane authorized the caller.
    pub bytes: &'a [u8],
    pub filename: &'a str,
}

pub struct RequestWorkspace {
    _root: TempDir,
    request: PathBuf,
    spill: PathBuf,
}

impl RequestWorkspace {
    pub fn new(prefix: &str) -> Result<Self> {
        let root = Builder::new()
            .prefix(prefix)
            .tempdir()
            .context("creating DuckDB request workspace")?;
        let request = root.path().join("request");
        let spill = root.path().join("spill");
        std::fs::create_dir_all(&request)?;
        std::fs::create_dir_all(&spill)?;
        Ok(Self {
            _root: root,
            request,
            spill,
        })
    }

    pub fn request_dir(&self) -> &Path {
        &self.request
    }

    pub fn spill_dir(&self) -> &Path {
        &self.spill
    }

    pub fn materialize_inline(
        &self,
        filename: &str,
        bytes: &[u8],
        max_bytes: u64,
    ) -> Result<PathBuf> {
        self.write_bounded(filename, bytes, max_bytes, "inline source")
    }

    pub fn materialize_artifact(
        &self,
        artifact: AuthorizedArtifact<'_>,
        max_bytes: u64,
    ) -> Result<PathBuf> {
        self.write_bounded(
            artifact.filename,
            artifact.bytes,
            max_bytes,
            "artifact source",
        )
    }

    fn write_bounded(
        &self,
        filename: &str,
        bytes: &[u8],
        max_bytes: u64,
        label: &str,
    ) -> Result<PathBuf> {
        validate_filename(filename)?;
        if bytes.len() as u64 > max_bytes {
            bail!("{label} exceeds the {max_bytes} byte limit");
        }
        let path = self.request.join(filename);
        std::fs::write(&path, bytes).with_context(|| format!("writing {label}"))?;
        Ok(path)
    }

    pub fn fetch_https(
        &self,
        uri: &str,
        filename: &str,
        policy: &HttpsSourcePolicy,
    ) -> Result<PathBuf> {
        validate_filename(filename)?;
        let destination = self.request.join(filename);
        let result = fetch_https_to(uri, &destination, policy);
        if result.is_err() {
            let _ = std::fs::remove_file(&destination);
        }
        result.map(|()| destination)
    }
}

/// Materialize bytes that were already authorized by the artifact plane into
/// an existing request directory.
pub fn materialize_authorized_artifact(
    request_dir: &Path,
    artifact: AuthorizedArtifact<'_>,
    max_bytes: u64,
) -> Result<PathBuf> {
    validate_filename(artifact.filename)?;
    if artifact.bytes.len() as u64 > max_bytes {
        bail!("artifact source exceeds the {max_bytes} byte limit");
    }
    std::fs::create_dir_all(request_dir)?;
    let path = request_dir.join(artifact.filename);
    std::fs::write(&path, artifact.bytes).context("writing authorized artifact source")?;
    Ok(path)
}

/// Fetch a governed HTTPS source into an existing request directory.
pub fn materialize_https_source(
    request_dir: &Path,
    uri: &str,
    filename: &str,
    policy: &HttpsSourcePolicy,
) -> Result<PathBuf> {
    validate_filename(filename)?;
    std::fs::create_dir_all(request_dir)?;
    let destination = request_dir.join(filename);
    let result = fetch_https_to(uri, &destination, policy);
    if result.is_err() {
        let _ = std::fs::remove_file(&destination);
    }
    result.map(|()| destination)
}

fn fetch_https_to(uri: &str, destination: &Path, policy: &HttpsSourcePolicy) -> Result<()> {
    let mut url = Url::parse(uri).with_context(|| format!("invalid source URI `{uri}`"))?;
    for redirect in 0..=policy.max_redirects {
        let (host, address) = policy.resolve_public(&url)?;
        // Pin the request to the address that passed validation. TLS still
        // authenticates the original hostname and every redirect is rebuilt.
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(policy.connect_timeout)
            .timeout(policy.total_timeout)
            .resolve(&host, address)
            .user_agent("veoveo-duckdb-runtime")
            .build()
            .context("building governed source client")?;
        let mut response = client
            .get(url.clone())
            .send()
            .with_context(|| format!("fetching source `{url}`"))?;
        if response.status().is_redirection() {
            if redirect == policy.max_redirects {
                bail!("source exceeded the redirect limit");
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .context("source redirect omitted Location")?
                .to_str()
                .context("source redirect Location is not valid text")?;
            url = url.join(location).context("invalid source redirect URI")?;
            continue;
        }
        if !response.status().is_success() {
            bail!("source returned HTTP {}", response.status());
        }
        if response
            .content_length()
            .is_some_and(|length| length > policy.max_bytes)
        {
            bail!("source exceeds the {} byte limit", policy.max_bytes);
        }

        let mut file = File::create(destination)
            .with_context(|| format!("creating {}", destination.display()))?;
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = response
                .read(&mut buffer)
                .context("streaming source response")?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > policy.max_bytes {
                bail!("source exceeds the {} byte limit", policy.max_bytes);
            }
            file.write_all(&buffer[..read])
                .context("writing source into request workspace")?;
        }
        file.sync_data().context("syncing materialized source")?;
        return Ok(());
    }
    unreachable!("redirect loop returns or fails")
}

fn validate_filename(filename: &str) -> Result<()> {
    let path = Path::new(filename);
    if filename.is_empty()
        || filename.contains('\0')
        || path.components().count() != 1
        || path.file_name().and_then(|name| name.to_str()) != Some(filename)
    {
        bail!("request-local filename must be one safe path component");
    }
    Ok(())
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0)
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_special_addresses_are_rejected() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(
                !is_public_ip(address.parse().unwrap()),
                "accepted {address}"
            );
        }
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn workspace_rejects_path_traversal_and_enforces_size() {
        let workspace = RequestWorkspace::new("veoveo-source-test-").unwrap();
        assert!(workspace.materialize_inline("../secret", b"x", 10).is_err());
        assert!(
            workspace
                .materialize_inline("large.csv", b"123", 2)
                .is_err()
        );
        let path = workspace
            .materialize_inline("input.csv", b"a\n1\n", 10)
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"a\n1\n");
    }

    #[test]
    fn network_is_denied_without_an_explicit_host() {
        let workspace = RequestWorkspace::new("veoveo-source-test-").unwrap();
        let error = workspace
            .fetch_https(
                "https://example.com/source.csv",
                "source.csv",
                &HttpsSourcePolicy::deny_network(),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("not allowed"));
    }

    #[test]
    fn loopback_is_rejected_even_when_allowlisted() {
        let policy = HttpsSourcePolicy::new(["127.0.0.1".to_string()]);
        let url = Url::parse("https://127.0.0.1/source.csv").unwrap();
        assert!(policy.resolve_public(&url).is_err());
    }

    #[test]
    fn authorized_artifact_is_bounded_and_request_local() {
        let workspace = RequestWorkspace::new("veoveo-artifact-test-").unwrap();
        let path = workspace
            .materialize_artifact(
                AuthorizedArtifact {
                    bytes: b"PAR1",
                    filename: "artifact.parquet",
                },
                4,
            )
            .unwrap();
        assert!(path.starts_with(workspace.request_dir()));
        assert_eq!(std::fs::read(path).unwrap(), b"PAR1");
    }
}
