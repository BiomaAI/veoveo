use super::*;

pub(crate) struct SmokeMcpClient;

impl ClientHandler for SmokeMcpClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-smoke", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub(crate) type SmokeMcpSession = RunningService<rmcp::RoleClient, SmokeMcpClient>;

pub(crate) fn run_mcp(
    conformance: &Path,
    gateway_base: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), format!("{gateway_base}/mcp/default").into()];
    all_args.extend(args);
    run_checked(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])
}

pub(crate) async fn connect_mcp_session(url: &str, bearer_token: &str) -> Result<SmokeMcpSession> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.to_string())
            .auth_header(bearer_token.to_string()),
    );
    Ok(SmokeMcpClient.serve(transport).await?)
}

pub(crate) async fn read_mcp_resource_json(session: &SmokeMcpSession, uri: &str) -> Result<Value> {
    let result = session
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let Some(text) = result.contents.iter().find_map(|content| match content {
        ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
        _ => None,
    }) else {
        bail!("MCP resource `{uri}` did not return text content: {result:?}");
    };
    Ok(serde_json::from_str(text)?)
}

pub(crate) async fn assert_mcp_session_resource_denied(
    session: &SmokeMcpSession,
    uri: &str,
) -> Result<()> {
    if read_mcp_resource_json(session, uri).await.is_ok() {
        bail!("same MCP session unexpectedly read `{uri}` after policy update");
    }
    Ok(())
}

pub(crate) fn run_direct_mcp(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), url.into()];
    all_args.extend(args);
    run_checked(conformance, all_args, envs)
}

pub(crate) fn assert_mcp_denied(
    conformance: &Path,
    mcp_url: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<()> {
    let mut all_args = vec!["--url".into(), mcp_url.into()];
    all_args.extend(args);
    let output = run_raw(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])?;
    if output.status.success() {
        bail!(
            "MCP command was unexpectedly authorized\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
