//! Installed GPU replay with dispatch and official MCP observation on distinct Pods.
use super::*;
use rmcp::model::{
    DetailedTask, GetTaskParams, ServerNotification, SubscriptionFilter, TaskPayload, TaskStatus,
};
use serde::Serialize;
use std::collections::BTreeSet;
use veoveo_stream_mcp::contract::{
    AnalysisResults, RunId, RunRecordingOutput, RunResultsUri, RunUri,
};
use veoveo_types::ResourceUri;

#[path = "replica_pods.rs"]
mod pods;

const OBSERVER_PORT: u16 = 18797;
const OBSERVER_URL: &str = "http://127.0.0.1:18797/stream/mcp";

pub(super) struct ReplicaProbe {
    writer: pods::PodIdentity,
    observer: pods::PodIdentity,
    report: PathBuf,
    forward: PortForwardGuard,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    writer: &'a pods::PodIdentity,
    observer: &'a pods::PodIdentity,
    run: Option<RunId>,
    task_notifications: u32,
    resource_notifications: u32,
    reconnect_verified: bool,
    cancellation_verified: bool,
    completed: bool,
    failure: Option<String>,
}

impl ReplicaProbe {
    pub(super) fn prepare(
        installation: &InstalledTarget,
        names: &[String],
        work_dir: &Path,
    ) -> Result<Option<Self>> {
        if names.is_empty() {
            return Ok(None);
        }
        ensure!(names.len() == 2, "supply writer and observer Pod names");
        let writer = pods::PodIdentity::read(installation, &names[0])?;
        let observer = pods::PodIdentity::read(installation, &names[1])?;
        pods::check_pair(&writer, &observer)?;
        let ports = [8797, OBSERVER_PORT].map(|port| {
            std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                .with_context(|| format!("replica port {port} is already occupied"))
        });
        for port in ports {
            drop(port?);
        }
        std::fs::create_dir_all(work_dir)?;
        let report = work_dir.join("stream-replicas.json");
        let mut receipt = std::fs::File::create_new(&report).context(
            "replica report already exists; inspect the previous outcome before another run",
        )?;
        serde_json::to_writer_pretty(
            &mut receipt,
            &json!({"writer": writer, "observer": observer, "completed": false}),
        )?;
        let kube = &installation.target.kubernetes;
        let forward = PortForwardGuard::spawn(
            &kube.context,
            &kube.namespace,
            &format!("pod/{}", observer.name),
            OBSERVER_PORT,
            8797,
        )?;
        Ok(Some(Self {
            writer,
            observer,
            report,
            forward,
        }))
    }

    pub(super) fn writer_resource(&self) -> String {
        format!("pod/{}", self.writer.name)
    }

    pub(super) async fn run(
        &mut self,
        installation: &InstalledTarget,
        writer: &FinalTaskSmokeClient,
        token: String,
        arguments: serde_json::Value,
        writer_forward: &mut PortForwardGuard,
    ) -> Result<serde_json::Value> {
        let mut report = Report {
            writer: &self.writer,
            observer: &self.observer,
            run: None,
            task_notifications: 0,
            resource_notifications: 0,
            reconnect_verified: false,
            cancellation_verified: false,
            completed: false,
            failure: None,
        };
        let outcome = tokio::time::timeout(Duration::from_secs(300), async {
            ensure!(
                self.forward.child.try_wait()?.is_none()
                    && writer_forward.child.try_wait()?.is_none(),
                "replica port forward exited"
            );
            let observer = FinalTaskSmokeClient::new(OBSERVER_URL, token).with_host(STREAM_HOST);
            let observer = observer
                .connect()
                .await
                .context("connecting to observer Pod")?;
            let writer = writer.connect().await.context("connecting to writer Pod")?;
            let result = observe(&writer, &observer, arguments, &mut report).await;
            let stopped = tokio::time::timeout(Duration::from_secs(10), async {
                let (observer, writer) = tokio::join!(observer.cancel(), writer.cancel());
                observer?;
                writer?;
                Ok::<_, anyhow::Error>(())
            })
            .await;
            let output = result?;
            stopped.context("replica MCP cleanup exceeded ten seconds")??;
            ensure!(
                self.forward.child.try_wait()?.is_none()
                    && writer_forward.child.try_wait()?.is_none(),
                "replica port forward exited"
            );
            ensure!(
                pods::PodIdentity::read(installation, &self.writer.name)? == self.writer
                    && pods::PodIdentity::read(installation, &self.observer.name)? == self.observer,
                "Stream Pods changed during replica qualification"
            );
            Ok::<_, anyhow::Error>(output)
        })
        .await
        .context("cross-replica Stream run exceeded 300 seconds")
        .and_then(|r| r);
        report.completed = outcome.is_ok();
        report.failure = outcome.as_ref().err().map(|error| format!("{error:#}"));
        serde_json::to_writer_pretty(std::fs::File::create(&self.report)?, &report)?;
        outcome
    }
}

async fn observe(
    writer: &SmokeMcpClient,
    observer: &SmokeMcpClient,
    arguments: serde_json::Value,
    report: &mut Report<'_>,
) -> Result<serde_json::Value> {
    let created = call_tool_as_task(writer, "run_recording", arguments).await?;
    let run = RunId::parse(&created.task_id)?;
    report.run = Some(run);
    let resources = BTreeSet::from([RunUri::new(run).to_uri(), RunResultsUri::new(run).to_uri()]);
    let filter = SubscriptionFilter::builder()
        .task_ids([run.to_string()])
        .resource_subscriptions(resources.iter().map(ToString::to_string))
        .build();
    let mut subscription = observer.listen(filter.clone()).await?;
    ensure!(
        subscription.acknowledged() == &filter,
        "observer changed the subscription filter"
    );
    snapshot(
        &mut subscription,
        run,
        &resources,
        TaskStatus::Working,
        report,
    )
    .await?;
    let before = observer
        .get_task(GetTaskParams::new(run.to_string()))
        .await?
        .task;
    ensure!(
        before.status() == TaskStatus::Working,
        "Task {run} finished before observer admission; cross-replica transition is unproven"
    );
    let mut terminal = None;
    let mut invalidated = BTreeSet::new();
    while terminal.is_none() || invalidated != resources {
        match subscription
            .next()
            .await?
            .context("observer subscription ended before completion")?
        {
            ServerNotification::TaskStatusNotification(update) => {
                let task = update.params.task;
                ensure!(
                    task.task.task_id == run.to_string(),
                    "observer received an unrelated Task"
                );
                report.task_notifications += 1;
                match task.status() {
                    TaskStatus::Working => {}
                    TaskStatus::Completed => {
                        if terminal.is_none() {
                            invalidated.clear();
                        }
                        terminal = Some(task);
                    }
                    status => bail!("Stream Task {run} ended with {status:?}"),
                }
            }
            ServerNotification::ResourceUpdatedNotification(update) => {
                let uri = ResourceUri::new(update.params.uri)?;
                ensure!(
                    resources.contains(&uri),
                    "observer received an unrequested resource"
                );
                report.resource_notifications += 1;
                if terminal.is_some() {
                    invalidated.insert(uri);
                }
            }
            _ => bail!("observer received an unrequested notification"),
        }
    }
    let terminal = terminal.context("missing completed Task")?;
    let dispatched = writer
        .get_task(GetTaskParams::new(run.to_string()))
        .await?
        .task;
    let observed = observer
        .get_task(GetTaskParams::new(run.to_string()))
        .await?
        .task;
    ensure!(
        terminal == dispatched && terminal == observed,
        "completion notification and cross-replica Task reads disagree"
    );
    let output = completed(&terminal)?;
    let typed: RunRecordingOutput = serde_json::from_value(output.clone())?;
    ensure!(
        typed.run_id() == run,
        "completion product belongs to another run"
    );
    let uri = typed.result_uri().to_uri();
    let a = read_mcp_resource_json(writer, uri.as_str()).await?;
    let b = read_mcp_resource_json(observer, uri.as_str()).await?;
    ensure!(a == b, "cross-replica result resources disagree");
    let results: AnalysisResults = serde_json::from_value(b)?;
    results.validate()?;
    ensure!(
        results.pipeline_id == *typed.pipeline_uri.id()
            && results.model_id == *typed.model_uri.id()
            && results.processed_frames == typed.summary.processed_frames,
        "observer result does not match the completed product"
    );
    subscription.cancel().await?;
    ensure!(
        subscription.next().await?.is_none(),
        "cancelled subscription still delivers updates"
    );
    let mut resumed = observer.listen(filter.clone()).await?;
    ensure!(
        resumed.acknowledged() == &filter,
        "reconnected observer changed the filter"
    );
    let recovered = snapshot(&mut resumed, run, &resources, TaskStatus::Completed, report).await?;
    ensure!(
        recovered == terminal,
        "reconnected Task differs from completion"
    );
    report.reconnect_verified = true;
    resumed.cancel().await?;
    ensure!(
        resumed.next().await?.is_none(),
        "cancelled reconnect still delivers updates"
    );
    report.cancellation_verified = true;
    Ok(output)
}

fn completed(task: &DetailedTask) -> Result<serde_json::Value> {
    let TaskPayload::Completed { result } = &task.payload else {
        bail!("Task is not complete");
    };
    let result: rmcp::model::CallToolResult =
        serde_json::from_value(serde_json::Value::Object(result.clone()))?;
    ensure!(
        result.is_error != Some(true),
        "completed Task contains a tool error"
    );
    result
        .structured_content
        .context("completed Task omits its typed product")
}

/// Initial invalidations establish source readiness before asserting a transition.
async fn snapshot(
    subscription: &mut rmcp::service::Subscription,
    run: RunId,
    resources: &BTreeSet<ResourceUri>,
    expected: TaskStatus,
    report: &mut Report<'_>,
) -> Result<DetailedTask> {
    let mut task = None;
    let mut seen = BTreeSet::new();
    while task.is_none() || &seen != resources {
        match subscription
            .next()
            .await?
            .context("subscription ended before its baseline")?
        {
            ServerNotification::TaskStatusNotification(update) => {
                ensure!(
                    update.params.task.task.task_id == run.to_string(),
                    "baseline contains another Task"
                );
                ensure!(
                    update.params.task.status() == expected,
                    "Task {run} baseline is {:?}, expected {expected:?}",
                    update.params.task.status()
                );
                report.task_notifications += 1;
                task = Some(update.params.task);
            }
            ServerNotification::ResourceUpdatedNotification(update) => {
                let uri = ResourceUri::new(update.params.uri)?;
                ensure!(
                    resources.contains(&uri),
                    "baseline contains an unrequested resource"
                );
                report.resource_notifications += 1;
                seen.insert(uri);
            }
            _ => bail!("baseline contains an unrequested notification"),
        }
    }
    task.context("subscription omitted the Task baseline")
}
