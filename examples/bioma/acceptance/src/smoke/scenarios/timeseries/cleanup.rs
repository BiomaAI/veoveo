//! Retain actual SDK handles and their original close futures across cancellation.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    io::{Read, Seek, SeekFrom},
    pin::Pin,
    sync::Arc,
};
use tokio::sync::Mutex;
use veoveo_testing_support::lifecycle::owner::{self, CleanupKind, CleanupRegistration};

type Closing = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum CloseOutcome {
    Open,
    Pending,
    Closed,
    Failed,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Trace {
    connections: Vec<CloseOutcome>,
    subscriptions: Vec<CloseOutcome>,
}
impl Trace {
    fn connections_closed(&self) -> bool {
        self.connections
            .iter()
            .all(|state| *state == CloseOutcome::Closed)
    }
    fn subscriptions_closed(&self) -> bool {
        self.subscriptions
            .iter()
            .all(|state| *state == CloseOutcome::Closed)
    }
}
#[derive(Default, Clone, Copy)]
pub(super) enum Role {
    #[default]
    Lifecycle,
    Operator,
    Administrator,
}
#[derive(Default)]
pub(super) struct Handles {
    role: Role,
    pub client: Option<SmokeMcpClient>,
    pub state: TaskNotificationState,
    client_close: Option<Closing>,
    listener_close: Option<Closing>,
    trace: Trace,
    client_deadline: Option<tokio::time::Instant>,
    listener_deadline: Option<tokio::time::Instant>,
}
impl Handles {
    pub fn opened_client(&mut self, client: SmokeMcpClient) -> Result<()> {
        ensure!(
            self.client.is_none() && self.client_close.is_none() && self.trace.connections_closed(),
            "prior Timeseries connection close remains unresolved"
        );
        self.client_deadline = None;
        self.client = Some(client);
        self.trace.connections.push(CloseOutcome::Open);
        Ok(())
    }
    pub fn opened_subscription(&mut self) {
        self.listener_deadline = None;
        self.trace.subscriptions.push(CloseOutcome::Open);
    }
    pub fn sync(&self, receipt: &mut Receipt) {
        apply_trace(receipt, &self.trace, self.role);
    }
    pub fn closed(&self) -> bool {
        self.trace.connections_closed() && self.trace.subscriptions_closed()
    }
    pub async fn close_until(
        &mut self,
        end: tokio::time::Instant,
        file: &mut std::fs::File,
    ) -> Result<()> {
        // Persist Pending before consuming the actual SDK handle. The same future
        // stays in the retained slots if this caller or its deadline interrupts it.
        let mut listener_intent = Ok(());
        let mut caller_intent = Ok(());
        if self.listener_close.is_none() && self.state.subscription.is_some() {
            *self
                .trace
                .subscriptions
                .last_mut()
                .context("subscription opening omitted")? = CloseOutcome::Pending;
            listener_intent = write_trace(file, &self.trace, self.role);
            let mut stream = self.state.subscription.take().unwrap();
            self.listener_close = Some(Box::pin(async move {
                stream
                    .cancel()
                    .await
                    .map_err(|_| anyhow!("Timeseries subscription close failed"))
            }));
        }
        let listener = finish_close(
            &mut self.listener_close,
            self.trace.subscriptions.last_mut(),
            &mut self.listener_deadline,
            end,
        )
        .await;
        let listener_write = write_trace(file, &self.trace, self.role);
        if self.client_close.is_none() && self.client.is_some() {
            *self
                .trace
                .connections
                .last_mut()
                .context("connection opening omitted")? = CloseOutcome::Pending;
            caller_intent = write_trace(file, &self.trace, self.role);
            let client = self.client.take().unwrap();
            self.client_close = Some(Box::pin(async move {
                client
                    .cancel()
                    .await
                    .map_err(|_| anyhow!("Timeseries connection close failed"))
            }));
        }
        let caller = finish_close(
            &mut self.client_close,
            self.trace.connections.last_mut(),
            &mut self.client_deadline,
            end,
        )
        .await;
        let caller_write = write_trace(file, &self.trace, self.role);
        listener?;
        caller?;
        listener_intent?;
        caller_intent?;
        listener_write?;
        caller_write?;
        ensure!(
            self.closed(),
            "Timeseries SDK close history remains unresolved"
        );
        Ok(())
    }
}
async fn finish_close(
    pending: &mut Option<Closing>,
    state: Option<&mut CloseOutcome>,
    original_end: &mut Option<tokio::time::Instant>,
    end: tokio::time::Instant,
) -> Result<()> {
    let Some(future) = pending.as_mut() else {
        ensure!(
            !state
                .is_some_and(|state| matches!(state, CloseOutcome::Pending | CloseOutcome::Failed)),
            "Timeseries prior close remains unresolved"
        );
        return Ok(());
    };
    let state = state.context("close attempt omitted")?;
    let deadline = *original_end
        .get_or_insert_with(|| end.min(tokio::time::Instant::now() + Duration::from_secs(10)));
    let deadline = deadline.min(end);
    *original_end = Some(deadline);
    match tokio::time::timeout_at(deadline, future).await {
        Ok(result) => {
            *state = if result.is_ok() {
                CloseOutcome::Closed
            } else {
                CloseOutcome::Failed
            };
            pending.take();
            result
        }
        Err(_) => bail!("Timeseries SDK close deadline; original close remains retained"),
    }
}
fn write_trace(file: &mut std::fs::File, trace: &Trace, role: Role) -> Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "Timeseries cleanup receipt exceeds oneMiB"
    );
    let mut receipt: Receipt = serde_json::from_slice(&bytes)?;
    apply_trace(&mut receipt, trace, role);
    receipt.settle(false);
    persist(file, &receipt)
}
fn apply_trace(receipt: &mut Receipt, trace: &Trace, role: Role) {
    match role {
        Role::Lifecycle => {
            let journal = receipt.lifecycle.as_mut().expect("lifecycle journal");
            journal.cleanup = trace.clone();
            journal.connection_closed = trace.connections_closed();
            journal.subscription_closed = trace.subscriptions_closed();
            journal.passed = false;
        }
        Role::Operator => {
            receipt.operator_cleanup = trace.clone();
            receipt.operator_closed = trace.connections_closed();
            receipt.subscription_closed = trace.subscriptions_closed();
        }
        Role::Administrator => {
            receipt.administrator_cleanup = trace.clone();
            receipt.administrator_closed = trace.connections_closed();
        }
    }
}
pub(super) fn register(
    file: &std::fs::File,
    role: Role,
) -> Result<(Arc<Mutex<Handles>>, CleanupRegistration)> {
    let handles = Arc::new(Mutex::new(Handles {
        role,
        ..Handles::default()
    }));
    let retained = handles.clone();
    let mut writer = file.try_clone()?;
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "Timeseries SDK handles",
        &uuid::Uuid::now_v7().to_string(),
        move || async move {
            let end = tokio::time::Instant::from_std(owner::cleanup_deadline()?);
            retained.lock().await.close_until(end, &mut writer).await
        },
    )?;
    Ok((handles, registration))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn global_owner_drop_closes_original_sdk_slots_and_preserves_failed_history() -> Result<()>
    {
        const MODE: &str = "VEOVEO_TIMESERIES_CLEANUP_CONTROL";
        if let Ok(mode) = std::env::var(MODE) {
            use rmcp::{ClientServiceExt, ServiceExt};
            use std::sync::atomic::{AtomicUsize, Ordering};
            struct Source;
            impl rmcp::ServerHandler for Source {
                fn get_info(&self) -> rmcp::model::ServerConfig {
                    rmcp::model::ServerConfig::new(
                        rmcp::model::ServerCapabilities::builder()
                            .enable_tasks()
                            .build(),
                    )
                }
                fn accepted_subscription_filter(
                    &self,
                    filter: &rmcp::model::SubscriptionFilter,
                ) -> Option<rmcp::model::SubscriptionFilter> {
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
            let mut file = tempfile::tempfile()?;
            let mut receipt = Receipt::new(forecast_request()?);
            let role = if mode == "lifecycle" {
                Role::Lifecycle
            } else {
                Role::Operator
            };
            if mode == "lifecycle" {
                receipt.lifecycle = Some(serde_json::from_value(serde_json::json!({
                    "input":{"mode":"cancel","request":forecast_request()?},"taskId":null,"createdAt":null,"observations":[],
                    "cleanup":{"connections":[],"subscriptions":[]},"output":null,"artifactDigest":null,
                    "subscriptionClosed":false,"connectionClosed":false,"passed":false
                }))?);
            }
            persist(&mut file, &receipt)?;
            let attempts = Arc::new(AtomicUsize::new(0));
            let counted = attempts.clone();
            let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
            let failed = mode == "failed";
            let result: Result<()> = owner::run(async {
                let (owned, _registration) = register(&file, role)?;
                let (server_io, client_io) = tokio::io::duplex(8192);
                let server = tokio::spawn(async move { Source.serve(server_io).await });
                let client = veoveo_testing_support::SmokeMcpHandler
                    .serve_with_lifecycle(
                        client_io,
                        rmcp::ClientLifecycleMode::Discover {
                            preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                        },
                    )
                    .await?;
                let server = server.await??;
                tokio::spawn(async move {
                    let _ = closed_tx.send(server.waiting().await.is_ok());
                });
                let mut handles = owned.lock().await;
                handles.state.subscription = Some(
                    client
                        .peer()
                        .listen(
                            rmcp::model::SubscriptionFilter::builder()
                                .task_ids(["forecast-control"])
                                .build(),
                        )
                        .await?,
                );
                handles.opened_subscription();
                // This is the actual consuming SDK future, retained exactly as
                // production retains SmokeMcpClient::cancel after consumption.
                handles.client_close = Some(Box::pin(async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    client.cancel().await?;
                    ensure!(!failed, "controlled absent close acknowledgement");
                    Ok(())
                }));
                handles.trace.connections.push(CloseOutcome::Pending);
                handles.sync(&mut receipt);
                persist(&mut file, &receipt)?;
                let end = tokio::time::Instant::from_std(owner::cleanup_deadline()?);
                // Global owner expiry interrupts this first poll. Its registered
                // callback must resume the very same future and close listener.
                handles.close_until(end, &mut file).await?;
                std::future::pending::<Result<()>>().await
            })
            .await;
            ensure!(
                result.is_err(),
                "dropped/failed operation became successful"
            );
            ensure!(
                attempts.load(Ordering::SeqCst) == 1,
                "actual SDK close was recreated or omitted"
            );
            ensure!(
                tokio::time::timeout(Duration::from_secs(1), closed_rx).await??,
                "SDK transport stayed open"
            );
            file.seek(SeekFrom::Start(0))?;
            let final_receipt: Receipt = serde_json::from_reader(&mut file)?;
            let trace = if mode == "lifecycle" {
                &final_receipt.lifecycle.as_ref().unwrap().cleanup
            } else {
                &final_receipt.operator_cleanup
            };
            ensure!(
                trace.subscriptions_closed(),
                "actual listener was not closed"
            );
            ensure!(
                trace.connections_closed() != failed,
                "failed close history was erased"
            );
            return Ok(());
        }
        for mode in ["operator", "lifecycle", "failed"] {
            let directory = tempfile::tempdir()?;
            let deadline = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
                + 250;
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "global_owner_drop_closes_original_sdk_slots_and_preserves_failed_history",
                    "--nocapture",
                ])
                .env(MODE, mode)
                .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.to_string())
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2")
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path());
            let output =
                veoveo_testing_support::output_async(command, Duration::from_secs(10)).await?;
            ensure!(
                output.status.success(),
                "Timeseries owner cleanup {mode} failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }
    #[tokio::test]
    async fn interrupted_close_retains_original_future_and_empty_retry_cannot_erase_failure()
    -> Result<()> {
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut pending: Option<Closing> = Some(Box::pin(async move {
            receive.await?;
            Ok(())
        }));
        let mut state = CloseOutcome::Pending;
        let mut original_end = None;
        let end = tokio::time::Instant::now() + Duration::from_secs(1);
        assert!(
            tokio::time::timeout(
                Duration::from_millis(1),
                finish_close(&mut pending, Some(&mut state), &mut original_end, end)
            )
            .await
            .is_err()
        );
        assert!(state == CloseOutcome::Pending && pending.is_some());
        let captured = original_end;
        send.send(()).unwrap();
        finish_close(
            &mut pending,
            Some(&mut state),
            &mut original_end,
            end + Duration::from_secs(1),
        )
        .await?;
        assert!(state == CloseOutcome::Closed && pending.is_none() && original_end == captured);
        state = CloseOutcome::Failed;
        assert!(
            finish_close(&mut pending, Some(&mut state), &mut original_end, end)
                .await
                .is_err()
        );
        assert!(state == CloseOutcome::Failed);
        state = CloseOutcome::Pending;
        assert!(
            finish_close(&mut pending, Some(&mut state), &mut original_end, end)
                .await
                .is_err()
        );
        Ok(())
    }
}
