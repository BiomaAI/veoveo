use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use url::Url;
use veoveo_mcp_contract::ScopeName;

#[derive(Clone)]
pub(crate) enum RerunMapProvider {
    OpenStreetMap,
    Mapbox {
        access_token: Option<String>,
        diagnostic: Option<&'static str>,
    },
}

impl RerunMapProvider {
    pub(crate) const fn connect_origin(&self) -> &'static str {
        match self {
            Self::OpenStreetMap => "https://tile.openstreetmap.org",
            Self::Mapbox { .. } => "https://api.mapbox.com",
        }
    }
}

impl std::fmt::Debug for RerunMapProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenStreetMap => formatter.write_str("OpenStreetMap"),
            Self::Mapbox { diagnostic, .. } => formatter
                .debug_struct("Mapbox")
                .field("access_token", &"[REDACTED]")
                .field("diagnostic", diagnostic)
                .finish(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Config {
    bind: SocketAddr,
    public_base_url: Url,
    gateway_url: Url,
    oauth_client_id: String,
    oauth_resource: Url,
    mcp_transport_url: Url,
    oauth_scopes: BTreeSet<ScopeName>,
    admin_profile: String,
    outbound_ca_bundle: Option<PathBuf>,
    rerun_map_provider: RerunMapProvider,
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
        let configured_mcp_transport = optional("VEOVEO_CONSOLE_MCP_TRANSPORT_URL")?;
        let mcp_transport_url =
            resolve_mcp_transport_url(&oauth_resource, configured_mcp_transport.as_deref())?;
        let oauth_scopes = parse_oauth_scopes(&required("VEOVEO_CONSOLE_OAUTH_SCOPES")?)?;
        let admin_profile = mcp_profile("VEOVEO_CONSOLE_OAUTH_RESOURCE", &oauth_resource)?;
        validate_identifier("OAuth resource profile", &admin_profile)?;
        let outbound_ca_bundle = optional("VEOVEO_CONSOLE_OUTBOUND_CA_BUNDLE")?
            .map(PathBuf::from)
            .map(|path| {
                if !path.is_absolute() {
                    bail!("VEOVEO_CONSOLE_OUTBOUND_CA_BUNDLE must be an absolute path");
                }
                Ok(path)
            })
            .transpose()?;
        let rerun_map_provider = parse_rerun_map_provider(
            &env_or("VEOVEO_CONSOLE_RERUN_MAP_PROVIDER", "openStreetMap"),
            optional("RERUN_MAPBOX_ACCESS_TOKEN")?,
        )?;
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
            mcp_transport_url,
            oauth_scopes,
            admin_profile,
            outbound_ca_bundle,
            rerun_map_provider,
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
    pub(crate) fn mcp_transport_url(&self) -> &Url {
        &self.mcp_transport_url
    }
    pub(crate) fn oauth_scope(&self) -> String {
        self.oauth_scopes
            .iter()
            .map(ScopeName::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub(crate) fn oauth_scopes(&self) -> &BTreeSet<ScopeName> {
        &self.oauth_scopes
    }
    pub(crate) const fn session_key(&self) -> &[u8; 32] {
        &self.session_key
    }
    pub(crate) fn asset_dir(&self) -> &Path {
        &self.asset_dir
    }
    pub(crate) fn outbound_ca_bundle(&self) -> Option<&Path> {
        self.outbound_ca_bundle.as_deref()
    }
    pub(crate) const fn rerun_map_provider(&self) -> &RerunMapProvider {
        &self.rerun_map_provider
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

    pub(crate) fn cluster_authorization_url(&self) -> Url {
        self.admin_url("console/cluster")
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

    pub(crate) fn recording_playback_url(&self, recording_id: &str) -> Url {
        self.gateway_url
            .join(&format!(
                "/recordings/{}/{recording_id}/playback",
                self.admin_profile
            ))
            .expect("validated profile and recording id")
    }

    pub(crate) fn recording_live_segment_url(&self, recording_id: &str, segment_id: &str) -> Url {
        self.gateway_url
            .join(&format!(
                "/recordings/{}/{recording_id}/segments/{segment_id}/live.rrd-frames",
                self.admin_profile
            ))
            .expect("validated profile and recording/segment ids")
    }

    pub(crate) fn recording_blueprint_url(&self, recording_id: &str, revision: u64) -> Url {
        self.gateway_url
            .join(&format!(
                "/recordings/{}/{recording_id}/blueprints/{revision}/data.rrd",
                self.admin_profile
            ))
            .expect("validated profile and recording/Blueprint ids")
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

    #[cfg(test)]
    pub(crate) fn for_test(gateway_url: Url) -> Self {
        Self {
            bind: "127.0.0.1:0".parse().expect("valid test bind"),
            public_base_url: gateway_url.clone(),
            oauth_client_id: "console".to_owned(),
            oauth_resource: gateway_url.join("/mcp/admin").expect("valid test resource"),
            mcp_transport_url: gateway_url
                .join("/mcp/admin")
                .expect("valid test transport"),
            oauth_scopes: BTreeSet::from([
                ScopeName::new("admin:manage").expect("valid test scope")
            ]),
            admin_profile: "admin".to_owned(),
            outbound_ca_bundle: None,
            rerun_map_provider: RerunMapProvider::OpenStreetMap,
            session_key: [7; 32],
            asset_dir: PathBuf::from("/tmp/veoveo-console-test-assets"),
            gateway_url,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_mcp_transport_url(mut self, transport_url: Url) -> Self {
        self.mcp_transport_url = transport_url;
        self
    }
}

fn required(key: &'static str) -> anyhow::Result<String> {
    let value = std::env::var(key).with_context(|| format!("missing required env var {key}"))?;
    if value.trim().is_empty() {
        bail!("{key} must not be empty");
    }
    Ok(value)
}

fn optional(key: &'static str) -> anyhow::Result<Option<String>> {
    match std::env::var(key) {
        Ok(value) if value.trim().is_empty() => bail!("{key} must not be empty when set"),
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading env var {key}")),
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn parse_rerun_map_provider(
    provider: &str,
    mapbox_access_token: Option<String>,
) -> anyhow::Result<RerunMapProvider> {
    match provider {
        "openStreetMap" => {
            if mapbox_access_token.is_some() {
                bail!(
                    "RERUN_MAPBOX_ACCESS_TOKEN must be absent when VEOVEO_CONSOLE_RERUN_MAP_PROVIDER is openStreetMap"
                );
            }
            Ok(RerunMapProvider::OpenStreetMap)
        }
        "mapbox" => {
            let (access_token, diagnostic) = match mapbox_access_token {
                None => (
                    None,
                    Some("Mapbox is selected, but no installation token is mounted"),
                ),
                Some(token) if validate_mapbox_access_token(&token).is_ok() => (Some(token), None),
                Some(_) => (
                    None,
                    Some(
                        "Mapbox is selected, but the installation token is not a browser-safe public token",
                    ),
                ),
            };
            Ok(RerunMapProvider::Mapbox {
                access_token,
                diagnostic,
            })
        }
        _ => bail!("VEOVEO_CONSOLE_RERUN_MAP_PROVIDER must be one of openStreetMap or mapbox"),
    }
}

fn validate_mapbox_access_token(token: &str) -> anyhow::Result<()> {
    if token.len() < 8
        || token.len() > 4096
        || !token.starts_with("pk.")
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        bail!(
            "RERUN_MAPBOX_ACCESS_TOKEN must be a browser-safe Mapbox public token beginning with pk."
        );
    }
    Ok(())
}

fn absolute_url(key: &'static str) -> anyhow::Result<Url> {
    absolute_url_value(key, &required(key)?)
}

fn absolute_url_value(key: &'static str, value: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value).with_context(|| format!("{key} must be an absolute URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("{key} must be an http(s) URL without credentials, query, or fragment");
    }
    Ok(url)
}

fn mcp_profile(key: &'static str, url: &Url) -> anyhow::Result<String> {
    url.path()
        .strip_prefix("/mcp/")
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{key} must end in /mcp/<profile>"))
}

fn resolve_mcp_transport_url(
    oauth_resource: &Url,
    configured_transport: Option<&str>,
) -> anyhow::Result<Url> {
    let transport = configured_transport.map_or_else(
        || Ok(oauth_resource.clone()),
        |value| absolute_url_value("VEOVEO_CONSOLE_MCP_TRANSPORT_URL", value),
    )?;
    let oauth_profile = mcp_profile("VEOVEO_CONSOLE_OAUTH_RESOURCE", oauth_resource)?;
    let transport_profile = mcp_profile("VEOVEO_CONSOLE_MCP_TRANSPORT_URL", &transport)?;
    if transport_profile != oauth_profile {
        bail!(
            "VEOVEO_CONSOLE_MCP_TRANSPORT_URL profile `{transport_profile}` must match VEOVEO_CONSOLE_OAUTH_RESOURCE profile `{oauth_profile}`"
        );
    }
    Ok(transport)
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
            .field("mcp_transport_url", &self.mcp_transport_url)
            .field("oauth_scopes", &self.oauth_scopes)
            .field("admin_profile", &self.admin_profile)
            .field("outbound_ca_bundle", &self.outbound_ca_bundle)
            .field("rerun_map_provider", &self.rerun_map_provider)
            .field("session_key", &"[REDACTED]")
            .field("asset_dir", &self.asset_dir)
            .finish()
    }
}

fn parse_oauth_scopes(value: &str) -> anyhow::Result<BTreeSet<ScopeName>> {
    let scopes = value
        .split_ascii_whitespace()
        .map(ScopeName::new)
        .collect::<Result<BTreeSet<_>, _>>()
        .context("VEOVEO_CONSOLE_OAUTH_SCOPES contains an invalid scope")?;
    if scopes.is_empty() {
        bail!("VEOVEO_CONSOLE_OAUTH_SCOPES must contain at least one scope");
    }
    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_oauth_scopes_are_typed_deduplicated_and_stable() {
        let scopes = parse_oauth_scopes("operator:use admin:manage operator:use").unwrap();
        assert_eq!(
            scopes.iter().map(ScopeName::as_str).collect::<Vec<_>>(),
            ["admin:manage", "operator:use"]
        );
        assert!(parse_oauth_scopes(" ").is_err());
    }

    #[test]
    fn console_mcp_transport_requires_an_absolute_http_endpoint() {
        for invalid in [
            "mcp-gateway:8788/mcp/admin",
            "ftp://mcp-gateway/mcp/admin",
            "http://user@mcp-gateway/mcp/admin",
            "http://mcp-gateway/mcp/admin?tenant=one",
            "http://mcp-gateway/mcp/admin#fragment",
        ] {
            assert!(
                absolute_url_value("VEOVEO_CONSOLE_MCP_TRANSPORT_URL", invalid).is_err(),
                "accepted invalid transport URL {invalid}"
            );
        }
        assert!(
            absolute_url_value(
                "VEOVEO_CONSOLE_MCP_TRANSPORT_URL",
                "http://mcp-gateway:8788/mcp/admin"
            )
            .is_ok()
        );
    }

    #[test]
    fn console_mcp_transport_profile_is_typed_from_the_exact_path() {
        let admin = Url::parse("https://public.example/mcp/admin").unwrap();
        let operator = Url::parse("http://mcp-gateway:8788/mcp/operator").unwrap();
        assert_eq!(
            mcp_profile("VEOVEO_CONSOLE_OAUTH_RESOURCE", &admin).unwrap(),
            "admin"
        );
        assert_ne!(
            mcp_profile("VEOVEO_CONSOLE_OAUTH_RESOURCE", &admin).unwrap(),
            mcp_profile("VEOVEO_CONSOLE_MCP_TRANSPORT_URL", &operator).unwrap()
        );
        for invalid in [
            "http://mcp-gateway:8788/mcp/",
            "http://mcp-gateway:8788/mcp/admin/",
            "http://mcp-gateway:8788/internal/mcp/admin",
        ] {
            assert!(
                mcp_profile(
                    "VEOVEO_CONSOLE_MCP_TRANSPORT_URL",
                    &Url::parse(invalid).unwrap()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn console_mcp_transport_defaults_to_oauth_resource_and_rejects_profile_mismatch() {
        let oauth = Url::parse("https://public.example/mcp/operator").unwrap();
        assert_eq!(resolve_mcp_transport_url(&oauth, None).unwrap(), oauth);
        assert_eq!(
            resolve_mcp_transport_url(&oauth, Some("http://mcp-gateway:8788/mcp/operator"))
                .unwrap()
                .as_str(),
            "http://mcp-gateway:8788/mcp/operator"
        );
        let error = resolve_mcp_transport_url(&oauth, Some("http://mcp-gateway:8788/mcp/admin"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("profile `admin` must match"), "{error}");
    }

    #[test]
    fn rerun_mapbox_is_typed_and_invalid_tokens_become_diagnostics() {
        assert!(matches!(
            parse_rerun_map_provider("openStreetMap", None).unwrap(),
            RerunMapProvider::OpenStreetMap
        ));
        assert!(parse_rerun_map_provider("openStreetMap", Some("pk.example".to_owned())).is_err());
        assert!(matches!(
            parse_rerun_map_provider("mapbox", None).unwrap(),
            RerunMapProvider::Mapbox {
                access_token: None,
                diagnostic: Some(_)
            }
        ));
        assert!(matches!(
            parse_rerun_map_provider("mapbox", Some("sk.secret".to_owned())).unwrap(),
            RerunMapProvider::Mapbox {
                access_token: None,
                diagnostic: Some(_)
            }
        ));
        assert!(matches!(
            parse_rerun_map_provider("mapbox", Some("pk.example-token".to_owned())).unwrap(),
            RerunMapProvider::Mapbox { .. }
        ));
    }

    #[test]
    fn config_debug_never_contains_the_mapbox_token() {
        let mut config = Config::for_test(Url::parse("http://127.0.0.1:8788").unwrap());
        config.rerun_map_provider = RerunMapProvider::Mapbox {
            access_token: Some("pk.do-not-print".to_owned()),
            diagnostic: None,
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("pk.do-not-print"));
        assert!(debug.contains("[REDACTED]"));
    }
}
