//! Consumer assertions for the retained installed Frames fixture.
use super::*;
use anyhow::ensure;
use futures::future::join_all;
use rmcp::{ServiceError, model::*, service::Subscription};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_frames_mcp::{contract::*, uris};
use veoveo_testing_support::SmokeMcpClient;
use veoveo_types::{CanonicalTaskId, TaskId};

pub(super) struct OwnedListener {
    pub(super) stream: Subscription,
    closed: bool,
}
impl OwnedListener {
    fn new(stream: Subscription) -> Self {
        Self {
            stream,
            closed: false,
        }
    }
    pub(super) async fn close(&mut self) -> bool {
        if !self.closed {
            self.closed = matches!(
                tokio::time::timeout(Duration::from_secs(5), self.stream.cancel()).await,
                Ok(Ok(_))
            );
        }
        self.closed
    }
}

const FIXTURE_COUNT: usize = 101;
const PAGE_BUDGET: usize = 32;
const CONCURRENCY: usize = 4;
#[path = "assertions.rs"]
mod assertions;
use assertions::*;
pub(super) use assertions::{ForeignProbe, probe_timed_out};

#[derive(serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(super) enum Intent {
    Create { request: CreateWorldRequest },
    Publish { request: PublishWorldRequest },
    Convert { request: ConvertFrameRequest },
    Batch { request: BatchTransformRequest },
}
#[derive(serde::Serialize)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(super) enum Observation {
    Unresolved,
    NotDispatched,
    Created {
        output: CreateWorldOutput,
    },
    Published {
        output: PublishWorldOutput,
    },
    InvalidParams,
    Converted {
        output: ConvertFrameOutput,
    },
    TaskAdmitted {
        task_id: CanonicalTaskId,
    },
    TaskCompleted {
        task_id: CanonicalTaskId,
        output: Box<BatchTransformOutput>,
    },
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Mutation {
    intent: Intent,
    observation: Observation,
}
fn intent(file: &mut File, receipt: &mut InstalledFramesReceipt, intent: Intent) -> Result<usize> {
    ensure!(
        receipt.mutations.len() < 207,
        "Frames mutation dispatch budget exhausted"
    );
    let index = receipt.mutations.len();
    receipt.mutations.push(Mutation {
        intent,
        observation: Observation::Unresolved,
    });
    record_frames_mutation_intent(file, receipt)?;
    Ok(index)
}
fn observe(
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    index: usize,
    observation: Observation,
) -> Result<()> {
    receipt.mutations[index].observation = observation;
    write_frames_evidence(file, receipt)
}

pub(super) fn finalize_outcome(
    receipt: &mut InstalledFramesReceipt,
    operation_failed: bool,
    cleanup_closed: bool,
) {
    if operation_failed || !cleanup_closed {
        receipt.outcome = failure_outcome(receipt);
    }
}

pub(super) fn failure_outcome(receipt: &InstalledFramesReceipt) -> InstalledFramesOutcome {
    if receipt.mutations.is_empty() {
        InstalledFramesOutcome::FailedBeforeMutation
    } else if receipt.mutations.iter().any(|mutation| {
        matches!(
            mutation.observation,
            Observation::Unresolved | Observation::TaskAdmitted { .. }
        )
    }) {
        InstalledFramesOutcome::MutationUnresolved
    } else {
        InstalledFramesOutcome::FixtureFailedSettled
    }
}
enum Received<T> {
    Response(Result<T>),
    NotDispatched,
}
// Each request retains its response before yielding Ready to the cancellable group.
// An already-started sibling continues after failure; an unstarted sibling is refused.
async fn collect_received<T, F>(
    requests: impl IntoIterator<Item = (usize, F)>,
    retain: impl FnMut(usize, Received<T>) -> bool,
) where
    F: std::future::Future<Output = Result<T>>,
{
    let retain = std::cell::RefCell::new(retain);
    let dispatch_allowed = std::cell::Cell::new(true);
    join_all(requests.into_iter().map(|(index, request)| {
        let retain = &retain;
        let dispatch_allowed = &dispatch_allowed;
        async move {
            let response = if dispatch_allowed.get() {
                Received::Response(request.await)
            } else {
                Received::NotDispatched
            };
            if !retain.borrow_mut()(index, response) {
                dispatch_allowed.set(false);
            }
        }
    }))
    .await;
}
fn admitted_response<T>(
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    index: usize,
    received: Received<T>,
) -> Option<Result<T>> {
    match received {
        Received::Response(reply) => Some(reply),
        Received::NotDispatched => {
            if let Err(error) = observe(file, receipt, index, Observation::NotDispatched) {
                remember_failure(receipt, error);
            }
            None
        }
    }
}
fn remember_failure(receipt: &mut InstalledFramesReceipt, error: anyhow::Error) {
    if receipt.first_error.is_none() {
        receipt.first_error = Some(error);
    }
}
fn group_settled(receipt: &InstalledFramesReceipt) -> Result<()> {
    ensure!(
        receipt.first_error.is_none(),
        "Frames sibling response or persistence failed"
    );
    Ok(())
}

// An ordinary transport error leaves dispatch unresolved.
async fn call<T: serde::de::DeserializeOwned>(
    client: &SmokeMcpClient,
    tool: &str,
    request: impl serde::Serialize,
) -> Result<T> {
    installed_frames_call(client, tool, request).await
}
pub(super) async fn create(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    request: CreateWorldRequest,
) -> Result<CreateWorldOutput> {
    let index = intent(
        file,
        receipt,
        Intent::Create {
            request: request.clone(),
        },
    )?;
    let output: CreateWorldOutput = call(client, "create_world", &request).await?;
    observe(
        file,
        receipt,
        index,
        Observation::Created {
            output: output.clone(),
        },
    )?;
    require_created(&request, &output)?;
    Ok(output)
}
fn require_created(request: &CreateWorldRequest, output: &CreateWorldOutput) -> Result<()> {
    ensure!(
        output.world.world_id() == request.world_id
            && output.world.display_name == request.display_name
            && output.world.description == request.description
            && output.world.revision() == 0
            && output.world.head_revision_id().is_none()
            && output.world.created_at == output.world.updated_at,
        "Frames creation metadata disagrees with the admitted request"
    );
    Ok(())
}
pub(super) async fn identical_publication(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    request: PublishWorldRequest,
    original: &FrameWorldSummary,
) -> Result<PublishWorldOutput> {
    let a = intent(
        file,
        receipt,
        Intent::Publish {
            request: request.clone(),
        },
    )?;
    let b = intent(
        file,
        receipt,
        Intent::Publish {
            request: request.clone(),
        },
    )?;
    let mut outputs = BTreeMap::new();
    collect_received(
        [
            (
                a,
                call::<PublishWorldOutput>(client, "publish_world", &request),
            ),
            (
                b,
                call::<PublishWorldOutput>(client, "publish_world", &request),
            ),
        ],
        |index, received| {
            let Some(reply) = admitted_response(file, receipt, index, received) else {
                return false;
            };
            match reply {
                Ok(output) => {
                    let persisted = observe(
                        file,
                        receipt,
                        index,
                        Observation::Published {
                            output: output.clone(),
                        },
                    );
                    outputs.insert(index, output);
                    if let Err(error) = persisted {
                        remember_failure(receipt, error);
                    }
                }
                Err(error) => remember_failure(receipt, error),
            }
            receipt.first_error.is_none()
        },
    )
    .await;
    group_settled(receipt)?;
    let left = outputs
        .remove(&a)
        .context("Frames first publication response unavailable")?;
    let right = outputs
        .remove(&b)
        .context("Frames second publication response unavailable")?;
    require_publication(&request, original, &left, 1)?;
    require_publication(&request, original, &right, 1)?;
    ensure!(
        left.created != right.created
            && left.world == right.world
            && left.revision == right.revision,
        "Frames identical concurrent publication did not settle as one creation and one replay"
    );
    Ok(if left.created { left } else { right })
}

pub(super) async fn run(
    client: &SmokeMcpClient,
    foreign: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    published: &PublishWorldOutput,
    listeners: &mut Vec<OwnedListener>,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let prefix = receipt.world_id.to_string();
    // Refuse an already excessively large caller catalog before adding the remaining fixtures.
    let initial_worlds = world_pages(client).await?.0;
    // Preflight admitted the existing catalog before creating this run's primary world.
    ensure!(
        initial_worlds.len() <= (PAGE_BUDGET - 2) * 100 + 1,
        "Frames world fixture exceeds the traversal budget"
    );
    let initial_usage = usage_pages(client).await?.0;
    ensure!(
        initial_usage.len() <= (PAGE_BUDGET - 2) * 100,
        "Frames usage fixture exceeds the traversal budget"
    );
    let mut owned = BTreeMap::from([(receipt.world_id.clone(), published.world.clone())]);
    for start in (1..FIXTURE_COUNT).step_by(CONCURRENCY) {
        let requests = (start..(start + CONCURRENCY).min(FIXTURE_COUNT))
            .map(|n| {
                Ok(CreateWorldRequest {
                    world_id: FrameWorldId::parse(format!("{prefix}-{n:03}"))?,
                    display_name: format!("Installed Frames fixture {n}"),
                    description: Some("Retained installed paging fixture".into()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let indices = requests
            .iter()
            .map(|request| {
                intent(
                    file,
                    receipt,
                    Intent::Create {
                        request: request.clone(),
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        collect_received(
            indices.iter().copied().zip(
                requests
                    .iter()
                    .map(|request| call::<CreateWorldOutput>(client, "create_world", request)),
            ),
            |index, received| {
                let Some(reply) = admitted_response(file, receipt, index, received) else {
                    return false;
                };
                match reply {
                    Ok(output) => {
                        let request = &requests[index - indices[0]];
                        let persisted = observe(
                            file,
                            receipt,
                            index,
                            Observation::Created {
                                output: output.clone(),
                            },
                        );
                        if let Err(error) = persisted {
                            remember_failure(receipt, error);
                        }
                        if let Err(error) = require_created(request, &output) {
                            remember_failure(receipt, error);
                        }
                        owned.insert(request.world_id.clone(), output.world);
                    }
                    Err(error) => remember_failure(receipt, error),
                }
                receipt.first_error.is_none()
            },
        )
        .await;
        group_settled(receipt)?;
    }
    let accepted = competing_publications(client, file, receipt, published).await?;
    receipt.revision_uri = Some(accepted.revision.revision_uri().clone());
    owned.insert(receipt.world_id.clone(), accepted.world.clone());
    write_frames_evidence(file, receipt)?;
    let current: FrameWorldSummary =
        installed_frames_read(client, accepted.world.world_uri().as_str()).await?;
    ensure!(
        current == accepted.world,
        "Frames current world metadata differs from the winning publication"
    );
    let revision: FrameWorldRevision =
        installed_frames_read(client, accepted.revision.revision_uri().as_str()).await?;
    ensure!(
        revision == accepted.revision
            && current.head_revision_id() == Some(&revision.revision_id())
            && current.revision() == revision.revision().get(),
        "Frames current head and immutable revision disagree"
    );
    let (worlds, count) = world_pages(client).await?;
    receipt.worlds_pages = count;
    ensure!(
        count >= 2,
        "Frames world fixture did not produce multiple pages"
    );
    for (id, expected) in &owned {
        ensure!(
            worlds.get(id) == Some(expected),
            "Frames pages omit or change an owned world"
        );
    }
    completion(client, &prefix, &revision).await?;
    let frame_uri = revision
        .revision_uri()
        .frame(&FrameId::parse("robot-world")?);
    let node: FrameNode = installed_frames_read(client, frame_uri.as_str()).await?;
    ensure!(
        revision.frame(&frame_uri) == Some(&node),
        "Frames resource node differs from its immutable tree"
    );
    let request = ConvertFrameRequest {
        target: CoordinateSpace::WorldFrame { frame_uri },
        points: vec![CoordinatePoint::Wgs84(Wgs84Position {
            latitude_degrees: 37.4220999,
            longitude_degrees: -122.0840575,
            ellipsoid_height_m: 12.0,
        })],
        allow_approximation: false,
    };
    let index = intent(
        file,
        receipt,
        Intent::Convert {
            request: request.clone(),
        },
    )?;
    let direct: ConvertFrameOutput = call(client, "convert_frame", &request).await?;
    observe(
        file,
        receipt,
        index,
        Observation::Converted {
            output: direct.clone(),
        },
    )?;
    provenance(client, &direct, &revision, &request).await?;
    require_denied_read(
        foreign,
        ForeignTarget::Operation {
            uri: direct.provenance.operation.operation_uri().clone(),
        },
        file,
        receipt,
    )
    .await?;
    let usage_uri = FrameUsageIndexUri::new(None);
    let usage_filter = SubscriptionFilter::builder()
        .resource_subscriptions([usage_uri.to_string()])
        .build();
    let usage_listener = listeners.len();
    listeners.push(OwnedListener::new(
        client
            .listen(usage_filter.clone())
            .await
            .map_err(|_| anyhow!("Frames usage listener admission failed"))?,
    ));
    ensure!(
        listeners[usage_listener].stream.acknowledged() == &usage_filter,
        "Frames usage listener changed its filter"
    );
    notification(&mut listeners[usage_listener].stream, usage_uri.as_str()).await?;
    let mut completed = BTreeMap::new();
    for start in (0..FIXTURE_COUNT).step_by(CONCURRENCY) {
        let count = CONCURRENCY.min(FIXTURE_COUNT - start);
        let requests = (0..count)
            .map(|_| BatchTransformRequest {
                convert: request.clone(),
                artifact: false,
            })
            .collect::<Vec<_>>();
        let indices = requests
            .iter()
            .map(|request| {
                intent(
                    file,
                    receipt,
                    Intent::Batch {
                        request: request.clone(),
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut ids = Vec::new();
        collect_received(
            indices
                .iter()
                .copied()
                .zip(requests.iter().map(|request| async move {
                    tokio::time::timeout(
                        Duration::from_secs(15),
                        veoveo_testing_support::call_tool_as_task(
                            client,
                            "frames__batch_transform",
                            serde_json::to_value(request).expect("closed Frames input serializes"),
                        ),
                    )
                    .await
                    .map_err(|_| anyhow!("Frames Task dispatch deadline; outcome unresolved"))?
                    .map_err(|_| anyhow!("Frames Task dispatch outcome unresolved"))
                    .and_then(|task| {
                        CanonicalTaskId::parse(&task.task_id)
                            .map_err(|_| anyhow!("Frames Task response identity invalid"))
                    })
                })),
            |index, received| {
                let Some(reply) = admitted_response(file, receipt, index, received) else {
                    return false;
                };
                match reply {
                    Ok(id) => {
                        let persisted = observe(
                            file,
                            receipt,
                            index,
                            Observation::TaskAdmitted {
                                task_id: id.clone(),
                            },
                        );
                        ids.push((index, id));
                        if let Err(error) = persisted {
                            remember_failure(receipt, error);
                        }
                    }
                    Err(error) => remember_failure(receipt, error),
                }
                receipt.first_error.is_none()
            },
        )
        .await;
        group_settled(receipt)?;
        let filter = SubscriptionFilter::builder()
            .task_ids(ids.iter().map(|(_, id)| id.to_string()))
            .build();
        let task_listener = listeners.len();
        listeners.push(OwnedListener::new(
            client
                .listen(filter.clone())
                .await
                .map_err(|_| anyhow!("Frames Task listener admission failed"))?,
        ));
        ensure!(
            listeners[task_listener].stream.acknowledged() == &filter,
            "Frames Task listener changed its filter"
        );
        await_tasks(&mut listeners[task_listener].stream, &ids, deadline).await?;
        if start == 0 {
            notification(&mut listeners[usage_listener].stream, usage_uri.as_str()).await?;
            let _ = listeners[usage_listener].close().await;
        }
        for (index, id) in ids {
            let task = veoveo_testing_support::await_task_terminal_with_timeout(
                client,
                id.as_str(),
                Duration::from_secs(15),
            )
            .await
            .map_err(|_| anyhow!("Frames completed Task read/owned settlement failed"))?;
            ensure!(
                task.task.task_id == id.as_str() && task.status() == TaskStatus::Completed,
                "Frames completed Task identity/status changed"
            );
            let TaskPayload::Completed { result } = task.payload else {
                bail!("Frames completed Task omitted its result");
            };
            let result: CallToolResult = serde_json::from_value(Value::Object(result))?;
            ensure!(
                result.is_error != Some(true),
                "Frames batch Task completed with a tool error"
            );
            let value = result
                .structured_content
                .context("Frames batch Task omitted structured content")?;
            let _: BatchTransformTaskOutput = serde_json::from_value(value.clone())
                .map_err(|_| anyhow!("Frames batch Task result failed owner admission"))?;
            let output: BatchTransformOutput = serde_json::from_value(value)
                .map_err(|_| anyhow!("Frames batch output failed owner admission"))?;
            ensure!(
                output.artifact.is_none(),
                "Frames CPU batch unexpectedly created an Artifact"
            );
            observe(
                file,
                receipt,
                index,
                Observation::TaskCompleted {
                    task_id: id,
                    output: Box::new(output.clone()),
                },
            )?;
            provenance(client, &output.result, &revision, &request).await?;
            ensure!(
                completed
                    .insert(
                        output.result.provenance.operation.operation_id().clone(),
                        output
                    )
                    .is_none(),
                "Frames batch operation identity repeated"
            );
        }
        let _ = listeners[task_listener].close().await;
    }
    let (entries, count) = usage_pages(client).await?;
    receipt.usage_pages = count;
    ensure!(
        count >= 2,
        "Frames usage fixture did not produce multiple pages"
    );
    ensure!(
        entries
            .keys()
            .filter(|id| !initial_usage.contains_key(id))
            .count()
            <= FIXTURE_COUNT + CONCURRENCY,
        "Frames concurrent caller activity exceeds the fixture observation budget"
    );
    let mut matched = BTreeMap::new();
    for (id, entry) in entries {
        if initial_usage.contains_key(&id) {
            continue;
        }
        let report: veoveo_mcp_contract::UsageReport =
            installed_frames_read(client, entry.usage_uri().as_str()).await?;
        ensure!(
            TaskId::parse(&report.task_id)? == id && report.usage_uri == entry.usage_uri().as_str(),
            "Frames usage identity disagrees with its page reference"
        );
        for record in &report.records {
            let Some(source) = record.source_id.as_deref() else {
                continue;
            };
            let operation_id = CoordinateOperationId::parse(source)?;
            if !completed.contains_key(&operation_id) {
                continue;
            }
            ensure!(
                record.kind == veoveo_mcp_contract::UsageKind::Actual
                    && record.quantity == Some(1.0)
                    && record.unit.as_deref() == Some("point")
                    && record.amount.is_none()
                    && record.currency.is_none()
                    && TaskId::parse(&record.task_id)? == id,
                "Frames actual usage shape or identity changed"
            );
            ensure!(
                matched.insert(operation_id, entry.clone()).is_none(),
                "Frames actual usage repeated an operation"
            );
        }
        receipt.usage.push(report);
        write_frames_evidence(file, receipt)?;
    }
    ensure!(
        matched.len() == FIXTURE_COUNT && completed.len() == FIXTURE_COUNT,
        "Frames usage pages omit completed fixture operations"
    );
    let foreign_entries = usage_pages(foreign).await?.0;
    ensure!(
        matched
            .values()
            .all(|entry| !foreign_entries.contains_key(&entry.task_id())),
        "Frames foreign usage pages exposed an owned Task"
    );
    let (operation, usage) = matched
        .first_key_value()
        .context("Frames usage fixture is empty")?;
    let operation_uri = FrameOperationUri::new(operation);
    require_denied_read(
        foreign,
        ForeignTarget::Operation { uri: operation_uri },
        file,
        receipt,
    )
    .await?;
    require_denied_read(
        foreign,
        ForeignTarget::TaskUsage {
            uri: usage.usage_uri().clone(),
        },
        file,
        receipt,
    )
    .await?;
    require_denied_subscription(foreign, usage.usage_uri(), listeners, file, receipt).await?;
    write_frames_evidence(file, receipt)?;
    Ok(())
}

async fn competing_publications(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    published: &PublishWorldOutput,
) -> Result<PublishWorldOutput> {
    let mut left = PublishWorldRequest {
        world_id: receipt.world_id.clone(),
        expected_head_revision_id: Some(published.revision.revision_id()),
        tree: installed_frames_tree()?,
    };
    left.tree.frames[0].description = Some("Installed competing publication left".into());
    let mut right = left.clone();
    right.tree.frames[0].description = Some("Installed competing publication right".into());
    let a = intent(
        file,
        receipt,
        Intent::Publish {
            request: left.clone(),
        },
    )?;
    let b = intent(
        file,
        receipt,
        Intent::Publish {
            request: right.clone(),
        },
    )?;
    let mut outputs = BTreeMap::new();
    collect_received(
        [
            (a, publish_candidate(client, &left)),
            (b, publish_candidate(client, &right)),
        ],
        |index, received| {
            let Some(reply) = admitted_response(file, receipt, index, received) else {
                return false;
            };
            match reply {
                Ok(output) => {
                    let observation = match &output {
                        Some(output) => Observation::Published {
                            output: output.clone(),
                        },
                        None => Observation::InvalidParams,
                    };
                    let persisted = observe(file, receipt, index, observation);
                    outputs.insert(index, output);
                    if let Err(error) = persisted {
                        remember_failure(receipt, error);
                    }
                }
                Err(error) => remember_failure(receipt, error),
            }
            receipt.first_error.is_none()
        },
    )
    .await;
    group_settled(receipt)?;
    let left_output = outputs
        .remove(&a)
        .context("Frames left publication response unavailable")?;
    let right_output = outputs
        .remove(&b)
        .context("Frames right publication response unavailable")?;
    ensure!(
        left_output.is_some() != right_output.is_some(),
        "Frames competing expected-head publications did not yield one winner"
    );
    let (winner, request) = if let Some(output) = left_output {
        (output, &left)
    } else {
        (right_output.expect("one publication won"), &right)
    };
    require_publication(request, &published.world, &winner, 2)?;
    ensure!(
        winner.created && winner.world.revision() == 2 && winner.revision.revision().get() == 2,
        "Frames competing publication advanced the head incorrectly"
    );
    Ok(winner)
}
fn require_publication(
    request: &PublishWorldRequest,
    original: &FrameWorldSummary,
    output: &PublishWorldOutput,
    number: u64,
) -> Result<()> {
    let admitted = ValidatedWorldTree::new(request.tree.clone())?;
    ensure!(
        request.world_id == original.world_id()
            && output.world.world_id() == request.world_id
            && output.revision.world_id() == request.world_id
            && output.world.display_name == original.display_name
            && output.world.description == original.description
            && output.world.created_at == original.created_at
            && output.world.updated_at >= original.updated_at
            && output.world.revision() == number
            && output.revision.revision().get() == number
            && output.world.head_revision_id() == Some(&output.revision.revision_id())
            && output.revision.tree() == admitted.tree(),
        "Frames publication changed submitted tree, world metadata or head identity"
    );
    Ok(())
}
pub(super) async fn admit_catalogs(client: &SmokeMcpClient, world: &FrameWorldId) -> Result<()> {
    let worlds = world_pages(client).await?.0;
    let usage = usage_pages(client).await?.0;
    ensure!(
        worlds.len() <= (PAGE_BUDGET - 2) * 100 && usage.len() <= (PAGE_BUDGET - 2) * 100,
        "Frames caller catalogs leave insufficient fixture traversal budget"
    );
    ensure!(
        !worlds
            .keys()
            .any(|id| id.as_str().starts_with(world.as_str())),
        "Frames fresh fixture prefix is already present"
    );
    Ok(())
}

async fn publish_candidate(
    client: &SmokeMcpClient,
    request: &PublishWorldRequest,
) -> Result<Option<PublishWorldOutput>> {
    let params = CallToolRequestParams::new("frames__publish_world")
        .with_arguments(serde_json::from_value(serde_json::to_value(request)?)?);
    match tokio::time::timeout(Duration::from_secs(15), client.call_tool_once(params))
        .await
        .map_err(|_| anyhow!("Frames competing publication deadline; outcome unresolved"))?
    {
        Err(ServiceError::McpError(error)) if error.code == ErrorCode::INVALID_PARAMS => Ok(None),
        Err(_) => bail!("Frames competing publication response unresolved"),
        Ok(response) => {
            let result = require_complete_frames_response(response)?;
            ensure!(
                result.is_error != Some(true),
                "Frames competing publication returned an unclassified tool error"
            );
            Ok(Some(serde_json::from_value(
                result
                    .structured_content
                    .context("Frames publication omitted structured content")?,
            )?))
        }
    }
}

#[cfg(test)]
mod installed_mutation_tests {
    use super::*;
    fn receipt() -> Result<InstalledFramesReceipt> {
        Ok(InstalledFramesReceipt {
            schema_version: "veoveo.ai/frames-installed-evidence/v3",
            world_id: FrameWorldId::parse("receipt-fixture")?,
            revision_uri: None,
            outcome: InstalledFramesOutcome::FailedBeforeMutation,
            cleanup: InstalledFramesCleanup::NotOpened,
            retained_append_only: true,
            mutations: Vec::new(),
            worlds_pages: 0,
            usage_pages: 0,
            task_subscription_cleanup: false,
            first_error: None,
            foreign_probes: Vec::new(),
            usage: Vec::new(),
        })
    }
    fn request() -> Result<CreateWorldRequest> {
        Ok(CreateWorldRequest {
            world_id: FrameWorldId::parse("receipt-fixture")?,
            display_name: "Owned fixture".into(),
            description: None,
        })
    }
    #[test]
    fn ledger_keeps_independent_dispatch_uncertainty_and_refuses_unpersisted_intent() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let mut receipt = receipt()?;
        let request = request()?;
        let first = intent(
            &mut file,
            &mut receipt,
            Intent::Create {
                request: request.clone(),
            },
        )?;
        let second = intent(
            &mut file,
            &mut receipt,
            Intent::Create {
                request: request.clone(),
            },
        )?;
        observe(
            &mut file,
            &mut receipt,
            first,
            Observation::Created {
                output: CreateWorldOutput {
                    world: FrameWorldSummary::new(
                        request.world_id,
                        request.display_name,
                        chrono::Utc::now(),
                    ),
                },
            },
        )?;
        let persisted: Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(
            persisted["mutations"][first]["observation"]["state"],
            "created"
        );
        assert_eq!(
            persisted["mutations"][second]["observation"]["state"],
            "unresolved"
        );
        assert_eq!(persisted["outcome"], "mutation_unresolved");
        let before = fs::read(&path)?;
        let mut read_only = File::open(&path)?;
        assert!(
            intent(
                &mut read_only,
                &mut receipt,
                Intent::Create {
                    request: self::request()?
                }
            )
            .is_err()
        );
        assert_eq!(fs::read(&path)?, before);
        Ok(())
    }
    #[tokio::test]
    async fn received_task_identity_survives_pending_sibling_cancellation() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut file = admit_frames_evidence(&directory.path().join("receipt.json"))?;
        let mut receipt = receipt()?;
        let a = intent(
            &mut file,
            &mut receipt,
            Intent::Create {
                request: request()?,
            },
        )?;
        let b = intent(
            &mut file,
            &mut receipt,
            Intent::Create {
                request: request()?,
            },
        )?;
        let known = CanonicalTaskId::parse("gateway-known-task")?;
        let requests = [(a, false), (b, true)].into_iter().map(|(index, pending)| {
            let known = known.clone();
            (index, async move {
                if pending {
                    std::future::pending::<()>().await;
                }
                Ok(known)
            })
        });
        let outcome = tokio::time::timeout(
            Duration::from_millis(50),
            collect_received(requests, |index, received| {
                let Some(reply) = admitted_response(&mut file, &mut receipt, index, received)
                else {
                    return false;
                };
                let result = reply.and_then(|id| {
                    observe(
                        &mut file,
                        &mut receipt,
                        index,
                        Observation::TaskAdmitted { task_id: id },
                    )
                });
                if let Err(error) = result {
                    remember_failure(&mut receipt, error);
                }
                receipt.first_error.is_none()
            }),
        )
        .await;
        assert!(outcome.is_err());
        assert!(
            matches!(&receipt.mutations[a].observation, Observation::TaskAdmitted { task_id } if task_id == &known)
        );
        assert!(matches!(
            receipt.mutations[b].observation,
            Observation::Unresolved
        ));
        let retained: Value =
            serde_json::from_slice(&fs::read(directory.path().join("receipt.json"))?)?;
        assert_eq!(
            retained["mutations"][a]["observation"]["taskId"],
            known.as_str()
        );
        Ok(())
    }
    #[tokio::test]
    async fn sibling_outcomes_survive_first_persistence_or_validation_failure() -> Result<()> {
        for write_failure in [true, false] {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("receipt.json");
            let mut file = admit_frames_evidence(&path)?;
            let mut receipt = receipt()?;
            let a = intent(
                &mut file,
                &mut receipt,
                Intent::Create {
                    request: request()?,
                },
            )?;
            let b = intent(
                &mut file,
                &mut receipt,
                Intent::Create {
                    request: request()?,
                },
            )?;
            if write_failure {
                file = File::open(&path)?;
            }
            let requests = [a, b].into_iter().map(|index| {
                (index, async move {
                    tokio::task::yield_now().await;
                    Ok(CreateWorldOutput {
                        world: FrameWorldSummary::new(
                            FrameWorldId::parse(format!("received-{index}"))?,
                            "Received outcome".into(),
                            chrono::Utc::now(),
                        ),
                    })
                })
            });
            collect_received(requests, |index, received| {
                let Some(reply) = admitted_response(&mut file, &mut receipt, index, received)
                else {
                    return false;
                };
                let result = reply.and_then(|output| {
                    observe(
                        &mut file,
                        &mut receipt,
                        index,
                        Observation::Created { output },
                    )
                });
                if let Err(error) = result {
                    remember_failure(&mut receipt, error);
                }
                if index == a && !write_failure {
                    remember_failure(&mut receipt, anyhow!("first validation failure"));
                }
                receipt.first_error.is_none()
            })
            .await;
            assert!(matches!(
                receipt.mutations[a].observation,
                Observation::Created { .. }
            ));
            assert!(matches!(
                receipt.mutations[b].observation,
                Observation::Created { .. }
            ));
            assert!(group_settled(&receipt).is_err());
            assert!(receipt.first_error.is_some());
            if !write_failure {
                assert_eq!(
                    receipt.first_error.as_ref().unwrap().to_string(),
                    "first validation failure"
                );
                let retained: Value = serde_json::from_slice(&fs::read(path)?)?;
                assert_eq!(retained["mutations"][b]["observation"]["state"], "created");
            }
        }
        Ok(())
    }
    #[tokio::test]
    async fn persistence_failure_refuses_unstarted_sibling_dispatch() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let mut receipt = receipt()?;
        let a = intent(
            &mut file,
            &mut receipt,
            Intent::Create {
                request: request()?,
            },
        )?;
        let b = intent(
            &mut file,
            &mut receipt,
            Intent::Create {
                request: request()?,
            },
        )?;
        file = File::open(path)?;
        let started = std::cell::Cell::new(0);
        let requests = [a, b].into_iter().map(|index| {
            let started = &started;
            (index, async move {
                started.set(started.get() + 1);
                Ok(CreateWorldOutput {
                    world: FrameWorldSummary::new(
                        FrameWorldId::parse(format!("received-{index}"))?,
                        "Received outcome".into(),
                        chrono::Utc::now(),
                    ),
                })
            })
        });
        collect_received(requests, |index, received| {
            let Some(reply) = admitted_response(&mut file, &mut receipt, index, received) else {
                return false;
            };
            if let Err(error) = reply.and_then(|output| {
                observe(
                    &mut file,
                    &mut receipt,
                    index,
                    Observation::Created { output },
                )
            }) {
                remember_failure(&mut receipt, error);
            }
            receipt.first_error.is_none()
        })
        .await;
        assert_eq!(started.get(), 1);
        assert!(matches!(
            receipt.mutations[a].observation,
            Observation::Created { .. }
        ));
        assert!(matches!(
            receipt.mutations[b].observation,
            Observation::NotDispatched
        ));
        Ok(())
    }
    fn publication_fixture() -> Result<(PublishWorldRequest, FrameWorldSummary, PublishWorldOutput)>
    {
        let request = PublishWorldRequest {
            world_id: FrameWorldId::parse("published-fixture")?,
            expected_head_revision_id: None,
            tree: installed_frames_tree()?,
        };
        let original = FrameWorldSummary::new(
            request.world_id.clone(),
            "Original name".into(),
            chrono::Utc::now(),
        )
        .with_description(Some("Original description".into()));
        let uri = FrameWorldUri::new(&request.world_id)
            .revision(&FrameWorldRevisionId::parse("fixture-revision")?);
        let revision = FrameWorldRevision::new(
            uri,
            1.try_into()?,
            ValidatedWorldTree::new(request.tree.clone())?,
            original.created_at,
        );
        let output = PublishWorldOutput {
            world: original
                .clone()
                .with_head(revision.revision_id(), 1.try_into()?),
            revision,
            created: true,
        };
        Ok((request, original, output))
    }
    #[test]
    fn publication_binds_submitted_tree_and_original_world_metadata() -> Result<()> {
        let (request, original, output) = publication_fixture()?;
        require_publication(&request, &original, &output, 1)?;
        let mut third_tree = request.clone();
        third_tree.tree.frames[0].description = Some("Different submitted tree".into());
        assert!(require_publication(&third_tree, &original, &output, 1).is_err());
        let mut renamed = output.clone();
        renamed.world.display_name = "Unrequested rename".into();
        assert!(require_publication(&request, &original, &renamed, 1).is_err());
        let mut wrong_world = request.clone();
        wrong_world.world_id = FrameWorldId::parse("unrelated-world")?;
        assert!(require_publication(&wrong_world, &original, &output, 1).is_err());
        Ok(())
    }
    #[test]
    fn fixture_failure_distinguishes_settled_mutations_from_pending_dispatch_or_task() -> Result<()>
    {
        let mut receipt = receipt()?;
        assert_eq!(
            failure_outcome(&receipt),
            InstalledFramesOutcome::FailedBeforeMutation
        );
        let request = request()?;
        let output = CreateWorldOutput {
            world: FrameWorldSummary::new(
                request.world_id.clone(),
                request.display_name.clone(),
                chrono::Utc::now(),
            ),
        };
        receipt.mutations = (0..106)
            .map(|_| Mutation {
                intent: Intent::Create {
                    request: request.clone(),
                },
                observation: Observation::Created {
                    output: output.clone(),
                },
            })
            .collect();
        assert_eq!(
            failure_outcome(&receipt),
            InstalledFramesOutcome::FixtureFailedSettled
        );
        receipt.outcome = InstalledFramesOutcome::Passed;
        finalize_outcome(&mut receipt, false, true);
        assert_eq!(receipt.outcome, InstalledFramesOutcome::Passed);
        // A completed domain run cannot pass if awaited connection cleanup fails.
        finalize_outcome(&mut receipt, false, false);
        assert_eq!(
            receipt.outcome,
            InstalledFramesOutcome::FixtureFailedSettled
        );
        receipt.mutations[105].observation = Observation::NotDispatched;
        assert_eq!(
            failure_outcome(&receipt),
            InstalledFramesOutcome::FixtureFailedSettled
        );
        receipt.mutations[105].observation = Observation::InvalidParams;
        assert_eq!(
            failure_outcome(&receipt),
            InstalledFramesOutcome::FixtureFailedSettled
        );
        receipt.mutations[105].observation = Observation::Unresolved;
        assert_eq!(
            failure_outcome(&receipt),
            InstalledFramesOutcome::MutationUnresolved
        );
        receipt.mutations[105].observation = Observation::TaskAdmitted {
            task_id: CanonicalTaskId::parse("gateway-observed-pending-task")?,
        };
        assert_eq!(
            failure_outcome(&receipt),
            InstalledFramesOutcome::MutationUnresolved
        );
        Ok(())
    }
    #[test]
    fn dispatch_cap_refuses_the_next_mutation_without_rewriting_the_receipt() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let mut receipt = receipt()?;
        receipt.mutations = (0..207)
            .map(|_| Mutation {
                intent: Intent::Create {
                    request: request().unwrap(),
                },
                observation: Observation::Unresolved,
            })
            .collect();
        write_frames_evidence(&mut file, &receipt)?;
        let before = fs::read(&path)?;
        assert!(
            intent(
                &mut file,
                &mut receipt,
                Intent::Create {
                    request: request()?
                }
            )
            .is_err()
        );
        assert_eq!(receipt.mutations.len(), 207);
        assert_eq!(fs::read(path)?, before);
        Ok(())
    }
}
