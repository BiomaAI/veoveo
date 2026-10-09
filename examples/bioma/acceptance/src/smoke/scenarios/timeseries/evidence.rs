//! Caller-owned journal outlives operation cancellation and SDK cleanup.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::{Seek, SeekFrom, Write};
use veoveo_mcp_conformance::client::failure::ObservedFailure;
use veoveo_timeseries_mcp::contract::TimeseriesForecastOutput;
use veoveo_types::{CanonicalTaskId, ResourceUri, Sha256Digest, TaskId};

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Outcome {
    #[vocabulary(rename = "not_dispatched")]
    NotDispatched,
    #[vocabulary(rename = "mutation_unresolved")]
    MutationUnresolved,
    #[vocabulary(rename = "task_unresolved")]
    TaskUnresolved,
    #[vocabulary(rename = "observed_failure")]
    ObservedFailure,
    #[vocabulary(rename = "passed")]
    Passed,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum ReadOutcome {
    Pending,
    Received { digest: Sha256Digest },
    Mcp { failure: ObservedFailure },
    Transport,
    Timeout,
    Interrupted,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReadObservation {
    pub target: ResourceUri,
    pub outcome: ReadOutcome,
}
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "method",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(super) enum CatalogMembers {
    #[serde(rename = "tools/list")]
    ToolsList {
        expected: Vec<veoveo_gateway_contract::GatewayToolName>,
        actual: Option<Vec<veoveo_gateway_contract::GatewayToolName>>,
    },
    #[serde(rename = "resources/templates/list")]
    ResourceTemplatesList {
        expected: Vec<veoveo_types::ResourceTemplateUri>,
        actual: Option<Vec<veoveo_types::ResourceTemplateUri>>,
    },
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum CatalogOutcome {
    Pending,
    Received,
    Failure { observed: ObservedFailure },
    Transport,
    RequestFailed,
    Timeout,
    Interrupted,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CatalogObservation {
    pub endpoint: veoveo_gateway_contract::ProtectedResourceId,
    pub members: CatalogMembers,
    pub outcome: CatalogOutcome,
    pub response_digest: Option<Sha256Digest>,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum ReceiptSchema {
    #[vocabulary(rename = "veoveo.ai/installed-timeseries/v2")]
    V2,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Receipt {
    schema: ReceiptSchema,
    pub request: TimeseriesForecastRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<super::lifecycle::Journal>,
    pub outcome: Outcome,
    pub task_id: Option<CanonicalTaskId>,
    pub native_task_id: Option<TaskId>,
    pub completed_payload: Option<rmcp::model::CallToolResult>,
    pub output: Option<TimeseriesForecastOutput>,
    pub artifact_digest: Option<Sha256Digest>,
    pub reads: Vec<ReadObservation>,
    pub catalogs: Vec<CatalogObservation>,
    pub usage_member: bool,
    pub foreign_usage_denied: bool,
    #[serde(default)]
    pub operator_cleanup: super::cleanup::Trace,
    #[serde(default)]
    pub administrator_cleanup: super::cleanup::Trace,
    pub subscription_closed: bool,
    pub operator_closed: bool,
    pub administrator_closed: bool,
}
impl Receipt {
    pub fn new(request: TimeseriesForecastRequest) -> Self {
        Self {
            schema: ReceiptSchema::V2,
            request,
            lifecycle: None,
            outcome: Outcome::NotDispatched,
            task_id: None,
            native_task_id: None,
            completed_payload: None,
            output: None,
            artifact_digest: None,
            reads: Vec::new(),
            catalogs: Vec::new(),
            usage_member: false,
            foreign_usage_denied: false,
            operator_cleanup: Default::default(),
            administrator_cleanup: Default::default(),
            subscription_closed: false,
            operator_closed: false,
            administrator_closed: false,
        }
    }
    pub fn observed(&mut self, event: TaskNotificationObservation<'_>) {
        match event {
            TaskNotificationObservation::Admitted(id) => {
                self.task_id = Some(id.clone());
                self.outcome = Outcome::TaskUnresolved;
            }
            TaskNotificationObservation::Completed { task_id, payload } => {
                self.task_id = Some(task_id.clone());
                self.completed_payload = Some(payload.clone());
                self.outcome = Outcome::ObservedFailure;
            }
        }
    }
    pub fn settle(&mut self, operation_ok: bool) {
        for catalog in &mut self.catalogs {
            if matches!(catalog.outcome, CatalogOutcome::Pending) {
                catalog.outcome = CatalogOutcome::Interrupted;
            }
        }
        for read in &mut self.reads {
            if matches!(read.outcome, ReadOutcome::Pending) {
                read.outcome = ReadOutcome::Interrupted;
            }
        }
        if self.completed_payload.is_some() {
            self.outcome = if operation_ok
                && self.subscription_closed
                && self.operator_closed
                && self.administrator_closed
            {
                Outcome::Passed
            } else {
                Outcome::ObservedFailure
            };
        }
    }
}
pub(super) fn persist(file: &mut std::fs::File, receipt: &Receipt) -> Result<()> {
    let bytes = serde_json::to_vec(receipt)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "Timeseries receipt exceeds one MiB"
    );
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len().try_into()?)?;
    file.flush()?;
    file.sync_data()?;
    Ok(())
}

/// Intent is persisted before polling the SDK collector; observations precede assertions.
pub(super) async fn catalog<T: Serialize>(
    file: &mut std::fs::File,
    receipt: &mut Receipt,
    endpoint: &veoveo_gateway_contract::ProtectedResourceId,
    members: CatalogMembers,
    request: impl std::future::Future<Output = Result<T>>,
) -> Result<(usize, T)> {
    ensure!(
        receipt.catalogs.len() < 2,
        "Timeseries catalog request budget exhausted"
    );
    let index = receipt.catalogs.len();
    receipt.catalogs.push(CatalogObservation {
        endpoint: endpoint.clone(),
        members,
        outcome: CatalogOutcome::Pending,
        response_digest: None,
    });
    persist(file, receipt)?;
    match tokio::time::timeout(Duration::from_secs(30), request).await {
        Ok(Ok(value)) => {
            let bytes = serde_json::to_vec(&value)?;
            receipt.catalogs[index].response_digest = Some(Sha256Digest::from_bytes(
                <sha2::Sha256 as sha2::Digest>::digest(bytes).into(),
            ));
            receipt.catalogs[index].outcome = CatalogOutcome::Received;
            persist(file, receipt)?;
            Ok((index, value))
        }
        Ok(Err(error)) => {
            receipt.catalogs[index].outcome = catalog_failure(&error);
            persist(file, receipt)?;
            bail!("Timeseries catalog request failed; see private receipt")
        }
        Err(_) => {
            receipt.catalogs[index].outcome = CatalogOutcome::Timeout;
            persist(file, receipt)?;
            bail!("Timeseries catalog request exceeded 30 seconds; see private receipt")
        }
    }
}
fn catalog_failure(error: &anyhow::Error) -> CatalogOutcome {
    if let Some(service_error) = error.downcast_ref::<rmcp::ServiceError>() {
        return match service_error {
            rmcp::ServiceError::McpError(error) => CatalogOutcome::Failure {
                observed: ObservedFailure::mcp(i64::from(error.code.0), error.message.as_ref()),
            },
            rmcp::ServiceError::Timeout { .. } => CatalogOutcome::Timeout,
            rmcp::ServiceError::Cancelled { .. } => CatalogOutcome::Interrupted,
            rmcp::ServiceError::TransportSend(_) | rmcp::ServiceError::TransportClosed => {
                match veoveo_mcp_conformance::client::failure::observe(error) {
                    Some(observed @ ObservedFailure::Http { .. }) => {
                        CatalogOutcome::Failure { observed }
                    }
                    _ => CatalogOutcome::Transport,
                }
            }
            _ => CatalogOutcome::RequestFailed,
        };
    }
    if error
        .downcast_ref::<tokio::time::error::Elapsed>()
        .is_some()
    {
        return CatalogOutcome::Timeout;
    }
    match veoveo_mcp_conformance::client::failure::observe(error) {
        Some(observed @ ObservedFailure::Http { .. }) => CatalogOutcome::Failure { observed },
        _ => CatalogOutcome::RequestFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn catalog_errors_persist_only_observed_codes_and_transport_classes() -> Result<()> {
        let message = "isolated-private-sentinel";
        let mcp = rmcp::ErrorData::invalid_params(message, None);
        let nested =
            rmcp::ServiceError::TransportSend(rmcp::transport::DynamicTransportError::from_parts(
                "isolated-test-transport",
                std::any::TypeId::of::<()>(),
                Box::new(mcp.clone()),
            ));
        let elapsed = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>())
            .await
            .unwrap_err();
        let cases = [
            (
                anyhow::Error::new(rmcp::ServiceError::McpError(mcp)),
                CatalogOutcome::Failure {
                    observed: ObservedFailure::mcp(-32602, message),
                },
            ),
            (anyhow::Error::new(nested), CatalogOutcome::Transport),
            (anyhow!(message), CatalogOutcome::RequestFailed),
            (anyhow::Error::new(elapsed), CatalogOutcome::Timeout),
        ];
        for (error, expected) in cases {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("receipt.json");
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            let mut receipt = Receipt::new(forecast_request()?);
            let endpoint = veoveo_gateway_contract::ProtectedResourceId::parse(
                "https://fixture.example/mcp/initial",
            )?;
            let result = catalog(
                &mut file,
                &mut receipt,
                &endpoint,
                CatalogMembers::ToolsList {
                    expected: vec![veoveo_gateway_contract::GatewayToolName::parse(
                        "timeseries__forecast",
                    )?],
                    actual: None,
                },
                std::future::ready(Err::<Vec<rmcp::model::Tool>, _>(error)),
            )
            .await;
            assert!(result.is_err());
            let bytes = std::fs::read(path)?;
            assert!(!std::str::from_utf8(&bytes)?.contains(message));
            let saved: Receipt = serde_json::from_slice(&bytes)?;
            assert!(saved.catalogs[0].outcome == expected);
            assert!(saved.catalogs[0].response_digest.is_none());
            assert!(saved.outcome == Outcome::NotDispatched && saved.task_id.is_none());
        }
        Ok(())
    }
    #[test]
    fn settlement_preserves_uncertain_task_and_refuses_cleanup_success_claim() -> Result<()> {
        let mut receipt = Receipt::new(forecast_request()?);
        receipt.outcome = Outcome::MutationUnresolved;
        let target = TimeseriesTaskUsageUri::new(TaskId::new())?.to_uri()?;
        receipt.reads.push(ReadObservation {
            target: target.clone(),
            outcome: ReadOutcome::Pending,
        });
        receipt.reads.push(ReadObservation {
            target,
            outcome: ReadOutcome::Timeout,
        });
        receipt.settle(false);
        assert!(matches!(receipt.reads[0].outcome, ReadOutcome::Interrupted));
        assert!(matches!(receipt.reads[1].outcome, ReadOutcome::Timeout));
        assert!(receipt.outcome == Outcome::MutationUnresolved);
        let id = CanonicalTaskId::parse("gateway-known-forecast")?;
        receipt.observed(TaskNotificationObservation::Admitted(&id));
        receipt.settle(false);
        assert!(
            receipt.outcome == Outcome::TaskUnresolved && receipt.task_id.as_ref() == Some(&id)
        );
        let payload = rmcp::model::CallToolResult::success(vec![]);
        receipt.observed(TaskNotificationObservation::Completed {
            task_id: &id,
            payload: &payload,
        });
        receipt.subscription_closed = true;
        receipt.operator_closed = true;
        receipt.settle(true);
        assert!(receipt.outcome == Outcome::ObservedFailure);
        receipt.administrator_closed = true;
        receipt.settle(true);
        assert!(receipt.outcome == Outcome::Passed);
        receipt.settle(false);
        assert!(receipt.outcome == Outcome::ObservedFailure);
        Ok(())
    }
    #[tokio::test]
    async fn timeout_and_receipt_write_failure_preserve_owned_subscription_cleanup() -> Result<()> {
        use rmcp::model::SubscriptionFilter;
        use rmcp::{ClientServiceExt, ServerHandler, ServiceExt};
        use std::sync::Arc;
        use tokio::sync::Notify;
        #[derive(Clone)]
        struct Source {
            entered: Arc<Notify>,
            cancelled: Arc<Notify>,
        }
        impl ServerHandler for Source {
            fn get_info(&self) -> rmcp::model::ServerConfig {
                rmcp::model::ServerConfig::new(
                    rmcp::model::ServerCapabilities::builder()
                        .enable_tasks()
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
                self.entered.notify_one();
                context.cancelled().await;
                self.cancelled.notify_one();
                Ok(())
            }
        }
        for write_failure in [false, true] {
            let source = Source {
                entered: Arc::new(Notify::new()),
                cancelled: Arc::new(Notify::new()),
            };
            let handler = source.clone();
            let (server_io, client_io) = tokio::io::duplex(8192);
            let server = tokio::spawn(async move { handler.serve(server_io).await });
            let client = veoveo_testing_support::SmokeMcpHandler
                .serve_with_lifecycle(
                    client_io,
                    rmcp::ClientLifecycleMode::Discover {
                        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                    },
                )
                .await?;
            let server = server.await??;
            let mut state = TaskNotificationState::default();
            let mut receipt = Receipt::new(forecast_request()?);
            let id = CanonicalTaskId::parse("gateway-known-forecast")?;
            receipt.observed(TaskNotificationObservation::Admitted(&id));
            state.subscription = Some(
                tokio::time::timeout(
                    Duration::from_secs(2),
                    client.peer().listen(
                        SubscriptionFilter::builder()
                            .task_ids([id.as_str()])
                            .build(),
                    ),
                )
                .await??,
            );
            tokio::time::timeout(Duration::from_secs(2), source.entered.notified()).await?;
            let operation = tokio::time::timeout(Duration::from_millis(1), async {
                if write_failure {
                    let mut file = std::fs::OpenOptions::new().write(true).open("/dev/full")?;
                    persist(&mut file, &receipt)?;
                }
                std::future::pending::<Result<()>>().await
            })
            .await;
            assert!(operation.is_err() || matches!(operation, Ok(Err(_))));
            assert!(state.subscription.is_some());
            state.close().await?;
            receipt.subscription_closed = state.listener_closed;
            receipt.settle(false);
            assert!(
                receipt.outcome == Outcome::TaskUnresolved && receipt.task_id.as_ref() == Some(&id)
            );
            tokio::time::timeout(Duration::from_secs(2), source.cancelled.notified()).await?;
            tokio::time::timeout(Duration::from_secs(2), client.cancel()).await??;
            tokio::time::timeout(Duration::from_secs(2), server.cancel()).await??;
        }
        Ok(())
    }
}
