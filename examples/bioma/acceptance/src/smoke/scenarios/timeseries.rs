//! One installed CPU forecast through the existing acceptance harness.
use super::*;
use anyhow::ensure;
use std::collections::BTreeSet;
use veoveo_duckdb_mcp::contract::{DuckDbReadOptions, DuckDbTabularSource};
use veoveo_timeseries_mcp::contract::{
    TimeseriesForecastHorizon, TimeseriesForecastRequest, TimeseriesTableMapping,
};

fn forecast_request() -> Result<TimeseriesForecastRequest> {
    Ok(TimeseriesForecastRequest::new(
        DuckDbTabularSource::InlineCsv {
            csv: "value\n1\n2\n3\n4\n".into(),
            filename: Some("installed-forecast.csv".into()),
            options: DuckDbReadOptions::default().with_header(true),
        },
        TimeseriesTableMapping::new("value".parse()?),
        TimeseriesForecastHorizon::new(2)?,
    ))
}

#[path = "timeseries/assertions.rs"]
mod assertions;
#[path = "timeseries/cleanup.rs"]
mod cleanup;
#[path = "timeseries/evidence.rs"]
mod evidence;
#[path = "timeseries/lifecycle.rs"]
mod lifecycle;
#[path = "timeseries/reads.rs"]
mod reads;
use evidence::{Outcome, Receipt, persist};
use std::os::unix::fs::OpenOptionsExt;
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_timeseries_mcp::contract::{TimeseriesForecastOutput, TimeseriesTaskUsageUri};
use veoveo_types::{ResourceAddress, ResourceUri};

pub(crate) async fn timeseries_installed(
    installation: &support::InstalledTarget,
    evidence_path: &Path,
    lifecycle_input: Option<&Path>,
) -> Result<()> {
    let lifecycle_input = lifecycle_input.map(lifecycle::Input::load).transpose()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    let administrator = installation.administrator()?;
    ensure!(
        administrator.principal != installation.operator.principal
            && administrator.profile != installation.operator.profile
            && administrator.resource != installation.operator.resource,
        "Timeseries visibility requires a distinct administrator identity/profile"
    );
    ensure!(
        evidence_path.is_absolute(),
        "Timeseries receipt requires an absolute path"
    );
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(evidence_path)
        .context("Timeseries receipt requires a new private file")?;
    let mut receipt = Receipt::new(forecast_request()?);
    persist(&mut file, &receipt)?;
    let (operator_owned, operator_registration) =
        cleanup::register(&file, cleanup::Role::Operator)?;
    let (foreign_owned, foreign_registration) =
        cleanup::register(&file, cleanup::Role::Administrator)?;
    let mut cleanup_file = file.try_clone()?;
    let result = tokio::time::timeout_at(deadline, async {
        let mut operator = operator_owned.lock().await;
        let mut foreign = foreign_owned.lock().await;
        let token = tokio::time::timeout(Duration::from_secs(15), installation.token())
            .await
            .map_err(|_| anyhow!("Timeseries operator OAuth deadline"))?
            .map_err(|_| anyhow!("Timeseries operator OAuth admission failed"))?;
        operator.opened_client(
            tokio::time::timeout(
                Duration::from_secs(15),
                connect_mcp_client(installation.operator.resource.as_str(), &token),
            )
            .await
            .map_err(|_| anyhow!("Timeseries operator connection deadline"))?
            .map_err(|_| anyhow!("Timeseries operator connection failed"))?,
        )?;
        operator.sync(&mut receipt);
        persist(&mut file, &receipt)?;
        let token = tokio::time::timeout(Duration::from_secs(15), administrator.token())
            .await
            .map_err(|_| anyhow!("Timeseries administrator OAuth deadline"))?
            .map_err(|_| anyhow!("Timeseries administrator OAuth admission failed"))?;
        foreign.opened_client(
            tokio::time::timeout(
                Duration::from_secs(15),
                connect_mcp_client(administrator.resource.as_str(), &token),
            )
            .await
            .map_err(|_| anyhow!("Timeseries administrator connection deadline"))?
            .map_err(|_| anyhow!("Timeseries administrator connection failed"))?,
        )?;
        foreign.sync(&mut receipt);
        persist(&mut file, &receipt)?;
        let client = operator
            .client
            .as_ref()
            .expect("operator connection admitted");
        admit(
            client,
            &installation.operator.resource,
            &mut file,
            &mut receipt,
        )
        .await?;
        let existing = reads::usage_ids(client, &mut file, &mut receipt).await?;
        ensure!(
            existing.len() <= 3000,
            "Timeseries usage leaves insufficient fixture traversal budget"
        );
        let request = receipt.request.clone();
        let arguments = serde_json::to_value(&request)?;
        receipt.outcome = Outcome::MutationUnresolved;
        if let Err(error) = persist(&mut file, &receipt) {
            // This invocation has not dispatched anything when intent persistence fails.
            receipt.outcome = Outcome::NotDispatched;
            return Err(error);
        }
        let (_task_id, payload) =
            complete_baseline(&mut operator, arguments, deadline, &mut file, &mut receipt).await?;
        let client = operator.client.as_ref().unwrap();
        let output: TimeseriesForecastOutput = serde_json::from_value(
            payload
                .structured_content
                .context("Timeseries completed Task omitted structured output")?,
        )
        .map_err(|_| anyhow!("Timeseries completed output failed owner admission"))?;
        receipt.output = Some(output.clone());
        persist(&mut file, &receipt)?;
        let metadata = assertions::metadata(&request, &output)?;
        receipt.native_task_id = Some(metadata.task_id);
        persist(&mut file, &receipt)?;
        ensure!(
            !existing.contains(&metadata.task_id),
            "Timeseries native Task identity already existed"
        );
        let metadata_uri =
            veoveo_artifact_mcp::contract::metadata_uri(output.artifact.artifact_id());
        let current: ArtifactMetadata =
            reads::json(client, &metadata_uri, &mut file, &mut receipt).await?;
        assertions::current_metadata(&output, &current)?;
        let timeseries_bytes = reads::blob(
            client,
            &output.result_uri.to_uri()?,
            &mut file,
            &mut receipt,
        )
        .await?;
        let artifact_uri = veoveo_artifact_mcp::contract::ArtifactResource::Occurrence(
            output.artifact.artifact_id(),
        )
        .to_uri();
        let artifact_bytes = reads::blob(client, &artifact_uri, &mut file, &mut receipt).await?;
        receipt.artifact_digest = Some(assertions::recording(
            &request,
            &output,
            &metadata,
            &timeseries_bytes,
            &artifact_bytes,
        )?);
        persist(&mut file, &receipt)?;
        let usage_uri = TimeseriesTaskUsageUri::new(metadata.task_id)?.to_uri()?;
        let usage: veoveo_mcp_contract::UsageReport =
            reads::json(client, &usage_uri, &mut file, &mut receipt).await?;
        require_usage(&usage, metadata.task_id, &usage_uri)?;
        let ids = reads::usage_ids(client, &mut file, &mut receipt).await?;
        ensure!(
            ids.contains(&metadata.task_id),
            "Timeseries usage pages omit the actual forecast Task"
        );
        receipt.usage_member = true;
        persist(&mut file, &receipt)?;
        let reply = reads::fetch(
            foreign
                .client
                .as_ref()
                .expect("administrator connection admitted"),
            &usage_uri,
            &mut file,
            &mut receipt,
        )
        .await?;
        let expected = veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
            -32602,
            format!("unknown usage task '{}'", metadata.task_id),
        );
        ensure!(
            matches!(reply,reads::Reply::Denied(actual) if actual==expected),
            "Timeseries foreign usage denial differed; see private receipt"
        );
        receipt.foreign_usage_denied = true;
        persist(&mut file, &receipt)?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|_| anyhow!("Timeseries operation exceeded300seconds; see retained receipt"))
    .and_then(|result| result);
    // The SDK handles and journal stay owned after operation cancellation. Cleanup
    // never cancels or redispatches an uncertain domain Task.
    let end = tokio::time::Instant::from_std(
        veoveo_testing_support::lifecycle::owner::cleanup_deadline()?,
    );
    let mut operator = operator_owned.lock().await;
    let operator_close = operator.close_until(end, &mut cleanup_file).await;
    operator.sync(&mut receipt);
    let mut foreign = foreign_owned.lock().await;
    let foreign_close = foreign.close_until(end, &mut cleanup_file).await;
    foreign.sync(&mut receipt);
    if operator.closed() {
        operator_registration.settled()?;
    }
    if foreign.closed() {
        foreign_registration.settled()?;
    }
    settle_baseline(
        &mut file,
        &mut receipt,
        result,
        operator_close,
        foreign_close,
    )?;
    if let Some(input) = lifecycle_input {
        let lifecycle_result = lifecycle::run(installation, input, &mut file, &mut receipt).await;
        receipt.settle(lifecycle_result.is_ok());
        persist(&mut file, &receipt)?;
        lifecycle_result?;
    }

    Ok(())
}
fn settle_baseline(
    file: &mut std::fs::File,
    receipt: &mut Receipt,
    operation: Result<()>,
    operator: Result<()>,
    administrator: Result<()>,
) -> Result<()> {
    receipt.settle(operation.is_ok() && operator.is_ok() && administrator.is_ok());
    persist(file, receipt)?;
    operation?;
    operator?;
    administrator?;
    ensure!(
        receipt.operator_closed && receipt.administrator_closed && receipt.subscription_closed,
        "Timeseries baseline SDK cleanup unresolved"
    );
    Ok(())
}
#[cfg(test)]
mod baseline_tests {
    use super::*;
    #[test]
    fn baseline_failure_or_unproven_cleanup_blocks_second_dispatch() -> Result<()> {
        for failed in 0..5 {
            let mut receipt = Receipt::new(forecast_request()?);
            receipt.operator_closed = true;
            receipt.administrator_closed = true;
            receipt.subscription_closed = failed != 3;
            let mut file = if failed == 4 {
                std::fs::OpenOptions::new().write(true).open("/dev/full")?
            } else {
                tempfile::tempfile()?
            };
            let gate = settle_baseline(
                &mut file,
                &mut receipt,
                if failed == 0 {
                    Err(anyhow!("operation failed"))
                } else {
                    Ok(())
                },
                if failed == 1 {
                    Err(anyhow!("operator close failed"))
                } else {
                    Ok(())
                },
                if failed == 2 {
                    Err(anyhow!("administrator close failed"))
                } else {
                    Ok(())
                },
            );
            let dispatched = std::cell::Cell::new(false);
            let result = gate.map(|()| dispatched.set(true));
            assert!(result.is_err() && !dispatched.get());
        }
        Ok(())
    }
}
async fn complete_baseline(
    handles: &mut cleanup::Handles,
    arguments: Value,
    deadline: tokio::time::Instant,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<(veoveo_types::CanonicalTaskId, rmcp::model::CallToolResult)> {
    tokio::time::timeout_at(
        deadline.min(tokio::time::Instant::now() + Duration::from_secs(180)),
        async {
            let admitted = call_tool_as_task(
                handles.client.as_ref().unwrap(),
                "timeseries__forecast",
                arguments,
            )
            .await
            .map_err(|_| anyhow!("Timeseries baseline dispatch unresolved"))?;
            let id = veoveo_types::CanonicalTaskId::parse(&admitted.task_id)?;
            receipt.observed(TaskNotificationObservation::Admitted(&id));
            persist(file, receipt)?;
            lifecycle::open_listener(handles, &id).await?;
            handles.sync(receipt);
            persist(file, receipt)?;
            let mut completed = None;
            for _ in 0..60 {
                let task =
                    lifecycle::notification(&mut handles.state, &id, &admitted.created_at).await?;
                match task.status() {
                    rmcp::model::TaskStatus::Completed => {
                        completed = Some(task);
                        break;
                    }
                    rmcp::model::TaskStatus::Working | rmcp::model::TaskStatus::InputRequired => {}
                    _ => bail!("Timeseries baseline Task did not complete"),
                }
            }
            let completed = completed.context("Timeseries baseline delivery budget exhausted")?;
            let current = handles
                .client
                .as_ref()
                .unwrap()
                .get_task(rmcp::model::GetTaskParams::new(id.to_string()))
                .await
                .map_err(|_| anyhow!("Timeseries baseline current Task read failed"))?;
            ensure!(
                current.task == completed,
                "Timeseries baseline delivered/current Tasks disagree"
            );
            let payload = task_payload(handles.client.as_ref().unwrap(), id.as_str())
                .await
                .map_err(|_| anyhow!("Timeseries baseline payload read failed"))?;
            ensure!(
                payload.is_error != Some(true),
                "Timeseries baseline tool error"
            );
            receipt.observed(TaskNotificationObservation::Completed {
                task_id: &id,
                payload: &payload,
            });
            persist(file, receipt)?;
            Ok((id, payload))
        },
    )
    .await
    .context("Timeseries baseline delivery deadline")?
}
async fn admit(
    client: &SmokeMcpClient,
    endpoint: &veoveo_gateway_contract::ProtectedResourceId,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<()> {
    ensure!(
        client
            .peer_info()
            .is_some_and(|info| info.capabilities.supports_tasks()),
        "Timeseries gateway does not advertise official Tasks"
    );
    let tools = evidence::CatalogMembers::ToolsList {
        expected: vec![veoveo_gateway_contract::GatewayToolName::parse(
            "timeseries__forecast",
        )?],
        actual: None,
    };
    let (index, tools) = evidence::catalog(
        file,
        receipt,
        endpoint,
        tools,
        veoveo_mcp_conformance::catalog::tools(client.peer()),
    )
    .await?;
    let actual = tools
        .iter()
        .map(|tool| veoveo_gateway_contract::GatewayToolName::parse(tool.name.as_ref()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| {
            anyhow!("Timeseries catalog contains invalid tool identities; see private receipt")
        })?;
    observe_catalog(file, receipt, index, |members| match members {
        evidence::CatalogMembers::ToolsList { actual: stored, .. } => *stored = Some(actual),
        _ => unreachable!("owning tools request"),
    })?;
    let templates = evidence::CatalogMembers::ResourceTemplatesList {
        expected: [
            veoveo_artifact_mcp::contract::METADATA_TEMPLATE,
            veoveo_artifact_mcp::contract::ARTIFACT_TEMPLATE,
        ]
        .into_iter()
        .map(veoveo_types::ResourceTemplateUri::new)
        .collect::<std::result::Result<Vec<_>, _>>()?,
        actual: None,
    };
    let (index, templates) = evidence::catalog(
        file,
        receipt,
        endpoint,
        templates,
        veoveo_mcp_conformance::catalog::templates(client.peer()),
    )
    .await?;
    let actual = templates
        .iter()
        .map(|template| veoveo_types::ResourceTemplateUri::new(template.uri_template.clone()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| {
            anyhow!("Timeseries catalog contains invalid template addresses; see private receipt")
        })?;
    observe_catalog(file, receipt, index, |members| match members {
        evidence::CatalogMembers::ResourceTemplatesList { actual: stored, .. } => {
            *stored = Some(actual)
        }
        _ => unreachable!("owning templates request"),
    })
}
fn observe_catalog(
    file: &mut std::fs::File,
    receipt: &mut Receipt,
    index: usize,
    observe: impl FnOnce(&mut evidence::CatalogMembers),
) -> Result<()> {
    observe(&mut receipt.catalogs[index].members);
    persist(file, receipt)?;
    require_catalog_members(&receipt.catalogs[index].members)
}
fn require_catalog_members(members: &evidence::CatalogMembers) -> Result<()> {
    fn require<T: PartialEq + std::fmt::Display>(
        expected: &[T],
        actual: Option<&Vec<T>>,
    ) -> Result<()> {
        let actual = actual.context("Timeseries catalog has no admitted member observation")?;
        let missing: Vec<_> = expected
            .iter()
            .filter(|required| !actual.contains(required))
            .map(ToString::to_string)
            .collect();
        let repeated: Vec<_> = expected
            .iter()
            .filter(|required| actual.iter().filter(|value| *value == *required).count() > 1)
            .map(ToString::to_string)
            .collect();
        ensure!(
            missing.is_empty() && repeated.is_empty(),
            "Timeseries catalog missing required members [{}]; duplicated required members [{}]; see private receipt",
            missing.join(", "),
            repeated.join(", ")
        );
        Ok(())
    }
    match members {
        evidence::CatalogMembers::ToolsList { expected, actual } => {
            require(expected, actual.as_ref())
        }
        evidence::CatalogMembers::ResourceTemplatesList { expected, actual } => {
            require(expected, actual.as_ref())
        }
    }
}

fn require_usage(
    usage: &veoveo_mcp_contract::UsageReport,
    id: veoveo_types::TaskId,
    uri: &ResourceUri,
) -> Result<()> {
    ensure!(
        usage.task_id.parse::<veoveo_types::TaskId>()? == id
            && usage.usage_uri == uri.as_str()
            && usage.records.len() == 1
            && usage.total_amount.is_none()
            && usage.currency.is_none()
            && usage.total_kind == Some(veoveo_mcp_contract::UsageKind::Actual),
        "Timeseries usage identity or records differ"
    );
    for record in &usage.records {
        let metadata: veoveo_timeseries_mcp::contract::TimeseriesForecastUsageMetadata =
            serde_json::from_value(record.metadata.clone())
                .map_err(|_| anyhow!("Timeseries usage metadata failed owner admission"))?;
        ensure!(
            metadata.series_count == 1 && metadata.horizon == TimeseriesForecastHorizon::new(2)?,
            "Timeseries usage metadata differs from the single-series forecast"
        );
        ensure!(
            record.task_id.parse::<veoveo_types::TaskId>()? == id
                && record.kind == veoveo_mcp_contract::UsageKind::Actual
                && record.model_id == "timeseries/naive-trend"
                && record.quantity == Some(4.0)
                && record.unit.as_deref() == Some("source_row")
                && record.amount.is_none()
                && record.currency.is_none()
                && record.source_id.is_none()
                && record.provider_job_id.is_none(),
            "Timeseries usage differs from the four-row forecast"
        );
    }
    Ok(())
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use veoveo_mcp_contract::{UsageKind, UsageRecord, UsageReport};
    use veoveo_timeseries_mcp::contract::{
        TimeseriesForecastUsageMetadata, TimeseriesForecastUsageMetadataBuilder,
    };
    use veoveo_types::TaskId;

    #[test]
    fn charged_provider_or_different_forecast_usage_is_rejected() -> Result<()> {
        let id = TaskId::new();
        let uri = TimeseriesTaskUsageUri::new(id)?.to_uri()?;
        let horizon = TimeseriesForecastHorizon::new(2)?;
        let record = UsageRecord {
            task_id: id.to_string(),
            source_id: None,
            provider_job_id: None,
            model_id: "timeseries/naive-trend".into(),
            kind: UsageKind::Actual,
            quantity: Some(4.0),
            unit: Some("source_row".into()),
            amount: None,
            currency: None,
            recorded_at: chrono::Utc::now(),
            metadata: serde_json::to_value(TimeseriesForecastUsageMetadata::new(1, horizon)?)?,
        };
        let report =
            |record| UsageReport::new(id.to_string(), uri.as_str()).with_records(vec![record]);
        require_usage(&report(record.clone()), id, &uri)?;
        let mut charged = record.clone();
        charged.amount = Some(0.1);
        let mut currency = record.clone();
        currency.currency = Some("USD".into());
        let mut source = record.clone();
        source.source_id = Some("unexpected-source".into());
        let mut provider = record.clone();
        provider.provider_job_id = Some("unexpected-job".into());
        let mut horizon_drift = record.clone();
        horizon_drift.metadata = serde_json::to_value(TimeseriesForecastUsageMetadata::new(
            1,
            TimeseriesForecastHorizon::new(3)?,
        )?)?;
        let mut series_drift = record.clone();
        series_drift.metadata =
            serde_json::to_value(TimeseriesForecastUsageMetadata::new(2, horizon)?)?;
        let mut format_drift = record;
        format_drift.metadata = serde_json::to_value(TimeseriesForecastUsageMetadataBuilder {
            series_count: 1,
            horizon,
            artifact_format: "other".into(),
        })?;
        for different in [
            charged,
            currency,
            source,
            provider,
            horizon_drift,
            series_drift,
            format_drift,
        ] {
            assert!(require_usage(&report(different), id, &uri).is_err());
        }
        Ok(())
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;
    use evidence::{CatalogMembers, CatalogOutcome};

    #[tokio::test]
    async fn missing_templates_are_named_after_actual_catalog_is_persisted() -> Result<()> {
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
        let required = [
            veoveo_artifact_mcp::contract::METADATA_TEMPLATE,
            veoveo_artifact_mcp::contract::ARTIFACT_TEMPLATE,
        ];
        let members = CatalogMembers::ResourceTemplatesList {
            expected: required
                .into_iter()
                .map(veoveo_types::ResourceTemplateUri::new)
                .collect::<std::result::Result<Vec<_>, _>>()?,
            actual: None,
        };
        let response = vec![rmcp::model::ResourceTemplate::new(
            "timeseries://docs/{doc_id}",
            "timeseries-doc",
        )];
        let (index, response) = evidence::catalog(
            &mut file,
            &mut receipt,
            &endpoint,
            members,
            std::future::ready(Ok(response)),
        )
        .await?;
        let actual = response
            .into_iter()
            .map(|template| veoveo_types::ResourceTemplateUri::new(template.uri_template))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let error = observe_catalog(&mut file, &mut receipt, index, |members| {
            let CatalogMembers::ResourceTemplatesList { actual: stored, .. } = members else {
                unreachable!()
            };
            *stored = Some(actual);
        })
        .unwrap_err();
        for expected in required {
            assert!(error.to_string().contains(expected));
        }
        let saved: Receipt = serde_json::from_slice(&std::fs::read(path)?)?;
        assert!(saved.outcome == Outcome::NotDispatched && saved.task_id.is_none());
        assert!(matches!(
            saved.catalogs[0].outcome,
            CatalogOutcome::Received
        ));
        assert!(saved.catalogs[0].response_digest.is_some());
        let CatalogMembers::ResourceTemplatesList {
            expected,
            actual: Some(actual),
        } = &saved.catalogs[0].members
        else {
            bail!("missing persisted catalog")
        };
        assert_eq!(expected.len(), 2);
        assert_eq!(
            actual,
            &[veoveo_types::ResourceTemplateUri::new(
                "timeseries://docs/{doc_id}"
            )?]
        );
        Ok(())
    }
    #[tokio::test]
    async fn failed_catalog_intent_journal_never_polls_request_or_dispatches_forecast() -> Result<()>
    {
        let mut file = std::fs::OpenOptions::new().write(true).open("/dev/full")?;
        let mut receipt = Receipt::new(forecast_request()?);
        let endpoint = veoveo_gateway_contract::ProtectedResourceId::parse(
            "https://fixture.example/mcp/initial",
        )?;
        let polled = std::cell::Cell::new(false);
        let request = async {
            polled.set(true);
            Ok(Vec::<rmcp::model::Tool>::new())
        };
        let result = evidence::catalog(
            &mut file,
            &mut receipt,
            &endpoint,
            CatalogMembers::ToolsList {
                expected: vec![veoveo_gateway_contract::GatewayToolName::parse(
                    "timeseries__forecast",
                )?],
                actual: None,
            },
            request,
        )
        .await;
        assert!(result.is_err() && !polled.get());
        receipt.settle(false);
        assert!(receipt.outcome == Outcome::NotDispatched && receipt.task_id.is_none());
        assert!(matches!(
            receipt.catalogs[0].outcome,
            CatalogOutcome::Interrupted
        ));
        Ok(())
    }
}
