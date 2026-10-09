//! Transport and report I/O shared by installed source-owner harnesses.
use anyhow::{Context, Result, ensure};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::ClientConfig,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use veoveo_deploy_contract::InstallationTarget;
use veoveo_mcp_conformance::{ConformanceCredentials, ConformanceReport};
use veoveo_types::HttpsUrl;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSource {
    pub installation_target: PathBuf,
    pub endpoint: HttpsUrl,
    pub caller_token_file: PathBuf,
    pub deployment: String,
    pub output: PathBuf,
}

impl InstalledSource {
    pub fn validate(&self) -> Result<InstallationTarget> {
        ensure!(
            self.installation_target.is_absolute(),
            "installation target path must be absolute"
        );
        let target = InstallationTarget::load(&self.installation_target)?;
        require_origin(&self.endpoint, &target)?;
        ensure!(
            self.output.is_absolute() && !self.output.exists(),
            "source report requires a new absolute path"
        );
        Ok(target)
    }

    pub fn credentials(&self) -> Result<ConformanceCredentials> {
        credentials(&self.caller_token_file)
    }

    pub async fn caller(&self) -> Result<RunningService<RoleClient, ClientConfig>> {
        connect(&self.endpoint, &self.caller_token_file).await
    }

    /// Connect with official Tasks support using the admitted private token file.
    pub async fn task_caller(&self) -> Result<crate::SmokeMcpClient> {
        let bearer = token(&self.caller_token_file)?;
        let connection = crate::SmokeMcpHandler
            .serve_with_lifecycle(
                transport(&self.endpoint, &bearer)?,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        Ok(crate::SmokeMcpClient::admitted(
            connection,
            self.endpoint.as_str(),
            &bearer,
            None,
        ))
    }

    pub fn report(&self, report: &ConformanceReport) -> Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.output)?;
        serde_json::to_writer_pretty(&mut file, report)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        ensure!(
            report.passed(),
            "source conformance failed; inspect {}",
            self.output.display()
        );
        Ok(())
    }
}

pub fn credentials(path: &Path) -> Result<ConformanceCredentials> {
    Ok(ConformanceCredentials::bearer(token(path)?))
}

/// Admit an owner-private token file and return a redacted Authorization header.
pub fn bearer_header(path: &Path) -> Result<reqwest::header::HeaderValue> {
    let token = token(path)?;
    let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| anyhow::anyhow!("invalid bearer authorization header"))?;
    value.set_sensitive(true);
    Ok(value)
}

pub async fn close(connection: RunningService<RoleClient, ClientConfig>) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), connection.cancel())
        .await
        .context("source connection shutdown exceeded ten seconds")??;
    Ok(())
}

pub fn require_origin(endpoint: &HttpsUrl, target: &InstallationTarget) -> Result<()> {
    ensure!(
        reqwest::Url::parse(endpoint.as_str())?.origin() == target.public_base_url.origin(),
        "source caller endpoint must belong to the installation's public origin"
    );
    Ok(())
}

pub async fn connect(
    endpoint: &HttpsUrl,
    token_file: &Path,
) -> Result<RunningService<RoleClient, ClientConfig>> {
    let transport = transport(endpoint, &token(token_file)?)?;
    Ok(ClientConfig::default()
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?)
}

fn transport(
    endpoint: &HttpsUrl,
    bearer: &str,
) -> Result<StreamableHttpClientTransport<reqwest::Client>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(StreamableHttpClientTransport::with_client(
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(65))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        StreamableHttpClientTransportConfig::with_uri(endpoint.as_str())
            .auth_header(bearer.to_owned()),
    ))
}

pub async fn read<T: serde::de::DeserializeOwned>(
    peer: &rmcp::Peer<RoleClient>,
    uri: &veoveo_types::ResourceUri,
) -> Result<T> {
    let result = peer
        .read_resource(rmcp::model::ReadResourceRequestParams::new(uri.as_str()))
        .await?;
    let [
        rmcp::model::ResourceContents::TextResourceContents {
            uri: actual, text, ..
        },
    ] = result.contents.as_slice()
    else {
        anyhow::bail!("source fixture requires one JSON resource");
    };
    ensure!(
        actual == uri.as_str() && text.len() <= 64 * 1024,
        "source fixture resource identity or size is invalid"
    );
    serde_json::from_str(text).context("decode source fixture resource")
}

pub fn input<T: serde::de::DeserializeOwned>() -> Result<T> {
    input_from("VEOVEO_SOURCE_CONFORMANCE_INPUT")
}

pub fn input_from<T: serde::de::DeserializeOwned>(variable: &str) -> Result<T> {
    let path = PathBuf::from(
        std::env::var_os(variable)
            .with_context(|| format!("set {variable} to an owner fixture file"))?,
    );
    ensure!(path.is_absolute(), "source fixture path must be absolute");
    ensure!(
        fs::metadata(&path)?.len() <= 64 * 1024,
        "source fixture input exceeds 64 KiB"
    );
    serde_json::from_slice(&fs::read(path)?).context("decode source fixture input")
}

fn token(path: &Path) -> Result<String> {
    ensure!(path.is_absolute(), "source token file must be absolute");
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 64 * 1024,
        "invalid source token file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "source token file must be private to its owner"
        );
    }
    let token = fs::read_to_string(path)?;
    ensure!(!token.trim().is_empty(), "source token file is empty");
    Ok(token.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_header_requires_private_file_and_redacts_debug() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("token");
        fs::write(&path, "fixture-private-token")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
            ensure!(bearer_header(&path).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        let header = bearer_header(&path)?;
        ensure!(header.is_sensitive());
        ensure!(!format!("{header:?}").contains("fixture-private-token"));
        fs::write(&path, "fixture\nprivate-token")?;
        let error = bearer_header(&path).unwrap_err();
        ensure!(error.to_string() == "invalid bearer authorization header");
        ensure!(!format!("{error:?}").contains("private-token"));
        fs::write(&path, " \n")?;
        ensure!(bearer_header(&path).is_err());
        ensure!(bearer_header(Path::new("relative-token")).is_err());
        ensure!(bearer_header(directory.path()).is_err());
        Ok(())
    }
}
