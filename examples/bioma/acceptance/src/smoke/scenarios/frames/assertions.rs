//! Installed resource, completion, provenance and subscription assertions.
use super::*;

pub(super) async fn world_pages(
    client: &SmokeMcpClient,
) -> Result<(BTreeMap<FrameWorldId, FrameWorldSummary>, usize)> {
    let mut cursor = None;
    let mut found = BTreeMap::new();
    let mut previous = None;
    for n in 1..=PAGE_BUDGET {
        let uri = FrameWorldsUri::new(cursor.as_ref());
        let page: FrameWorldPage = installed_frames_read(client, uri.as_str()).await?;
        let next = append_world_page(page, &mut previous, &mut found)?;
        if let Some(next) = next {
            cursor = Some(next);
        } else {
            return Ok((found, n));
        }
    }
    bail!("Frames world catalog exceeds 32-page observation budget")
}
fn append_world_page(
    page: FrameWorldPage,
    previous: &mut Option<FrameWorldId>,
    found: &mut BTreeMap<FrameWorldId, FrameWorldSummary>,
) -> Result<Option<FrameWorldCursor>> {
    ensure!(
        page.limit == FRAME_WORLD_PAGE_SIZE && page.items.len() <= FRAME_WORLD_PAGE_SIZE,
        "Frames world page changed its bound"
    );
    for item in &page.items {
        let id = item.world_id();
        ensure!(
            previous.as_ref().is_none_or(|prior| &id > prior),
            "Frames world pages overlap or lose ordering"
        );
        *previous = Some(id.clone());
        ensure!(
            found.insert(id, item.clone()).is_none(),
            "Frames world page repeated an identity"
        );
    }
    if let Some(next) = &page.next_cursor {
        ensure!(
            page.items.len() == FRAME_WORLD_PAGE_SIZE && previous.as_ref() == Some(next.after()),
            "Frames world cursor does not identify the last full-page item"
        );
    }
    Ok(page.next_cursor)
}

pub(super) async fn usage_pages(
    client: &SmokeMcpClient,
) -> Result<(BTreeMap<TaskId, FrameUsageEntry>, usize)> {
    let mut cursor = None;
    let mut found = BTreeMap::new();
    let mut previous = None;
    for n in 1..=PAGE_BUDGET {
        let page: FrameUsagePage =
            installed_frames_read(client, FrameUsageIndexUri::new(cursor.as_ref()).as_str())
                .await?;
        for entry in page.items() {
            let id = entry.task_id();
            ensure!(
                previous.is_none_or(|prior| id > prior),
                "Frames usage pages overlap or lose ordering"
            );
            previous = Some(id);
            ensure!(
                found.insert(id, entry.clone()).is_none(),
                "Frames usage pages repeat an identity"
            );
        }
        if let Some(next) = page.next_cursor() {
            ensure!(
                page.items().len() == FRAME_USAGE_PAGE_SIZE && previous == Some(next.after()),
                "Frames usage cursor does not identify the last full-page item"
            );
            cursor = Some(next.clone());
        } else {
            return Ok((found, n));
        }
    }
    bail!("Frames usage catalog exceeds 32-page observation budget")
}
pub(super) async fn completion(
    client: &SmokeMcpClient,
    prefix: &str,
    revision: &FrameWorldRevision,
) -> Result<()> {
    let request = CompleteRequestParams::new(
        Reference::for_resource(uris::WORLD_TEMPLATE),
        ArgumentInfo::new("world_id", prefix),
    );
    let response = client
        .complete(request)
        .await
        .map_err(|_| anyhow!("Frames world completion failed"))?;
    ensure!(
        response.completion.values.len() == 100
            && response.completion.has_more == Some(true)
            && response.completion.total.is_none(),
        "Frames completion did not expose its real truncated result"
    );
    let ids = response
        .completion
        .values
        .iter()
        .map(FrameWorldId::parse)
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let expected = std::iter::once(FrameWorldId::parse(prefix)?)
        .chain((1..FIXTURE_COUNT).map(|n| {
            FrameWorldId::parse(format!("{prefix}-{n:03}")).expect("bounded typed fixture identity")
        }))
        .take(100)
        .collect::<BTreeSet<_>>();
    ensure!(
        ids == expected,
        "Frames completion returned unrelated or duplicate worlds"
    );
    let context = CompletionContext::with_arguments(std::collections::HashMap::from([(
        "world_id".into(),
        revision.world_id().to_string(),
    )]));
    let request = CompleteRequestParams::new(
        Reference::for_resource(uris::WORLD_REVISION_TEMPLATE),
        ArgumentInfo::new("revision_id", ""),
    )
    .with_context(context);
    let response = client
        .complete(request)
        .await
        .map_err(|_| anyhow!("Frames revision completion failed"))?;
    ensure!(
        response
            .completion
            .values
            .iter()
            .any(|id| id == revision.revision_id().as_str())
            && response.completion.has_more != Some(true),
        "Frames parent-scoped revision completion omitted the owned head"
    );
    let missing = client
        .complete(CompleteRequestParams::new(
            Reference::for_resource(uris::WORLD_FRAME_TEMPLATE),
            ArgumentInfo::new("frame_id", ""),
        ))
        .await
        .map_err(|_| anyhow!("Frames missing-parent completion failed"))?;
    ensure!(
        missing.completion.values.is_empty(),
        "Frames completion admitted missing parent contexts"
    );
    let world_only = CompletionContext::with_arguments(std::collections::HashMap::from([(
        "world_id".into(),
        revision.world_id().to_string(),
    )]));
    let missing_revision = client
        .complete(
            CompleteRequestParams::new(
                Reference::for_resource(uris::WORLD_FRAME_TEMPLATE),
                ArgumentInfo::new("frame_id", ""),
            )
            .with_context(world_only),
        )
        .await
        .map_err(|_| anyhow!("Frames incomplete-parent completion failed"))?;
    ensure!(
        missing_revision.completion.values.is_empty(),
        "Frames completion admitted a missing revision context"
    );
    let context = CompletionContext::with_arguments(std::collections::HashMap::from([
        ("world_id".into(), revision.world_id().to_string()),
        ("revision_id".into(), revision.revision_id().to_string()),
    ]));
    let response = client
        .complete(
            CompleteRequestParams::new(
                Reference::for_resource(uris::WORLD_FRAME_TEMPLATE),
                ArgumentInfo::new("frame_id", ""),
            )
            .with_context(context),
        )
        .await
        .map_err(|_| anyhow!("Frames frame completion failed"))?;
    let actual = response
        .completion
        .values
        .iter()
        .map(FrameId::parse)
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let expected = revision
        .tree()
        .frames
        .iter()
        .map(|node| node.frame_id.clone())
        .collect::<BTreeSet<_>>();
    ensure!(
        actual == expected && response.completion.has_more != Some(true),
        "Frames frame completion changed revision membership"
    );
    Ok(())
}
pub(in super::super) async fn provenance(
    client: &SmokeMcpClient,
    output: &ConvertFrameOutput,
    revision: &FrameWorldRevision,
    request: &ConvertFrameRequest,
) -> Result<()> {
    require_conversion(output, revision, request)?;
    let operation: CoordinateOperationProvenance =
        installed_frames_read(client, output.provenance.operation.operation_uri().as_str()).await?;
    ensure!(
        operation == output.provenance,
        "Frames operation resource differs from conversion provenance"
    );
    Ok(())
}
pub(in super::super) fn require_conversion(
    output: &ConvertFrameOutput,
    revision: &FrameWorldRevision,
    request: &ConvertFrameRequest,
) -> Result<()> {
    ensure!(
        output.points.len() == 1
            && !output.provenance.approximation_used
            && output.sources.len() == 1,
        "Frames conversion changed its point count, approximation or source set"
    );
    ensure!(
        output.sources[0].revision_uri() == revision.revision_uri()
            && &output.sources[0].digest == revision.spec_digest(),
        "Frames conversion source does not name the traversed immutable revision and digest"
    );
    ensure!(
        output.provenance.kind == CoordinateOperationKind::FrameConversion
            && output.provenance.operation.source_frame == Some(CoordinateSpace::Wgs84),
        "Frames conversion provenance changed its operation or source space"
    );
    let [CoordinatePoint::WorldFrame(point)] = output.points.as_slice() else {
        bail!("Frames conversion returned the wrong coordinate space");
    };
    let CoordinateSpace::WorldFrame { frame_uri } = &request.target else {
        bail!("Frames fixture target is not a world frame");
    };
    ensure!(
        &point.frame_uri == frame_uri
            && output.provenance.operation.target_frame == Some(request.target.clone())
            && point.frame_uri.revision_uri() == *revision.revision_uri()
            && output.provenance.operation.target_frame
                == Some(CoordinateSpace::WorldFrame {
                    frame_uri: point.frame_uri.clone()
                })
            && [point.x_m, point.y_m, point.z_m]
                .iter()
                .all(|value| value.is_finite()),
        "Frames conversion point and target provenance disagree"
    );
    Ok(())
}
pub(super) async fn notification(subscription: &mut Subscription, uri: &str) -> Result<()> {
    let update = tokio::time::timeout(Duration::from_secs(15), subscription.next())
        .await
        .context("Frames resource observation exceeded fifteen seconds")?
        .map_err(|_| anyhow!("Frames resource stream outcome unresolved"))?
        .context("Frames resource listener ended before notification")?;
    require_worlds_invalidation(update, uri)
}
pub(super) async fn await_tasks(
    subscription: &mut Subscription,
    ids: &[(usize, CanonicalTaskId)],
    deadline: tokio::time::Instant,
) -> Result<()> {
    let expected = ids
        .iter()
        .map(|(_, id)| id.clone())
        .collect::<BTreeSet<_>>();
    tokio::time::timeout_at(
        deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
        async {
            let mut completed = BTreeSet::new();
            while completed != expected {
                let notification = subscription
                    .next()
                    .await
                    .map_err(|_| anyhow!("Frames Task observation unresolved"))?
                    .context("Frames Task listener ended before completion")?;
                let ServerNotification::TaskStatusNotification(update) = notification else {
                    bail!("Frames Task listener delivered an unexpected notification");
                };
                let id = CanonicalTaskId::parse(&update.params.task.task.task_id)
                    .map_err(|_| anyhow!("Frames Task notification identity invalid"))?;
                ensure!(
                    expected.contains(&id),
                    "Frames Task listener delivered an unrelated identity"
                );
                match update.params.task.status() {
                    TaskStatus::Completed => {
                        completed.insert(id);
                    }
                    TaskStatus::Working => {}
                    _ => bail!("Frames batch Task did not complete successfully"),
                }
            }
            Ok::<_, anyhow::Error>(())
        },
    )
    .await
    .context("Frames Task delivery exceeded thirty seconds")?
}
#[derive(Clone, serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(in super::super) enum ForeignTarget {
    Operation { uri: FrameOperationUri },
    TaskUsage { uri: FrameTaskUsageUri },
}
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(in super::super) enum ForeignMethod {
    ResourceRead,
    ListenAdmission,
    ListenTerminal,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(in super::super) enum TransportClass {
    Send,
    Closed,
    UnexpectedResponse,
    Lagged,
    Cancelled,
    Timeout,
    InputRequiredRoundsExceeded,
    UnclassifiedServiceError,
}
#[derive(serde::Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(in super::super) enum ForeignObservation {
    AwaitingResponse,
    PeerFailure {
        observed: veoveo_mcp_conformance::client::failure::ObservedFailure,
    },
    UnexpectedSuccess,
    Acknowledged {
        exact_filter: bool,
    },
    UnexpectedNotification,
    EndedWithoutDenial,
    Transport {
        class: TransportClass,
    },
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct ForeignProbe {
    pub(in super::super) method: ForeignMethod,
    pub(in super::super) target: ForeignTarget,
    pub(in super::super) expected: veoveo_mcp_conformance::client::failure::ObservedFailure,
    pub(in super::super) observed: ForeignObservation,
}
pub(in super::super) fn expected_denial(
    target: &ForeignTarget,
    method: ForeignMethod,
) -> veoveo_mcp_conformance::client::failure::ObservedFailure {
    let message = match (target, method) {
        (ForeignTarget::Operation { uri }, _) => format!("unknown operation `{uri}`"),
        (ForeignTarget::TaskUsage { uri }, ForeignMethod::ResourceRead) => {
            format!("unknown usage task `{}`", uri.task_id())
        }
        (ForeignTarget::TaskUsage { .. }, _) => "unknown usage task".into(),
    };
    veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
        i64::from(ErrorCode::INVALID_PARAMS.0),
        message,
    )
}
pub(in super::super) fn observe_peer_failure(error: ServiceError) -> ForeignObservation {
    if let ServiceError::McpError(error) = error {
        return ForeignObservation::PeerFailure {
            observed: veoveo_mcp_conformance::client::failure::ObservedFailure::mcp(
                i64::from(error.code.0),
                error.message.as_ref(),
            ),
        };
    }
    let class = match &error {
        ServiceError::TransportSend(_) => TransportClass::Send,
        ServiceError::TransportClosed => TransportClass::Closed,
        ServiceError::UnexpectedResponse => TransportClass::UnexpectedResponse,
        ServiceError::SubscriptionLagged { .. } => TransportClass::Lagged,
        ServiceError::Cancelled { .. } => TransportClass::Cancelled,
        ServiceError::Timeout { .. } => TransportClass::Timeout,
        ServiceError::InputRequiredRoundsExceeded { .. } => {
            TransportClass::InputRequiredRoundsExceeded
        }
        ServiceError::McpError(_) => unreachable!("protocol error handled above"),
        _ => TransportClass::UnclassifiedServiceError,
    };
    match veoveo_mcp_conformance::client::failure::observe(&anyhow::Error::new(error)) {
        Some(observed @ veoveo_mcp_conformance::client::failure::ObservedFailure::Http { .. }) => {
            ForeignObservation::PeerFailure { observed }
        }
        // Only a direct MCP response can establish the owner's denial. A transport
        // source chain containing ErrorData is diagnostic context, not a peer response.
        _ => ForeignObservation::Transport { class },
    }
}
fn begin_probe(
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    target: ForeignTarget,
    method: ForeignMethod,
) -> Result<usize> {
    let expected = expected_denial(&target, method);
    let index = receipt.foreign_probes.len();
    receipt.foreign_probes.push(ForeignProbe {
        method,
        target,
        expected,
        observed: ForeignObservation::AwaitingResponse,
    });
    write_frames_evidence(file, receipt)?;
    Ok(index)
}
fn finish_probe(
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
    index: usize,
    observed: ForeignObservation,
    require_denial: bool,
) -> Result<()> {
    let expected = &receipt.foreign_probes[index].expected;
    let denied =
        matches!(&observed, ForeignObservation::PeerFailure { observed } if observed == expected);
    receipt.foreign_probes[index].observed = observed;
    if require_denial && !denied {
        receipt.outcome = failure_outcome(receipt);
    }
    write_frames_evidence(file, receipt)?;
    ensure!(
        !require_denial || denied,
        "Frames foreign response did not match the current protocol denial and owner message digest; see private receipt"
    );
    Ok(())
}
pub(in super::super) fn probe_timed_out(receipt: &mut InstalledFramesReceipt) {
    for probe in &mut receipt.foreign_probes {
        if matches!(probe.observed, ForeignObservation::AwaitingResponse) {
            probe.observed = ForeignObservation::Transport {
                class: TransportClass::Timeout,
            };
        }
    }
}
pub(super) async fn require_denied_read(
    client: &SmokeMcpClient,
    target: ForeignTarget,
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
) -> Result<()> {
    let index = begin_probe(file, receipt, target.clone(), ForeignMethod::ResourceRead)?;
    let uri = match &target {
        ForeignTarget::Operation { uri } => uri.as_str(),
        ForeignTarget::TaskUsage { uri } => uri.as_str(),
    };
    let observed = match tokio::time::timeout(
        Duration::from_secs(15),
        client.read_resource(ReadResourceRequestParams::new(uri)),
    )
    .await
    {
        Ok(Err(error)) => observe_peer_failure(error),
        Ok(Ok(_)) => ForeignObservation::UnexpectedSuccess,
        Err(_) => ForeignObservation::Transport {
            class: TransportClass::Timeout,
        },
    };
    finish_probe(file, receipt, index, observed, true)
}
pub(super) async fn require_denied_subscription(
    client: &SmokeMcpClient,
    uri: &FrameTaskUsageUri,
    listeners: &mut Vec<OwnedListener>,
    file: &mut File,
    receipt: &mut InstalledFramesReceipt,
) -> Result<()> {
    let target = ForeignTarget::TaskUsage { uri: uri.clone() };
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([uri.to_string()])
        .build();
    let mut index = begin_probe(
        file,
        receipt,
        target.clone(),
        ForeignMethod::ListenAdmission,
    )?;
    let outcome = tokio::time::timeout(Duration::from_secs(15), async {
        match client.listen(filter.clone()).await {
            Err(error) => return Ok::<_, anyhow::Error>(observe_peer_failure(error)),
            Ok(listener) => listeners.push(OwnedListener::new(listener)),
        }
        let listener = listeners.last_mut().expect("foreign listener acknowledged");
        let exact_filter = listener.stream.acknowledged() == &filter;
        finish_probe(
            file,
            receipt,
            index,
            ForeignObservation::Acknowledged { exact_filter },
            false,
        )?;
        ensure!(
            exact_filter,
            "Frames foreign listener changed its filter; see private receipt"
        );
        index = begin_probe(file, receipt, target, ForeignMethod::ListenTerminal)?;
        Ok(match listener.stream.next().await {
            Err(error) => observe_peer_failure(error),
            Ok(Some(_)) => ForeignObservation::UnexpectedNotification,
            Ok(None) => ForeignObservation::EndedWithoutDenial,
        })
    })
    .await;
    let observed = match outcome {
        Ok(result) => result?,
        Err(_) => ForeignObservation::Transport {
            class: TransportClass::Timeout,
        },
    };
    finish_probe(file, receipt, index, observed, true)
}

#[cfg(test)]
mod installed_page_tests {
    use super::*;
    fn worlds() -> Result<Vec<FrameWorldSummary>> {
        let now = chrono::Utc::now();
        (0..101)
            .map(|n| {
                Ok(FrameWorldSummary::new(
                    FrameWorldId::parse(format!("page-{n:03}"))?,
                    "Owned paging fixture".into(),
                    now,
                ))
            })
            .collect()
    }
    #[test]
    fn real_cursor_continuation_requires_ordered_distinct_pages() -> Result<()> {
        let worlds = worlds()?;
        let first = FrameWorldPage {
            items: worlds[..100].to_vec(),
            limit: 100,
            next_cursor: Some(FrameWorldCursor::new(&worlds[99].world_id())),
        };
        let mut previous = None;
        let mut found = BTreeMap::new();
        let cursor = append_world_page(first.clone(), &mut previous, &mut found)?
            .context("first page cursor")?;
        assert_eq!(cursor.after(), &worlds[99].world_id());
        assert!(append_world_page(first.clone(), &mut previous, &mut found).is_err());
        let second = FrameWorldPage {
            items: vec![worlds[100].clone()],
            limit: 100,
            next_cursor: None,
        };
        assert!(append_world_page(second, &mut previous, &mut found)?.is_none());
        assert_eq!(found.len(), 101);
        assert_eq!(found.values().cloned().collect::<Vec<_>>(), worlds);
        Ok(())
    }
    #[test]
    fn internally_consistent_wrong_frame_cannot_satisfy_requested_conversion() -> Result<()> {
        let world = FrameWorldId::parse("conversion-fixture")?;
        let uri =
            FrameWorldUri::new(&world).revision(&FrameWorldRevisionId::parse("revision-fixture")?);
        let revision = FrameWorldRevision::new(
            uri,
            1.try_into()?,
            ValidatedWorldTree::new(installed_frames_tree()?)?,
            chrono::Utc::now(),
        );
        let requested = revision
            .revision_uri()
            .frame(&FrameId::parse("robot-world")?);
        let wrong = revision
            .revision_uri()
            .frame(&FrameId::parse("launch-enu")?);
        let request = ConvertFrameRequest {
            target: CoordinateSpace::WorldFrame {
                frame_uri: requested.clone(),
            },
            points: vec![CoordinatePoint::Wgs84(Wgs84Position {
                latitude_degrees: 0.0,
                longitude_degrees: 0.0,
                ellipsoid_height_m: 0.0,
            })],
            allow_approximation: false,
        };
        let mut output = ConvertFrameOutput {
            points: vec![CoordinatePoint::WorldFrame(WorldFramePosition {
                frame_uri: requested.clone(),
                x_m: 0.0,
                y_m: 0.0,
                z_m: 0.0,
            })],
            sources: vec![FrameSourceReference::from(&revision)],
            provenance: CoordinateOperationProvenance {
                operation: CoordinateOperationRef::new(
                    CoordinateOperationId::parse("op-fixture")?,
                    chrono::Utc::now(),
                )
                .with_frames(Some(CoordinateSpace::Wgs84), Some(request.target.clone())),
                kind: CoordinateOperationKind::FrameConversion,
                source_crs: None,
                target_crs: None,
                engine: None,
                grid_packages: Vec::new(),
                approximation_used: false,
                accuracy_m: None,
                warnings: Vec::new(),
            },
        };
        require_conversion(&output, &revision, &request)?;
        output.points = vec![CoordinatePoint::WorldFrame(WorldFramePosition {
            frame_uri: wrong.clone(),
            x_m: 0.0,
            y_m: 0.0,
            z_m: 0.0,
        })];
        output.provenance.operation.target_frame =
            Some(CoordinateSpace::WorldFrame { frame_uri: wrong });
        assert!(require_conversion(&output, &revision, &request).is_err());
        Ok(())
    }
    #[test]
    fn wrong_bound_and_unusable_cursors_cannot_establish_paging() -> Result<()> {
        let worlds = worlds()?;
        for page in [
            FrameWorldPage {
                items: worlds[..100].to_vec(),
                limit: 99,
                next_cursor: None,
            },
            FrameWorldPage {
                items: worlds[..100].to_vec(),
                limit: 100,
                next_cursor: Some(FrameWorldCursor::new(&worlds[98].world_id())),
            },
            FrameWorldPage {
                items: vec![worlds[0].clone()],
                limit: 100,
                next_cursor: Some(FrameWorldCursor::new(&worlds[0].world_id())),
            },
            FrameWorldPage {
                items: worlds,
                limit: 100,
                next_cursor: None,
            },
        ] {
            assert!(append_world_page(page, &mut None, &mut BTreeMap::new()).is_err());
        }
        Ok(())
    }
}

#[cfg(test)]
mod foreign_protocol_tests {
    use super::*;
    use rmcp::{ClientServiceExt, ServerHandler, ServiceExt};
    fn receipt() -> Result<InstalledFramesReceipt> {
        Ok(InstalledFramesReceipt {
            schema_version: "veoveo.ai/frames-installed-evidence/v3",
            world_id: FrameWorldId::parse("foreign-probe-fixture")?,
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
    fn operation() -> Result<ForeignTarget> {
        Ok(ForeignTarget::Operation {
            uri: FrameOperationUri::new(&CoordinateOperationId::parse("op-wire-fixture")?),
        })
    }
    #[derive(Clone)]
    struct MissingResources;
    impl ServerHandler for MissingResources {
        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(
                ServerCapabilities::builder()
                    .enable_resources()
                    .enable_resources_subscribe()
                    .build(),
            )
        }
        fn accepted_subscription_filter(
            &self,
            filter: &SubscriptionFilter,
        ) -> Option<SubscriptionFilter> {
            Some(filter.clone())
        }
        async fn read_resource(
            &self,
            params: ReadResourceRequestParams,
            _: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> std::result::Result<ReadResourceResponse, rmcp::ErrorData> {
            Err(rmcp::ErrorData::resource_not_found(
                format!("unknown operation `{}`", params.uri),
                None,
            ))
        }
        async fn listen(
            &self,
            _: rmcp::service::SubscriptionContext,
        ) -> std::result::Result<(), rmcp::ErrorData> {
            Err(rmcp::ErrorData::resource_not_found(
                "unknown usage task",
                None,
            ))
        }
    }
    #[tokio::test]
    async fn current_protocol_read_and_acknowledged_terminal_denial_match_owner_messages()
    -> Result<()> {
        let (server_io, client_io) = tokio::io::duplex(8192);
        let serving = tokio::spawn(async move { MissingResources.serve(server_io).await });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        let server = serving.await??;
        let directory = tempfile::tempdir()?;
        let mut file = admit_frames_evidence(&directory.path().join("receipt.json"))?;
        let mut receipt = receipt()?;
        let mut listener = None;
        let outcome = tokio::time::timeout(Duration::from_secs(5), async {
            let target = operation()?;
            let ForeignTarget::Operation { uri } = &target else {
                unreachable!()
            };
            let index = begin_probe(
                &mut file,
                &mut receipt,
                target.clone(),
                ForeignMethod::ResourceRead,
            )?;
            let observed = match client
                .read_resource(ReadResourceRequestParams::new(uri.as_str()))
                .await
            {
                Err(error) => observe_peer_failure(error),
                Ok(_) => ForeignObservation::UnexpectedSuccess,
            };
            finish_probe(&mut file, &mut receipt, index, observed, true)?;
            let usage = FrameTaskUsageUri::new(TaskId::new())?;
            let target = ForeignTarget::TaskUsage { uri: usage.clone() };
            let filter = SubscriptionFilter::builder()
                .resource_subscriptions([usage.to_string()])
                .build();
            let index = begin_probe(
                &mut file,
                &mut receipt,
                target.clone(),
                ForeignMethod::ListenAdmission,
            )?;
            listener = Some(client.listen(filter.clone()).await?);
            let stream = listener.as_mut().expect("acknowledged listener");
            ensure!(
                stream.acknowledged() == &filter,
                "test peer changed acknowledged filter"
            );
            finish_probe(
                &mut file,
                &mut receipt,
                index,
                ForeignObservation::Acknowledged { exact_filter: true },
                false,
            )?;
            let index = begin_probe(
                &mut file,
                &mut receipt,
                target,
                ForeignMethod::ListenTerminal,
            )?;
            let observed = match stream.next().await {
                Err(error) => observe_peer_failure(error),
                Ok(Some(_)) => ForeignObservation::UnexpectedNotification,
                Ok(None) => ForeignObservation::EndedWithoutDenial,
            };
            finish_probe(&mut file, &mut receipt, index, observed, true)?;
            Ok::<_, anyhow::Error>(())
        })
        .await;
        let listener_closed = immutable_subscription::cancel_owned(&mut listener).await;
        let client_closed = matches!(
            tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
            Ok(Ok(_))
        );
        let server_closed = matches!(
            tokio::time::timeout(Duration::from_secs(5), server.cancel()).await,
            Ok(Ok(_))
        );
        outcome.context("current protocol control deadline")??;
        ensure!(
            listener_closed && client_closed && server_closed,
            "current protocol fixture cleanup unresolved"
        );
        assert_eq!(receipt.foreign_probes.len(), 3);
        Ok(())
    }
    #[test]
    fn wrong_code_message_http_and_success_are_recorded_before_rejection_without_payloads()
    -> Result<()> {
        let target = operation()?;
        let ForeignTarget::Operation { uri } = &target else {
            unreachable!()
        };
        let message = format!("unknown operation `{uri}`");
        let sentinel = "never-copy-bearer-or-response-body";
        let wrapped = observe_peer_failure(ServiceError::TransportSend(
            rmcp::transport::DynamicTransportError::from_parts(
                "isolated-test-transport",
                std::any::TypeId::of::<()>(),
                Box::new(rmcp::ErrorData::invalid_params(message.clone(), None)),
            ),
        ));
        assert!(matches!(
            &wrapped,
            ForeignObservation::Transport {
                class: TransportClass::Send
            }
        ));
        let observations = [
            wrapped,
            observe_peer_failure(ServiceError::McpError(rmcp::ErrorData::new(
                ErrorCode::RESOURCE_NOT_FOUND,
                message,
                None,
            ))),
            observe_peer_failure(ServiceError::McpError(rmcp::ErrorData::invalid_params(
                sentinel,
                Some(serde_json::json!({"body": sentinel})),
            ))),
            ForeignObservation::PeerFailure {
                observed: veoveo_mcp_conformance::client::failure::ObservedFailure::Http {
                    status: 403,
                },
            },
            ForeignObservation::UnexpectedSuccess,
            observe_peer_failure(ServiceError::TransportClosed),
        ];
        for (n, observation) in observations.into_iter().enumerate() {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join(format!("receipt-{n}.json"));
            let mut file = admit_frames_evidence(&path)?;
            let mut receipt = receipt()?;
            let index = begin_probe(
                &mut file,
                &mut receipt,
                target.clone(),
                ForeignMethod::ResourceRead,
            )?;
            assert!(finish_probe(&mut file, &mut receipt, index, observation, true).is_err());
            let text = fs::read_to_string(path)?;
            assert!(!text.contains(sentinel));
            assert!(!text.contains("awaiting_response"));
            assert!(text.contains("resource_read"));
            assert!(text.contains(uri.as_str()));
        }
        Ok(())
    }
    #[test]
    fn timeout_retains_the_requested_method_and_target() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let mut receipt = receipt()?;
        begin_probe(
            &mut file,
            &mut receipt,
            operation()?,
            ForeignMethod::ResourceRead,
        )?;
        probe_timed_out(&mut receipt);
        write_frames_evidence(&mut file, &receipt)?;
        let text = fs::read_to_string(path)?;
        assert!(
            text.contains("resource_read")
                && text.contains("timeout")
                && !text.contains("awaiting_response")
        );
        Ok(())
    }
}
