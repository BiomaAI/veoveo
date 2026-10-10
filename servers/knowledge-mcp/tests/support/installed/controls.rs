//! Native input and official SDK lifecycle controls; no installed or GPU claims.
use super::*;
use rmcp::ServiceExt;
use std::{
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

pub(super) struct Scratch(pub(super) PathBuf);
impl Scratch {
    pub(super) fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("knowledge-policy-{}", uuid::Uuid::now_v7()));
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn registered_owner_drop_closes_actual_client_and_listener_without_empty_retry_success()
-> Result<()> {
    const MODE: &str = "VEOVEO_KNOWLEDGE_OWNER_CONTROL";
    if let Ok(mode) = std::env::var(MODE) {
        struct Source;
        impl rmcp::ServerHandler for Source {
            fn get_info(&self) -> ServerConfig {
                ServerConfig::new(
                    ServerCapabilities::builder()
                        .enable_resources()
                        .enable_resources_subscribe()
                        .enable_resources_list_changed()
                        .build(),
                )
            }
            fn accepted_subscription_filter(
                &self,
                filter: &SubscriptionFilter,
            ) -> Option<SubscriptionFilter> {
                Some(filter.clone())
            }
            async fn listen(
                &self,
                context: rmcp::service::SubscriptionContext,
            ) -> std::result::Result<(), rmcp::ErrorData> {
                context.cancelled().await;
                Ok(())
            }
        }
        let directory = Scratch::new()?;
        let output = directory.0.join("outcome.json");
        let journal = receipt::Journal::open(&output, "operator".parse()?)?;
        let count = Arc::new(AtomicUsize::new(0));
        let counted = count.clone();
        let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
        let failed = mode == "failed";
        let mut retained = None;
        let result: Result<()> = veoveo_testing_support::lifecycle::owner::run(async {
            let owned = cleanup::register(&journal)?;
            retained = Some(owned.clone());
            let (server_io, client_io) = tokio::io::duplex(8192);
            let server = tokio::spawn(async move { Source.serve(server_io).await });
            let mut handles = owned.lock().await;
            let slots = &mut *handles;
            for slot in [&mut slots.cold_watch, &mut slots.cold_forward] {
                let mut command = tokio::process::Command::new("sleep");
                command.arg("30");
                *slot = Some(veoveo_testing_support::spawn_async(command)?);
            }
            journal
                .acquire(&mut handles.caller, async {
                    ClientConfig::default()
                        .serve_with_lifecycle(
                            client_io,
                            rmcp::ClientLifecycleMode::Discover {
                                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                            },
                        )
                        .await
                        .map_err(anyhow::Error::from)
                })
                .await?;
            handles.opened_caller(&journal)?;
            let server = server.await??;
            tokio::spawn(async move {
                let _ = closed_tx.send(server.waiting().await.is_ok());
            });
            let peer = handles.caller.as_ref().unwrap().peer().clone();
            journal
                .listen(
                    &mut handles.listener,
                    &peer,
                    SubscriptionFilter::builder()
                        .resources_list_changed()
                        .build(),
                )
                .await?;
            handles.opened_listener(&journal)?;
            // Inject only the delay/error of close acknowledgement; the retained
            // future still owns and closes the real official SDK caller.
            handles.retain_control_close(counted, failed);
            if mode != "native-drop" {
                handles.close(&journal).await?;
            }
            std::future::pending::<Result<()>>().await
        })
        .await;
        if failed {
            retained
                .as_ref()
                .unwrap()
                .lock()
                .await
                .refuses_consumed_failed_control()
                .await?;
        }
        ensure!(
            result.is_err(),
            "cancelled or failed owner became successful"
        );
        ensure!(
            count.load(Ordering::SeqCst) == 1,
            "original consuming close future was replaced; attempts={} safe outcome={}",
            count.load(Ordering::SeqCst),
            fs::read_to_string(&output)?
        );
        ensure!(
            tokio::time::timeout(Duration::from_secs(1), closed_rx).await??,
            "actual MCP transport stayed open"
        );
        let text = fs::read_to_string(&output)?;
        let report: serde_json::Value = serde_json::from_str(&text)?;
        ensure!(
            report["listener"] == "closed" && report["nativeObservers"] == "closed",
            "actual SDK listener cleanup unproven"
        );
        ensure!(
            report["caller"] == if failed { "failed" } else { "closed" },
            "failed consumed slot became success"
        );
        ensure!(
            report["outcome"] != "passed"
                && !text.contains("bearer")
                && !text.contains("tokenFile"),
            "private outcome falsely passed or exposed credentials"
        );
        ensure!(
            fs::metadata(output)?.permissions().mode() & 0o077 == 0,
            "outcome was not private"
        );
        return Ok(());
    }
    for mode in ["interrupted", "failed", "native-drop"] {
        let directory = Scratch::new()?;
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            + 2000;
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command.args(["installed::controls::registered_owner_drop_closes_actual_client_and_listener_without_empty_retry_success","--exact","--nocapture"])
            .env(MODE,mode).env("VEOVEO_SMOKE_DEADLINE_UNIX_MS",deadline.to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS","4").env("VEOVEO_SMOKE_LOCAL_GROUPS",&directory.0);
        let output = veoveo_testing_support::output_async(command, Duration::from_secs(10)).await?;
        ensure!(
            output.status.success(),
            "Knowledge owner control failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[tokio::test]
async fn actual_http_initialization_denial_reaches_private_acquisition_status() -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    const SECRET: &str = "PRIVATE_LOOPBACK_AUTH_CHALLENGE_AND_BODY";
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let transport = StreamableHttpClientTransport::with_client(
        http,
        StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp")),
    );
    let directory = Scratch::new()?;
    let path = directory.0.join("outcome.json");
    let journal = receipt::Journal::open(&path, "operator".parse()?)?;
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async {
            let (mut socket, _) = listener.accept().await?;
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let count = socket.read(&mut buffer).await?;
                ensure!(count > 0 && request.len() + count <= 8192);
                request.extend_from_slice(&buffer[..count]);
            }
            ensure!(request.starts_with(b"POST /mcp HTTP/1.1\r\n"));
            let header_end = request.windows(4).position(|part| part == b"\r\n\r\n").unwrap() + 4;
            let headers = std::str::from_utf8(&request[..header_end])?;
            let content_length: usize = headers.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then_some(value.trim())
            }).context("loopback request lacks content length")?.parse()?;
            ensure!(header_end + content_length <= 8192);
            while request.len() < header_end + content_length {
                let count = socket.read(&mut buffer).await?;
                ensure!(count > 0 && request.len() + count <= 8192);
                request.extend_from_slice(&buffer[..count]);
            }
            let response = format!("HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer realm=\"{SECRET}\"\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SECRET}", SECRET.len());
            socket.write_all(response.as_bytes()).await?;
            socket.shutdown().await?;
            Ok::<_,anyhow::Error>(())
        }).await?
    });
    let mut slot = None;
    // Same SDK startup and typed-error conversion as the installed HTTPS path.
    let result = journal
        .acquire(&mut slot, super::connect_transport(transport))
        .await;
    let served = server.await?;
    served?;
    let error = result.unwrap_err();
    ensure!(slot.is_none() && !format!("{error:#}").contains(SECRET));
    let text = fs::read_to_string(path)?;
    ensure!(!text.contains(SECRET));
    let report: serde_json::Value = serde_json::from_str(&text)?;
    ensure!(
        report["requests"][0]["status"] == "http" && report["requests"][0]["httpStatus"] == 401
    );
    Ok(())
}

#[tokio::test]
async fn acquisition_retains_typed_initialization_denials_without_private_details() -> Result<()> {
    use rmcp::{
        service::ClientInitializeError,
        transport::{
            DynamicTransportError,
            streamable_http_client::{AuthRequiredError, StreamableHttpError},
        },
    };
    const SECRET: &str = "PRIVATE_INITIALIZATION_TOKEN_HEADER_BODY_MUST_NOT_APPEAR";
    for case in ["http401", "http403", "mcp", "unknown"] {
        let error = match case {
            "http401" => ClientInitializeError::TransportError {
                error: DynamicTransportError::from_parts(
                    "fixture",
                    std::any::TypeId::of::<()>(),
                    Box::new(AuthRequiredError::new(format!("Bearer {SECRET}"))),
                ),
                context: "send discover request".into(),
            },
            "http403" => ClientInitializeError::TransportError {
                error: DynamicTransportError::from_parts(
                    "fixture",
                    std::any::TypeId::of::<()>(),
                    Box::new(StreamableHttpError::<reqwest::Error>::HttpResponse {
                        status: reqwest::StatusCode::FORBIDDEN,
                        body: SECRET.into(),
                    }),
                ),
                context: "send discover request".into(),
            },
            "mcp" => ClientInitializeError::JsonRpcError(rmcp::ErrorData::invalid_request(
                SECRET,
                Some(serde_json::json!({"private":SECRET})),
            )),
            "unknown" => ClientInitializeError::ConnectionClosed(SECRET.into()),
            _ => unreachable!(),
        };
        // Exercise the exact initialization conversion used by installed connect,
        // then the real intent/acquisition/outcome path rather than just observe.
        let error = super::connection_failure(error);
        ensure!(
            error.downcast_ref::<ClientInitializeError>().is_some(),
            "typed initialization cause erased"
        );
        ensure!(error.to_string() == "Knowledge connection failed");
        let directory = Scratch::new()?;
        let path = directory.0.join("outcome.json");
        let journal = receipt::Journal::open(&path, "operator".parse()?)?;
        let mut slot = None;
        let failure = journal
            .acquire(&mut slot, async { Err(error) })
            .await
            .unwrap_err();
        ensure!(slot.is_none());
        ensure!(!format!("{failure:#}").contains(SECRET));
        let text = fs::read_to_string(&path)?;
        ensure!(!text.contains(SECRET));
        let report: serde_json::Value = serde_json::from_str(&text)?;
        let observation = &report["requests"][0];
        ensure!(report["requests"].as_array().unwrap().len() == 1);
        ensure!(observation["request"]["operation"] == "connect");
        match case {
            "http401" => ensure!(
                observation["status"] == "http"
                    && observation["httpStatus"] == 401
                    && observation["code"].is_null()
            ),
            "http403" => ensure!(
                observation["status"] == "http"
                    && observation["httpStatus"] == 403
                    && observation["code"].is_null()
            ),
            "mcp" => ensure!(observation["status"] == "mcp" && observation["code"] == -32600),
            "unknown" => ensure!(
                observation["status"] == "transport"
                    && observation["code"].is_null()
                    && observation["httpStatus"].is_null()
            ),
            _ => unreachable!(),
        }
    }
    Ok(())
}

#[tokio::test]
async fn journal_refuses_intent_write_failure_before_poll_and_redacts_protocol_failures()
-> Result<()> {
    let directory = Scratch::new()?;
    let path = directory.0.join("outcome.json");
    let journal = receipt::Journal::open(&path, "operator".parse()?)?;
    let error: Result<()> = journal
        .request(receipt::Request::ToolsList, async {
            Err(rmcp::ServiceError::McpError(
                rmcp::ErrorData::invalid_request("RAW_PRIVATE_ERROR_MUST_NOT_APPEAR", None),
            ))
        })
        .await;
    ensure!(error.is_err());
    let text = fs::read_to_string(&path)?;
    ensure!(text.contains("-32600") && !text.contains("RAW_PRIVATE_ERROR_MUST_NOT_APPEAR"));
    let called = std::cell::Cell::new(false);
    journal.break_writer_for_control()?;
    let result: Result<()> = journal
        .request(receipt::Request::ToolsList, async {
            called.set(true);
            Ok(())
        })
        .await;
    ensure!(
        result.is_err() && !called.get(),
        "failed intent write polled network work"
    );
    Ok(())
}
