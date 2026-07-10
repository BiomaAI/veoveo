use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use url::Url;

#[derive(Clone)]
pub(crate) struct Config {
    bind: SocketAddr,
    public_base_url: Url,
    gateway_url: Url,
    oauth_client_id: String,
    oauth_resource: Url,
    admin_profile: String,
    session_key: [u8; 32],
    asset_dir: PathBuf,
}

impl Config {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let bind = format!("0.0.0.0:{}", env_or("PORT", "8786"))
            .parse()
            .context("PORT must be a valid TCP port")?;
        let public_base_url = base_url("PUBLIC_BASE_URL")?;
        let gateway_url = base_url("VEOVEO_GATEWAY_URL")?;
        let oauth_client_id = required("VEOVEO_CONSOLE_OAUTH_CLIENT_ID")?;
        validate_identifier("VEOVEO_CONSOLE_OAUTH_CLIENT_ID", &oauth_client_id)?;
        let oauth_resource = absolute_url("VEOVEO_CONSOLE_OAUTH_RESOURCE")?;
        let admin_profile = oauth_resource
            .path()
            .strip_prefix("/mcp/")
            .filter(|value| !value.is_empty() && !value.contains('/'))
            .ok_or_else(|| anyhow!("VEOVEO_CONSOLE_OAUTH_RESOURCE must end in /mcp/<profile>"))?
            .to_owned();
        validate_identifier("OAuth resource profile", &admin_profile)?;
        let key_bytes = STANDARD
            .decode(required("VEOVEO_CONSOLE_SESSION_KEY")?)
            .context("VEOVEO_CONSOLE_SESSION_KEY must be canonical base64")?;
        let session_key: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| anyhow!("VEOVEO_CONSOLE_SESSION_KEY must decode to exactly 32 bytes"))?;
        let asset_dir = std::env::var_os("VEOVEO_CONSOLE_ASSET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/app/console"));

        Ok(Self {
            bind,
            public_base_url,
            gateway_url,
            oauth_client_id,
            oauth_resource,
            admin_profile,
            session_key,
            asset_dir,
        })
    }

    pub(crate) const fn bind(&self) -> SocketAddr {
        self.bind
    }
    pub(crate) fn oauth_client_id(&self) -> &str {
        &self.oauth_client_id
    }
    pub(crate) fn oauth_resource(&self) -> &Url {
        &self.oauth_resource
    }
    pub(crate) const fn session_key(&self) -> &[u8; 32] {
        &self.session_key
    }
    pub(crate) fn asset_dir(&self) -> &Path {
        &self.asset_dir
    }

    pub(crate) fn callback_url(&self) -> Url {
        self.public_base_url
            .join("/auth/callback")
            .expect("validated base URL")
    }

    pub(crate) fn authorize_url(&self) -> Url {
        self.public_base_url
            .join("/oauth/authorize")
            .expect("validated base URL")
    }

    pub(crate) fn token_url(&self) -> Url {
        self.gateway_url
            .join("/oauth/token")
            .expect("validated base URL")
    }

    pub(crate) fn revocation_url(&self) -> Url {
        self.gateway_url
            .join("/oauth/revoke")
            .expect("validated base URL")
    }

    pub(crate) fn snapshot_url(&self) -> Url {
        self.admin_url("console/snapshot")
    }

    pub(crate) fn admin_url(&self, path: &str) -> Url {
        debug_assert!(!path.starts_with('/'));
        self.gateway_url
            .join(&format!("/admin/{}/{path}", self.admin_profile))
            .expect("validated profile and typed path")
    }

    pub(crate) fn artifact_download_url(&self, artifact_id: &str) -> Url {
        self.gateway_url
            .join(&format!(
                "/artifacts/{}/{artifact_id}/download",
                self.admin_profile
            ))
            .expect("validated profile and artifact id")
    }

    pub(crate) fn gateway_host(&self) -> String {
        let host = self.public_base_url.host_str().expect("validated URL");
        match self.public_base_url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        }
    }

    pub(crate) fn secure_cookie(&self) -> bool {
        self.public_base_url.scheme() == "https"
    }
}

fn required(key: &'static str) -> anyhow::Result<String> {
    let value = std::env::var(key).with_context(|| format!("missing required env var {key}"))?;
    if value.trim().is_empty() {
        bail!("{key} must not be empty");
    }
    Ok(value)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn absolute_url(key: &'static str) -> anyhow::Result<Url> {
    let url =
        Url::parse(&required(key)?).with_context(|| format!("{key} must be an absolute URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("{key} must be an http(s) URL without query or fragment");
    }
    Ok(url)
}

fn base_url(key: &'static str) -> anyhow::Result<Url> {
    let mut url = absolute_url(key)?;
    if !matches!(url.path(), "" | "/") {
        bail!("{key} must not contain a path");
    }
    url.set_path("/");
    Ok(url)
}

fn validate_identifier(field: &'static str, value: &str) -> anyhow::Result<()> {
    if value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("{field} contains unsupported characters");
    }
    Ok(())
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("bind", &self.bind)
            .field("public_base_url", &self.public_base_url)
            .field("gateway_url", &self.gateway_url)
            .field("oauth_client_id", &self.oauth_client_id)
            .field("oauth_resource", &self.oauth_resource)
            .field("admin_profile", &self.admin_profile)
            .field("session_key", &"[REDACTED]")
            .field("asset_dir", &self.asset_dir)
            .finish()
    }
}
