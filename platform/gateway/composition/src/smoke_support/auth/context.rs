use anyhow::{Context, Result, ensure};
use std::{process::Stdio, time::Duration};

/// Credentials are selected per child process, never by changing global environment.
#[derive(Clone, Copy)]
pub(crate) enum ClientCredentials {
    Operator,
    Administrator,
}

impl ClientCredentials {
    fn environment(self) -> [&'static str; 2] {
        match self {
            Self::Operator => [
                "VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE",
                "VEOVEO_SERVICE_CLIENT_KEY_ID",
            ],
            Self::Administrator => [
                "VEOVEO_ADMIN_SERVICE_CLIENT_PRIVATE_KEY_FILE",
                "VEOVEO_ADMIN_SERVICE_CLIENT_KEY_ID",
            ],
        }
    }

    pub fn validate(self) -> Result<()> {
        for name in self.environment() {
            ensure!(
                std::env::var_os(name).is_some_and(|value| !value.is_empty()),
                "{name} is required for the selected installation client"
            );
        }
        Ok(())
    }
}

/// Wire adapter for a request already admitted against the installed control plane.
pub(crate) struct TokenRequest<'a> {
    pub token_url: &'a str,
    pub resource: &'a str,
    pub client_id: &'a str,
    pub scopes: &'a [&'a str],
    pub work_context: &'a str,
    pub credentials: ClientCredentials,
}

pub(crate) async fn exchange_token(request: TokenRequest<'_>) -> Result<String> {
    request.credentials.validate()?;
    let [key_file, key_id] = request.credentials.environment();
    let prepared = veoveo_testing_support::artifacts::binary_command(
        "veoveo-gateway-composition",
        "gateway-smoke-support",
    )?;
    let mut command = tokio::process::Command::from(prepared);
    command
        .args([
            "gateway-token-exchange",
            "--token-url",
            request.token_url,
            "--client-id",
            request.client_id,
            "--audience",
            request.token_url,
            "--resource",
            request.resource,
            "--work-context",
            request.work_context,
        ])
        .args(request.scopes.iter().flat_map(|scope| ["--scope", *scope]))
        .env(
            "VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE",
            std::env::var_os(key_file).context("missing client key file")?,
        )
        .env(
            "VEOVEO_SERVICE_CLIENT_KEY_ID",
            std::env::var_os(key_id).context("missing client key ID")?,
        )
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        veoveo_testing_support::output_async(command, Duration::from_secs(60)),
    )
    .await
    .context("gateway token exchange timed out")??;
    ensure!(
        output.status.success(),
        "gateway token exchange failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let token = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(!token.is_empty(), "gateway returned an empty access token");
    Ok(token)
}
