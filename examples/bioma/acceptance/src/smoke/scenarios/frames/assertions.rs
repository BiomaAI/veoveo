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
pub(super) async fn provenance(
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
fn require_conversion(
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
pub(super) async fn require_denied_read(client: &SmokeMcpClient, uri: &str) -> Result<()> {
    match client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
    {
        Err(ServiceError::McpError(error)) if error.code == ErrorCode::RESOURCE_NOT_FOUND => Ok(()),
        _ => bail!("Frames foreign read did not return resource not found"),
    }
}
pub(super) async fn require_denied_subscription(
    client: &SmokeMcpClient,
    uri: &str,
    listeners: &mut Vec<OwnedListener>,
) -> Result<()> {
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([uri])
        .build();
    tokio::time::timeout(Duration::from_secs(15), async {
        match client.listen(filter.clone()).await {
            Err(ServiceError::McpError(error)) if error.code == ErrorCode::RESOURCE_NOT_FOUND => {
                return Ok(());
            }
            Err(_) => bail!("Frames foreign listener returned a different admission failure"),
            Ok(listener) => listeners.push(OwnedListener::new(listener)),
        }
        let listener = listeners.last_mut().expect("foreign listener acknowledged");
        ensure!(
            listener.stream.acknowledged() == &filter,
            "Frames foreign listener changed its filter"
        );
        match listener.stream.next().await {
            Err(ServiceError::McpError(error)) if error.code == ErrorCode::RESOURCE_NOT_FOUND => {
                Ok(())
            }
            _ => bail!("Frames foreign subscription did not terminate with resource not found"),
        }
    })
    .await
    .context("Frames foreign subscription admission exceeded fifteen seconds")?
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
