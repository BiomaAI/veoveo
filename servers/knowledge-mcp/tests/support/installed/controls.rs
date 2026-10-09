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

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self> {
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
            handles.close(&journal).await?;
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
            report["listener"] == "closed",
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
    for mode in ["interrupted", "failed"] {
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
