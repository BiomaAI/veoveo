use super::*;

pub struct OperatorClient<'a> {
    pub conformance: &'a Path,
    pub installation: &'a InstalledTarget,
}

impl OperatorClient<'_> {
    pub async fn conformance(&self, operation: &[&str], timeout: Duration) -> Result<String> {
        let token = self.installation.token().await?;
        gateway_conformance(
            self.conformance,
            &self.installation.operator.resource,
            &token,
            operation,
            timeout,
        )
        .await
    }

    pub async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value> {
        self.call_tool_with_timeout(tool, arguments, Duration::from_secs(120))
            .await
    }

    pub async fn call_tool_with_timeout(
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

    pub async fn task_tool(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        veoveo_testing_support::lifecycle::owner::check_effect()?;
        let token = self.installation.token().await?;
        let session = veoveo_testing_support::connect_mcp_client(
            self.installation.operator.resource.as_str(),
            &token,
        )
        .await?;
        let result = run_owned_task(&session, tool, arguments, timeout).await;
        session.cancel().await?;
        result
    }

    pub async fn resource_text(&self, uri: &str, timeout: Duration) -> Result<String> {
        self.conformance(&["resource", uri], timeout).await
    }

    pub async fn resource(&self, uri: &str, timeout: Duration) -> Result<Value> {
        let output = self.resource_text(uri, timeout).await?;
        serde_json::from_str(&output)
            .with_context(|| format!("resource {uri} returned invalid JSON"))
    }
}
pub async fn gateway_conformance(
    conformance: &Path,
    resource: &veoveo_gateway_contract::ProtectedResourceId,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let mut command = tokio::process::Command::new(conformance);
    command
        .args(["--url", resource.as_str(), "--scheme", "uav-sim"])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        timeout,
        veoveo_testing_support::output_async(command, timeout),
    )
    .await
    .with_context(|| format!("conformance operation {operation:?} timed out"))??;
    ensure!(
        output.status.success(),
        "conformance operation {operation:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("decoding conformance output")
}

pub fn structured_output(output: &str) -> Result<Value> {
    let encoded = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .with_context(|| format!("conformance output omitted structured content:\n{output}"))?;
    serde_json::from_str(encoded).context("decoding structured MCP output")
}

pub fn json_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON output omitted string {pointer}: {value}"))
}

pub fn json_number(object: &serde_json::Map<String, Value>, key: &str) -> Result<f64> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("georeference_origin omitted numeric {key}"))
}

/// Flight's concrete maintained SDK path, shared by the installed caller and
/// controlled peer tests. Task registration occurs inside call_tool_as_task.
async fn run_owned_task(
    session: &veoveo_testing_support::SmokeMcpClient,
    tool: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value> {
    use veoveo_testing_support::{
        await_task_terminal_with_timeout, call_tool_as_task, task_payload,
    };
    let task = call_tool_as_task(session, tool, arguments).await?;
    eprintln!("task {} created (status {:?})", task.task_id, task.status);
    let terminal = await_task_terminal_with_timeout(session, &task.task_id, timeout).await?;
    ensure!(
        terminal.status() == rmcp::model::TaskStatus::Completed,
        "Flight Task did not complete successfully"
    );
    let result = task_payload(session, &task.task_id).await?;
    ensure!(
        result.is_error != Some(true),
        "Flight Task returned a tool error"
    );
    result
        .structured_content
        .context("Flight Task omitted structured content")
}

#[cfg(test)]
mod tests;
