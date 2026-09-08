use super::*;

pub(super) struct OperatorClient<'a> {
    pub(super) conformance: &'a Path,
    pub(super) base: &'a str,
}

impl OperatorClient<'_> {
    pub(super) async fn conformance(
        &self,
        operation: &[&str],
        timeout: Duration,
    ) -> Result<String> {
        let token = gateway_token(self.conformance, self.base).await?;
        gateway_conformance(
            self.conformance,
            self.base,
            "operator",
            &token,
            operation,
            timeout,
        )
        .await
    }

    pub(super) async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value> {
        self.call_tool_with_timeout(tool, arguments, Duration::from_secs(120))
            .await
    }

    pub(super) async fn call_tool_with_timeout(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let arguments = serde_json::to_string(&arguments)?;
        let output = self
            .conformance(
                &["call", "--tool-name", tool, "--arguments", &arguments],
                timeout,
            )
            .await?;
        structured_output(&output).with_context(|| format!("tool {tool} returned invalid output"))
    }

    pub(super) async fn task_tool(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let arguments = serde_json::to_string(&arguments)?;
        let timeout_seconds = timeout.as_secs().max(1).to_string();
        let output = self
            .conformance(
                &[
                    "task-call",
                    "--tool-name",
                    tool,
                    "--arguments",
                    &arguments,
                    "--timeout-seconds",
                    &timeout_seconds,
                ],
                timeout
                    .checked_add(Duration::from_secs(30))
                    .context("task timeout overflowed the subprocess margin")?,
            )
            .await?;
        structured_output(&output)
            .with_context(|| format!("task tool {tool} returned invalid output"))
    }

    pub(super) async fn resource_text(&self, uri: &str, timeout: Duration) -> Result<String> {
        self.conformance(&["resource", uri], timeout).await
    }

    pub(super) async fn resource(&self, uri: &str, timeout: Duration) -> Result<Value> {
        let output = self.resource_text(uri, timeout).await?;
        serde_json::from_str(&output)
            .with_context(|| format!("resource {uri} returned invalid JSON"))
    }
}
pub(super) async fn gateway_token(conformance: &Path, base: &str) -> Result<String> {
    gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        OPERATOR_PROFILE_SCOPES,
        "operations",
    )
    .await
}

pub(super) async fn gateway_conformance(
    conformance: &Path,
    base: &str,
    profile: &str,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let url = format!("{base}/mcp/{profile}");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args(["--url", &url, "--scheme", "uav-sim"])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .with_context(|| format!("conformance operation {operation:?} timed out"))??;
    ensure!(
        output.status.success(),
        "conformance operation {operation:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("decoding conformance output")
}

pub(super) fn structured_output(output: &str) -> Result<Value> {
    let encoded = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .with_context(|| format!("conformance output omitted structured content:\n{output}"))?;
    serde_json::from_str(encoded).context("decoding structured MCP output")
}

pub(super) fn json_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON output omitted string {pointer}: {value}"))
}

pub(super) fn json_number(object: &serde_json::Map<String, Value>, key: &str) -> Result<f64> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("georeference_origin omitted numeric {key}"))
}
