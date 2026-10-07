//! Headed CDP transport and hardware admission reused by owning browser assertions.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, fs, path::Path, sync::Arc, time::Duration};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, http::StatusCode},
};
use url::Url;
fn register_target_cleanup(
    cdp_base: &str,
    pending_id: &Arc<std::sync::Mutex<Option<String>>>,
) -> Result<veoveo_testing_support::lifecycle::owner::CleanupRegistration> {
    use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
    let observed_id = Arc::clone(pending_id);
    let cleanup_base = cdp_base.to_owned();
    let intent = uuid::Uuid::now_v7();
    owner::register_cleanup(
        CleanupKind::Browser,
        "headed CDP target",
        &intent.to_string(),
        move || async move {
            let target_id = observed_id
                .lock()
                .expect("owned browser identity")
                .clone()
                .context("browser create outcome unresolved; target reconciliation required")?;
            let mut cleanup = connect_headed_browser(&cleanup_base, "owned target cleanup").await?;
            let result = cleanup
                .command(
                    "Target.closeTarget",
                    serde_json::json!({"targetId":target_id}),
                    None,
                )
                .await?;
            ensure!(
                result.get("success").and_then(Value::as_bool) == Some(true),
                "owned browser target closure not confirmed"
            );
            Ok(())
        },
    )
}

pub async fn open_headed_target(cdp_base: &str, page_url: &str) -> Result<(Cdp, String, String)> {
    open_headed_target_in_window(cdp_base, page_url, false).await
}

pub async fn open_headed_target_in_window(
    cdp_base: &str,
    page_url: &str,
    new_window: bool,
) -> Result<(Cdp, String, String)> {
    let mut cdp = connect_headed_browser(cdp_base, "visual acceptance").await?;
    use veoveo_testing_support::lifecycle::owner;
    let pending_id = Arc::new(std::sync::Mutex::new(None::<String>));
    let registration = register_target_cleanup(cdp_base, &pending_id)?;
    owner::check_effect()?;
    let target = cdp
        .command(
            "Target.createTarget",
            serde_json::json!({"url": page_url, "newWindow": new_window}),
            None,
        )
        .await?;
    let target_id = value_string(&target, "/targetId")?.to_owned();
    *pending_id.lock().expect("owned browser identity") = Some(target_id.clone());
    registration.observed_identity(&target_id)?;
    cdp.owned_targets.insert(target_id.clone(), registration);
    let attached = cdp
        .command(
            "Target.attachToTarget",
            serde_json::json!({"targetId": target_id, "flatten": true}),
            None,
        )
        .await?;
    let session_id = value_string(&attached, "/sessionId")?.to_owned();
    for method in [
        "Runtime.enable",
        "Page.enable",
        "DOM.enable",
        "Log.enable",
        "Network.enable",
    ] {
        cdp.command(method, serde_json::json!({}), Some(&session_id))
            .await?;
    }
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        serde_json::json!({
            "width": 1920,
            "height": 1080,
            "deviceScaleFactor": 1,
            "mobile": false,
        }),
        Some(&session_id),
    )
    .await?;
    cdp.command(
        "Page.bringToFront",
        serde_json::json!({}),
        Some(&session_id),
    )
    .await?;
    wait_for_requested_document(&mut cdp, &session_id, page_url).await?;
    Ok((cdp, target_id, session_id))
}

pub async fn close_target(cdp: &mut Cdp, target_id: &str) -> Result<()> {
    let registration = cdp
        .owned_targets
        .get(target_id)
        .context("refuse closure of a browser target not created by this owner")?
        .clone();
    let result = cdp
        .command(
            "Target.closeTarget",
            serde_json::json!({"targetId":target_id}),
            None,
        )
        .await?;
    ensure!(
        result.get("success").and_then(Value::as_bool) == Some(true),
        "owned browser target closure not confirmed"
    );
    registration.settled()?;
    cdp.owned_targets.remove(target_id);
    Ok(())
}

pub async fn assert_page_visible(cdp: &mut Cdp, session_id: &str) -> Result<()> {
    let visible: bool = cdp
        .evaluate(session_id, "document.visibilityState === 'visible'", false)
        .await?;
    ensure!(
        visible,
        "visual acceptance requires the headed Console target to remain visible"
    );
    Ok(())
}

pub async fn capture_screenshot(cdp: &mut Cdp, session_id: &str, output: &Path) -> Result<String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating screenshot directory {}", parent.display()))?;
    }
    let result = cdp
        .command(
            "Page.captureScreenshot",
            serde_json::json!({
                "format": "png",
                "fromSurface": true,
                "captureBeyondViewport": false
            }),
            Some(session_id),
        )
        .await?;
    let encoded = value_string(&result, "/data")?;
    let bytes = STANDARD
        .decode(encoded)
        .context("decoding Chrome PNG screenshot")?;
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "Chrome screenshot was not PNG"
    );
    fs::write(output, &bytes)
        .with_context(|| format!("writing screenshot {}", output.display()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeVersion {
    web_socket_debugger_url: String,
}

pub async fn connect_headed_browser(endpoint: &str, acceptance: &str) -> Result<Cdp> {
    let endpoint = Url::parse(endpoint).context("parsing Chrome DevTools endpoint")?;
    let browser_web_socket = match endpoint.scheme() {
        "ws" => endpoint.to_string(),
        "http" | "https" => {
            let version_url = endpoint
                .join("json/version")
                .context("Chrome CDP URL cannot resolve /json/version")?;
            let version: ChromeVersion = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()?
                .get(version_url)
                .send()
                .await
                .context("headed Chrome DevTools endpoint is unavailable")?
                .error_for_status()?
                .json()
                .await?;
            version.web_socket_debugger_url
        }
        scheme => bail!(
            "Chrome DevTools endpoint must use http:// discovery or a direct ws:// browser endpoint, received {scheme:?}"
        ),
    };
    let mut cdp = Cdp::connect(&browser_web_socket).await?;
    let version = cdp
        .command("Browser.getVersion", serde_json::json!({}), None)
        .await
        .context("querying the attached Chrome identity")?;
    let product = value_string(&version, "/product")?;
    ensure!(
        !product.to_ascii_lowercase().contains("headless"),
        "{acceptance} requires headed Chrome; endpoint reported {product}"
    );
    Ok(cdp)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HardwareIdentity {
    user_agent: String,
    webgpu_vendor: String,
    webgpu_architecture: String,
    webgpu_device: String,
    webgpu_description: String,
    webgl_available: bool,
    webgl_vendor: String,
    webgl_renderer: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserGpuApi {
    WebGpu,
    WebGl,
}

impl HardwareIdentity {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.user_agent.contains("HeadlessChrome"),
            "attached Chrome is headless"
        );
        let (hardware_apis, webgpu, webgl) = self.hardware_apis();
        ensure!(
            !hardware_apis.is_empty(),
            "headed Chrome requires hardware-backed NVIDIA WebGPU or WebGL; \
             received WebGPU {webgpu:?} and WebGL {webgl:?}"
        );
        Ok(())
    }

    pub fn hardware_apis(&self) -> (Vec<BrowserGpuApi>, String, String) {
        let webgpu = format!(
            "{} {} {} {}",
            self.webgpu_vendor,
            self.webgpu_architecture,
            self.webgpu_device,
            self.webgpu_description
        )
        .to_ascii_lowercase();
        let webgl = format!("{} {}", self.webgl_vendor, self.webgl_renderer).to_ascii_lowercase();
        let mut hardware_apis = Vec::with_capacity(2);
        if !self.webgpu_vendor.is_empty()
            && webgpu.contains("nvidia")
            && !software_renderer(&webgpu)
        {
            hardware_apis.push(BrowserGpuApi::WebGpu);
        }
        if self.webgl_available && webgl.contains("nvidia") && !software_renderer(&webgl) {
            hardware_apis.push(BrowserGpuApi::WebGl);
        }
        (hardware_apis, webgpu, webgl)
    }
}

pub async fn wait_for_document(cdp: &mut Cdp, session_id: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let ready: bool = cdp
            .evaluate(
                session_id,
                r#"document.readyState === "complete" || document.readyState === "interactive""#,
                false,
            )
            .await?;
        if ready {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "App host document did not load"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn wait_for_requested_document(
    cdp: &mut Cdp,
    session_id: &str,
    requested_url: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let document = cdp
            .evaluate(
                session_id,
                r#"({href:location.href,readyState:document.readyState})"#,
                false,
            )
            .await?;
        if document_is_ready_at(&document, requested_url) {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "headed browser target did not load requested URL {requested_url:?}: {document}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn document_is_ready_at(document: &Value, requested_url: &str) -> bool {
    document.get("href").and_then(Value::as_str) == Some(requested_url)
        && document
            .get("readyState")
            .and_then(Value::as_str)
            .is_some_and(|state| state == "complete" || state == "interactive")
}

pub struct Cdp {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    next_id: u64,
    events: VecDeque<Value>,
    software_renderer_event: bool,
    owned_targets: std::collections::BTreeMap<
        String,
        veoveo_testing_support::lifecycle::owner::CleanupRegistration,
    >,
}

impl Cdp {
    /// Retained maintained CDP notifications for owner-specific assertions.
    pub fn retained_events(&self) -> impl Iterator<Item = &Value> {
        self.events.iter()
    }

    pub async fn connect(url: &str) -> Result<Self> {
        ensure!(
            url.starts_with("ws://"),
            "headed Chrome DevTools WebSocket must be local plaintext ws://"
        );
        let (socket, response) = connect_async(url).await?;
        ensure!(
            response.status() == StatusCode::SWITCHING_PROTOCOLS,
            "Chrome DevTools WebSocket returned {}",
            response.status()
        );
        Ok(Self {
            socket,
            next_id: 1,
            events: VecDeque::new(),
            software_renderer_event: false,
            owned_targets: std::collections::BTreeMap::new(),
        })
    }

    pub async fn command(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut request = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(session_id) = session_id {
            request["sessionId"] = Value::String(session_id.to_owned());
        }
        self.socket
            .send(Message::Text(serde_json::to_string(&request)?.into()))
            .await?;
        loop {
            let message = self
                .socket
                .next()
                .await
                .context("Chrome DevTools WebSocket closed")??;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_ref())?;
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        if let Some(error) = value.get("error") {
                            bail!("Chrome DevTools `{method}` failed: {error}");
                        }
                        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                    }
                    if !self.software_renderer_event {
                        let encoded = serde_json::to_string(&value)?.to_ascii_lowercase();
                        self.software_renderer_event = software_renderer(&encoded);
                    }
                    if retain_cdp_event(&value) {
                        const MAX_RETAINED_EVENTS: usize = 512;
                        if self.events.len() == MAX_RETAINED_EVENTS {
                            self.events.pop_front();
                        }
                        self.events.push_back(value);
                    }
                }
                Message::Ping(value) => self.socket.send(Message::Pong(value)).await?,
                Message::Close(frame) => {
                    bail!("Chrome DevTools WebSocket closed unexpectedly: {frame:?}")
                }
                _ => {}
            }
        }
    }

    pub async fn evaluate<T: serde::de::DeserializeOwned>(
        &mut self,
        session_id: &str,
        expression: &str,
        await_promise: bool,
    ) -> Result<T> {
        let result = self
            .command(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "awaitPromise": await_promise,
                    "returnByValue": true,
                    "userGesture": true,
                }),
                Some(session_id),
            )
            .await?;
        if let Some(exception) = result.get("exceptionDetails") {
            bail!("browser evaluation failed: {exception}");
        }
        let value = result
            .pointer("/result/value")
            .cloned()
            .with_context(|| format!("browser evaluation returned no value: {result}"))?;
        serde_json::from_value(value).context("decoding browser evaluation result")
    }

    pub async fn evaluate_context<T: serde::de::DeserializeOwned>(
        &mut self,
        session_id: &str,
        context_id: u64,
        expression: &str,
        await_promise: bool,
    ) -> Result<T> {
        let result = self
            .command(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "contextId": context_id,
                    "awaitPromise": await_promise,
                    "returnByValue": true,
                    "userGesture": true,
                }),
                Some(session_id),
            )
            .await?;
        if let Some(exception) = result.get("exceptionDetails") {
            bail!("browser App-frame evaluation failed: {exception}");
        }
        let value = result
            .pointer("/result/value")
            .cloned()
            .with_context(|| format!("browser App-frame evaluation returned no value: {result}"))?;
        serde_json::from_value(value).context("decoding browser App-frame evaluation result")
    }

    pub fn assert_no_software_renderer_events(&self) -> Result<()> {
        ensure!(
            !self.software_renderer_event,
            "headed Chrome emitted a software-renderer event"
        );
        Ok(())
    }

    pub async fn stream_diagnostics(&mut self, session_id: &str) -> Result<String> {
        let mut events = Vec::new();
        for event in self.events.iter().rev() {
            let Some(summary) = stream_event_summary(event) else {
                continue;
            };
            if !events.contains(&summary) {
                events.push(summary);
            }
            if events.len() == 16 {
                break;
            }
        }
        let exception_object_ids = self
            .events
            .iter()
            .rev()
            .filter_map(|event| {
                event
                    .pointer("/params/exceptionDetails/exception/objectId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .take(4)
            .collect::<Vec<_>>();
        for object_id in exception_object_ids {
            let Some(summary) = self.browser_object_summary(session_id, &object_id, 0).await else {
                continue;
            };
            let summary = format!("browser exception object {summary}");
            if !events.contains(&summary) {
                events.push(summary);
            }
        }
        events.reverse();
        Ok(if events.is_empty() {
            "Chrome reported no WebSocket response or transport error".to_owned()
        } else {
            format!("Chrome WebSocket diagnostics: {}", events.join("; "))
        })
    }

    pub async fn browser_object_summary(
        &mut self,
        session_id: &str,
        object_id: &str,
        depth: usize,
    ) -> Option<String> {
        let result = self
            .command(
                "Runtime.getProperties",
                serde_json::json!({
                    "objectId": object_id,
                    "ownProperties": true,
                    "accessorPropertiesOnly": false,
                    "generatePreview": true,
                }),
                Some(session_id),
            )
            .await
            .ok()?;
        let mut details = Vec::new();
        for property in result.get("result")?.as_array()? {
            let name = property.get("name")?.as_str()?;
            if !matches!(
                name,
                "action"
                    | "status"
                    | "info"
                    | "name"
                    | "message"
                    | "code"
                    | "description"
                    | "cause"
                    | "reason"
            ) {
                continue;
            }
            let Some(value) = property.get("value") else {
                continue;
            };
            if let Some(primitive) = browser_remote_primitive(value) {
                details.push(format!("{name}={primitive}"));
                continue;
            }
            if depth == 0
                && matches!(name, "info" | "cause" | "reason")
                && let Some(nested_object_id) = value.get("objectId").and_then(Value::as_str)
                && let Some(nested) =
                    Box::pin(self.browser_object_summary(session_id, nested_object_id, depth + 1))
                        .await
            {
                details.push(format!("{name}=({nested})"));
            }
        }
        (!details.is_empty()).then(|| bounded_browser_diagnostic(&details.join(",")))
    }
}

pub fn browser_remote_primitive(value: &Value) -> Option<String> {
    let primitive = value.get("value")?;
    let rendered = match primitive {
        Value::String(value) => value.to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(_) | Value::Object(_) => return None,
    };
    Some(bounded_browser_diagnostic(&rendered))
}

pub fn retain_cdp_event(event: &Value) -> bool {
    match event.get("method").and_then(Value::as_str) {
        Some(
            "Network.requestWillBeSent"
            | "Network.responseReceived"
            | "Network.loadingFinished"
            | "Network.loadingFailed"
            | "Network.webSocketCreated"
            | "Network.webSocketHandshakeResponseReceived"
            | "Network.webSocketFrameError"
            | "Network.webSocketClosed",
        ) => true,
        Some("Runtime.exceptionThrown") => true,
        Some("Runtime.consoleAPICalled") => matches!(
            event.pointer("/params/type").and_then(Value::as_str),
            Some("error" | "warning")
        ),
        _ => false,
    }
}

pub fn stream_event_summary(event: &Value) -> Option<String> {
    match event.get("method").and_then(Value::as_str)? {
        "Network.webSocketCreated" => event
            .pointer("/params/url")
            .and_then(Value::as_str)
            .map(redacted_network_url)
            .map(|url| format!("created {url}")),
        "Network.webSocketHandshakeResponseReceived" => {
            let status = event.pointer("/params/response/status")?.as_u64()?;
            let status_text = event
                .pointer("/params/response/statusText")
                .and_then(Value::as_str)
                .unwrap_or("");
            Some(
                format!("handshake HTTP {status} {status_text}")
                    .trim()
                    .to_owned(),
            )
        }
        "Network.webSocketFrameError" => event
            .pointer("/params/errorMessage")
            .and_then(Value::as_str)
            .map(|error| format!("frame error {error}")),
        "Network.loadingFailed" => event
            .pointer("/params/errorText")
            .and_then(Value::as_str)
            .map(|error| format!("load failed {error}")),
        "Runtime.exceptionThrown" => browser_exception_summary(event),
        "Runtime.consoleAPICalled" => {
            let message = event
                .pointer("/params/args")?
                .as_array()?
                .iter()
                .filter_map(|argument| {
                    argument
                        .get("value")
                        .and_then(Value::as_str)
                        .or_else(|| argument.get("description").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join(" ");
            (!message.is_empty()).then(|| {
                format!(
                    "browser {} {}",
                    event
                        .pointer("/params/type")
                        .and_then(Value::as_str)
                        .unwrap_or("console"),
                    bounded_browser_diagnostic(&message)
                )
            })
        }
        _ => None,
    }
}

pub fn browser_exception_summary(event: &Value) -> Option<String> {
    let exception = event.pointer("/params/exceptionDetails/exception");
    let description = exception
        .and_then(|value| value.get("description"))
        .or_else(|| event.pointer("/params/exceptionDetails/text"))
        .and_then(Value::as_str)?;
    let properties = exception
        .and_then(|value| value.pointer("/preview/properties"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|property| {
            let name = property.get("name")?.as_str()?;
            let value = property
                .get("value")
                .or_else(|| property.get("valuePreview"))?
                .as_str()?;
            Some(format!("{name}={value}"))
        })
        .collect::<Vec<_>>()
        .join(",");
    let detail = if properties.is_empty() {
        description.to_owned()
    } else {
        format!("{description} ({properties})")
    };
    Some(format!(
        "browser exception {}",
        bounded_browser_diagnostic(&detail)
    ))
}

pub fn bounded_browser_diagnostic(value: &str) -> String {
    let lowercase = value.to_ascii_lowercase();
    if [
        "authorization",
        "bearer",
        "access_token",
        "accesstoken",
        "jwt",
    ]
    .iter()
    .any(|needle| lowercase.contains(needle))
    {
        return "[redacted authentication-bearing browser diagnostic]".to_owned();
    }
    value.chars().take(320).collect()
}

pub fn redacted_network_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return "unparseable WebSocket URL".to_owned();
    };
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

pub fn value_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("Chrome DevTools response omitted {pointer}: {value}"))
}

pub fn software_renderer(value: &str) -> bool {
    [
        "swiftshader",
        "llvmpipe",
        "software rasterizer",
        "software adapter",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_target_refusal_retains_actual_identity_and_never_closes_foreign_target() {
        const CHILD: &str = "VEOVEO_TEST_BROWSER_REFUSAL";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let endpoint = format!("ws://{}", listener.local_addr().unwrap());
                let observed = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
                let recorded = Arc::clone(&observed);
                let server = tokio::spawn(async move {
                    for _ in 0..2 {
                        let (stream, _) = listener.accept().await.unwrap();
                        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                        while let Some(message) = socket.next().await {
                            let message = message.unwrap();
                            if !message.is_text() {
                                continue;
                            }
                            let request: Value =
                                serde_json::from_str(message.to_text().unwrap()).unwrap();
                            let method = request["method"].as_str().unwrap();
                            let result = if method == "Browser.getVersion" {
                                serde_json::json!({"product":"Chrome/fixture"})
                            } else {
                                assert_eq!(method, "Target.closeTarget");
                                let id = request["params"]["targetId"].as_str().unwrap();
                                assert_eq!(
                                    id, "owned-target",
                                    "foreign browser target received a closure"
                                );
                                recorded.lock().unwrap().push(id.to_owned());
                                serde_json::json!({"success":false})
                            };
                            socket
                                .send(Message::Text(
                                    serde_json::json!({"id":request["id"],"result":result})
                                        .to_string()
                                        .into(),
                                ))
                                .await
                                .unwrap();
                            if method == "Target.closeTarget" {
                                break;
                            }
                        }
                    }
                });
                let result = veoveo_testing_support::lifecycle::owner::run(async {
                    let pending = Arc::new(std::sync::Mutex::new(Some("owned-target".to_owned())));
                    let registration = register_target_cleanup(&endpoint, &pending)?;
                    registration.observed_identity("owned-target")?;
                    let mut cdp = Cdp::connect(&endpoint).await?;
                    cdp.owned_targets
                        .insert("owned-target".into(), registration);
                    assert!(close_target(&mut cdp, "foreign-target").await.is_err());
                    assert!(close_target(&mut cdp, "owned-target").await.is_err());
                    assert!(
                        cdp.owned_targets.contains_key("owned-target"),
                        "refusal discarded target ownership"
                    );
                    Ok(())
                })
                .await;
                assert!(
                    result.is_err(),
                    "refused target closure became owner success"
                );
                tokio::time::timeout(Duration::from_secs(2), server)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(*observed.lock().unwrap(), ["owned-target", "owned-target"]);
                let receipt = fs::read_dir(&root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".unresolved-owner-")
                    })
                    .unwrap();
                let retained: Value = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
                assert_eq!(retained[0]["kind"], "browser");
                assert_eq!(retained[0]["observedIdentity"], "owned-target");
                assert_eq!(retained[0]["settled"], false);
            });
            return;
        }
        let root =
            std::env::temp_dir().join(format!("veoveo-browser-refusal-{}", uuid::Uuid::now_v7()));
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "tests::browser_target_refusal_retains_actual_identity_and_never_closes_foreign_target", "--nocapture"])
            .env(CHILD, &root).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root);
        let mut child = veoveo_testing_support::ChildGuard::from_command(command)
            .unwrap()
            .with_drain_on_drop(Duration::ZERO);
        let end = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "browser refusal control failed; private fixture at {}",
                    root.display()
                );
                break;
            }
            assert!(
                std::time::Instant::now() < end,
                "browser refusal control timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn requested_document_readiness_rejects_the_initial_about_blank_target() {
        let requested = "https://installation.example/console/#/apps";
        assert!(!document_is_ready_at(
            &serde_json::json!({"href": "about:blank", "readyState": "complete"}),
            requested
        ));
        assert!(document_is_ready_at(
            &serde_json::json!({"href": requested, "readyState": "interactive"}),
            requested
        ));
    }
    #[test]
    fn chrome_version_uses_the_cdp_websocket_wire_casing() {
        let version: ChromeVersion = serde_json::from_value(serde_json::json!({
            "Browser": "Chrome/150.0.7871.186",
            "Protocol-Version": "1.3",
            "webSocketDebuggerUrl": "ws://127.0.0.1:9227/devtools/browser/id"
        }))
        .unwrap();
        assert_eq!(
            version.web_socket_debugger_url,
            "ws://127.0.0.1:9227/devtools/browser/id"
        );
    }
    #[test]
    fn stream_diagnostics_omit_request_headers_and_url_queries() {
        let created = serde_json::json!({
            "method": "Network.webSocketCreated",
            "params": {
                "url": "ws://localhost:8782/uav-sim/signaling/sign_in?peer_id=peer",
                "initiator": {
                    "requestHeaders": {
                        "Sec-WebSocket-Protocol": "authorization.bearer.secret"
                    }
                }
            }
        });
        let response = serde_json::json!({
            "method": "Network.webSocketHandshakeResponseReceived",
            "params": {
                "response": {
                    "status": 403,
                    "statusText": "Forbidden",
                    "headers": {
                        "Sec-WebSocket-Protocol": "authorization.bearer.secret"
                    }
                }
            }
        });
        let summaries = [&created, &response]
            .into_iter()
            .filter_map(stream_event_summary)
            .collect::<Vec<_>>()
            .join("; ");
        assert_eq!(
            summaries,
            "created ws://localhost:8782/uav-sim/signaling/sign_in; \
         handshake HTTP 403 Forbidden"
        );
        assert!(!summaries.contains("secret"));
        assert!(!summaries.contains("peer"));
    }
    #[test]
    fn software_renderer_fingerprints_fail_closed() {
        assert!(software_renderer("google swiftshader"));
        assert!(software_renderer("mesa llvmpipe"));
        assert!(software_renderer("software rasterizer warning"));
        assert!(!software_renderer("nvidia geforce rtx 4090"));
    }
    #[test]
    fn cdp_retains_only_events_used_by_acceptance_evidence() {
        assert!(retain_cdp_event(&serde_json::json!({
            "method": "Network.requestWillBeSent"
        })));
        assert!(retain_cdp_event(&serde_json::json!({
            "method": "Network.webSocketFrameError"
        })));
        assert!(!retain_cdp_event(&serde_json::json!({
            "method": "Runtime.consoleAPICalled"
        })));
        assert!(!retain_cdp_event(&serde_json::json!({
            "method": "Page.lifecycleEvent"
        })));
    }
    #[test]
    fn either_hardware_browser_api_satisfies_preflight() {
        let webgl_only = HardwareIdentity {
            user_agent: "Chrome".to_owned(),
            webgpu_vendor: "Google".to_owned(),
            webgpu_architecture: "SwiftShader".to_owned(),
            webgpu_device: String::new(),
            webgpu_description: String::new(),
            webgl_available: true,
            webgl_vendor: "Google Inc. (NVIDIA Corporation)".to_owned(),
            webgl_renderer: "ANGLE (NVIDIA GeForce RTX 4090)".to_owned(),
        };
        assert_eq!(webgl_only.hardware_apis().0, vec![BrowserGpuApi::WebGl]);
        webgl_only.validate().unwrap();

        let webgpu_only = HardwareIdentity {
            user_agent: "Chrome".to_owned(),
            webgpu_vendor: "NVIDIA".to_owned(),
            webgpu_architecture: "Lovelace".to_owned(),
            webgpu_device: "RTX 4090".to_owned(),
            webgpu_description: String::new(),
            webgl_available: false,
            webgl_vendor: String::new(),
            webgl_renderer: String::new(),
        };
        assert_eq!(webgpu_only.hardware_apis().0, vec![BrowserGpuApi::WebGpu]);
        webgpu_only.validate().unwrap();
    }
    #[test]
    fn browser_preflight_rejects_two_software_apis() {
        let software_only = HardwareIdentity {
            user_agent: "Chrome".to_owned(),
            webgpu_vendor: "Google".to_owned(),
            webgpu_architecture: "SwiftShader".to_owned(),
            webgpu_device: String::new(),
            webgpu_description: String::new(),
            webgl_available: true,
            webgl_vendor: "Google".to_owned(),
            webgl_renderer: "ANGLE (SwiftShader)".to_owned(),
        };
        assert!(software_only.hardware_apis().0.is_empty());
        assert!(software_only.validate().is_err());
    }
}
