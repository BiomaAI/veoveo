use super::*;
use admin::{Request, active, binding};
use serde::{Serialize, de::DeserializeOwned};

async fn read<T: DeserializeOwned + Serialize>(
    caller: &SmokeMcpClient,
    journal: &Arc<Mutex<Journal>>,
    resource: TimeResource,
) -> Result<T> {
    let mut journal = journal.lock().await;
    journal
        .evidence
        .trace
        .begin(trace::Request::Read(resource.clone()));
    journal.persist()?;
    let result = installed::read(caller.peer(), &resource.to_uri()?).await;
    journal.evidence.trace.finish(&result);
    journal.persist()?;
    result
}
async fn resolve(
    caller: &SmokeMcpClient,
    journal: &Arc<Mutex<Journal>>,
    input: &Input,
) -> Result<ResolveTimeOutput> {
    let request = relative(input)?;
    let mut journal = journal.lock().await;
    journal
        .evidence
        .trace
        .begin(trace::Request::Resolve(request.clone()));
    journal.persist()?;
    let result = veoveo_testing_support::installed::tools::call(
        caller.peer(),
        "time__resolve_time".parse()?,
        &request,
    )
    .await;
    journal.evidence.trace.finish(&result);
    journal.persist()?;
    result
}
fn relative(input: &Input) -> Result<ResolveTimeRequest> {
    Ok(ResolveTimeRequest {
        expression: TimeExpressionValue::EpochRelative {
            epoch_id: input.epoch.epoch_id.clone(),
            offset_nanoseconds: input.relative_offset_nanoseconds,
        }
        .build()?,
        additional_uncertainty_nanoseconds: 0,
    })
}
pub(super) fn delivered(
    notification: &ServerNotification,
    id: &rmcp::model::RequestId,
    filter: &SubscriptionFilter,
    acknowledged: &SubscriptionFilter,
) -> Result<()> {
    ensure!(
        filter == acknowledged,
        "authority exact subscription filter not acknowledged"
    );
    ensure!(
        notification.get_meta().subscription_id().as_ref() == Some(id),
        "authority subscription identity differs"
    );
    let ServerNotification::ResourceUpdatedNotification(update) = notification else {
        anyhow::bail!("authority listener delivered another notification");
    };
    ensure!(
        TimeResource::parse(&update.params.uri)? == TimeResource::AuthoritiesCurrent,
        "authority notification resource differs"
    );
    Ok(())
}
async fn next(listener: &mut Subscription, filter: &SubscriptionFilter) -> Result<()> {
    let notification = tokio::time::timeout(Duration::from_secs(20), listener.next())
        .await
        .context("authority resource delivery deadline")??
        .context("authority listener ended")?;
    delivered(
        &notification,
        listener.id(),
        filter,
        listener.acknowledged(),
    )
}
async fn snapshot(
    admin: &admin::Admin,
    journal: &Arc<Mutex<Journal>>,
) -> Result<Vec<ActiveAuthoritySelection>> {
    let selected = active(admin).await?;
    let mut journal = journal.lock().await;
    journal.evidence.active_snapshots.push(selected.clone());
    journal.persist()?;
    Ok(selected)
}
async fn acquired(
    admin: &admin::Admin,
    source: &input::SourceFixture,
    key: &str,
) -> Result<AuthorityRelease> {
    let request: CreateAcquisitionRequest = CreateAcquisitionRequestValue {
        source_id: source.create.source.source_id.clone(),
        expected_source_digest_sha256: Some(source.digest.clone()),
        idempotency_key: key.to_owned(),
    }
    .try_into()?;
    let initial: TimeAcquisition = admin.ok(Request::AcquisitionCreate(request)).await?;
    ensure!(
        initial.source_id == source.create.source.source_id
            && initial.expected_source_digest_sha256.as_ref() == Some(&source.digest),
        "acquisition acknowledgement differs"
    );
    // The documented administration API exposes correlated status GETs. This owner
    // profile permits at most 66 observations, separated by five seconds, per acknowledged job.
    let job = tokio::time::timeout_at(
        admin
            .deadline
            .min(tokio::time::Instant::now() + Duration::from_secs(330)),
        async {
            for attempt in 0..66 {
                let job: TimeAcquisition = admin
                    .ok(Request::AcquisitionGet(initial.acquisition_id.clone()))
                    .await?;
                ensure!(
                    job.acquisition_id == initial.acquisition_id
                        && job.source_id == initial.source_id
                        && job.expected_source_digest_sha256
                            == initial.expected_source_digest_sha256,
                    "acquisition status identity changed"
                );
                match job.status {
                    TimeAcquisitionStatus::Succeeded => {
                        ensure!(
                            job.phase == TimeAcquisitionPhase::Complete
                                && job.staged_release_id.is_some(),
                            "successful acquisition lacks staged release"
                        );
                        return Ok(job);
                    }
                    TimeAcquisitionStatus::Failed | TimeAcquisitionStatus::Cancelled => {
                        anyhow::bail!("selected acquisition did not succeed")
                    }
                    _ => (),
                }
                if attempt < 65 {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
            anyhow::bail!("acquisition observation request budget exhausted")
        },
    )
    .await
    .context("acquisition observation deadline; outcome unresolved")??;
    let release: AuthorityRelease = admin
        .ok(Request::ReleaseGet(job.staged_release_id.unwrap()))
        .await?;
    ensure!(
        release.state == AuthorityReleaseState::Staged
            && release.record_version.get() == 1
            && release.source_id == source.create.source.source_id
            && release.dataset_kind == source.create.source.dataset_kind
            && release.version_label == source.expected_version_label
            && release.source_url == source.create.source.url
            && release.source_digest_sha256.canonical() == source.digest.canonical(),
        "staged release differs from independent source fixture"
    );
    Ok(release)
}
fn guard(selected: &[ActiveAuthoritySelection], family: AuthorityDatasetKind) -> TimeWriteGuard {
    selected
        .iter()
        .find(|entry| entry.release.dataset_kind == family)
        .map_or(TimeWriteGuard::Absent, |entry| entry.write_guard())
}
pub(super) fn activation(
    candidate: &AuthorityRelease,
    selected: &[ActiveAuthoritySelection],
) -> ActivateReleaseRequest {
    ActivateReleaseRequest {
        expected_release_record_version: candidate.record_version,
        expected_active_pointer_version: guard(selected, candidate.dataset_kind),
    }
}
pub(super) fn refused(status: u16, code: Option<AdminErrorCode>) -> Result<()> {
    ensure!(
        status == 409 && code == Some(AdminErrorCode::VersionConflict),
        "guard conflict refusal required"
    );
    Ok(())
}
async fn changed(
    caller: &SmokeMcpClient,
    listener: &mut Subscription,
    filter: &SubscriptionFilter,
    before: &EffectiveTimeAuthority,
    expected: &[ActiveAuthoritySelection],
    journal: &Arc<Mutex<Journal>>,
) -> Result<EffectiveTimeAuthority> {
    next(listener, filter).await?;
    let current: EffectiveTimeAuthority =
        read(caller, journal, TimeResource::AuthoritiesCurrent).await?;
    changed_binding(before, &current)?;
    for selected in expected {
        let reference = match selected.release.dataset_kind {
            AuthorityDatasetKind::Tzdb => current.tzdb(),
            AuthorityDatasetKind::LeapSeconds => current.leap_seconds(),
        };
        ensure!(
            reference.release_id() == &selected.release.release_id
                && reference.source_digest() == selected.release.source_digest_sha256.canonical()
                && reference.version_label() == selected.release.version_label
                && reference.dataset_kind() == selected.release.dataset_kind,
            "current authority selection/provenance differs"
        );
    }
    let mut journal = journal.lock().await;
    journal.evidence.delivered_changes.push(current.clone());
    journal.persist()?;
    Ok(current)
}
async fn provenance(
    admin: &admin::Admin,
    current: &EffectiveTimeAuthority,
    selected: &[ActiveAuthoritySelection],
) -> Result<()> {
    for selected in selected {
        let reference = match selected.release.dataset_kind {
            AuthorityDatasetKind::Tzdb => current.tzdb(),
            AuthorityDatasetKind::LeapSeconds => current.leap_seconds(),
        };
        let veoveo_time_mcp::TimeAuthoritySource::Acquisition {
            source_id,
            acquisition_id,
        } = reference.source()
        else {
            anyhow::bail!("activated reference must name acquisition provenance");
        };
        let job: TimeAcquisition = admin
            .ok(Request::AcquisitionGet(acquisition_id.clone()))
            .await?;
        ensure!(
            job.status == TimeAcquisitionStatus::Succeeded
                && job.phase == TimeAcquisitionPhase::Complete
                && job.staged_release_id.as_ref() == Some(&selected.release.release_id)
                && &job.source_id == source_id
                && source_id == &selected.release.source_id
                && job
                    .expected_source_digest_sha256
                    .as_ref()
                    .map(AuthoritySourceDigest::canonical)
                    == Some(reference.source_digest()),
            "activated reference differs from acknowledged acquisition provenance"
        );
    }
    Ok(())
}
pub(super) fn changed_binding(
    before: &EffectiveTimeAuthority,
    current: &EffectiveTimeAuthority,
) -> Result<()> {
    ensure!(
        current.binding() != before.binding(),
        "initial/current snapshot cannot qualify an authority change"
    );
    Ok(())
}
pub(super) async fn exercise(
    input: &Input,
    admin: &admin::Admin,
    handles: &mut Handles,
    journal: &Arc<Mutex<Journal>>,
) -> Result<()> {
    let caller = handles
        .caller
        .as_ref()
        .context("authority caller missing")?;
    let initial: EffectiveTimeAuthority =
        read(caller, journal, TimeResource::AuthoritiesCurrent).await?;
    ensure!(
        initial == input.initial_authority,
        "initial effective authority differs"
    );
    ensure!(
        snapshot(admin, journal).await?.is_empty(),
        "isolated authority profile requires absent persisted pointers"
    );
    let sources: AdminPage<TimeSource> = admin.ok(Request::Sources).await?;
    ensure!(
        sources.items.is_empty() && sources.next_cursor.is_none(),
        "isolated source catalog must initially be empty"
    );
    for source in [&input.tzdb, &input.leaps] {
        let created: TimeSource = admin
            .ok(Request::SourceCreate(source.create.clone()))
            .await?;
        ensure!(
            created.source_id == source.create.source.source_id
                && created.dataset_kind == source.create.source.dataset_kind
                && created.url == source.create.source.url
                && created.expected_content_type == source.create.source.expected_content_type
                && created.enabled
                && created.record_version.get() == 1,
            "source create acknowledgement differs"
        );
    }
    let epoch_request: UpsertMissionEpochRequest = UpsertMissionEpochRequestValue {
        epoch: input.epoch.clone(),
        idempotency_key: input.epoch_idempotency_keys[0].clone(),
    }
    .try_into()?;
    let epoch: MissionEpoch = admin.ok(Request::EpochCreate(epoch_request)).await?;
    ensure!(epoch == input.epoch, "epoch v1 acknowledgement differs");
    let before = resolve(caller, journal, input).await?;
    ensure!(
        before.instant().total_nanoseconds() == input.expected_relative_tai_nanoseconds
            && before.effective_authority() == &initial,
        "initial epoch-relative physical expectation differs"
    );
    let uri = TimeResource::AuthoritiesCurrent.to_uri()?;
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([uri.to_string()])
        .build();
    handles.listener = Some(caller.listen(filter.clone()).await?);
    let listener = handles.listener.as_mut().unwrap();
    {
        let mut journal = journal.lock().await;
        journal.evidence.listener_closed = false;
        journal.evidence.subscription_id = Some(listener.id().clone());
        journal.evidence.subscription_filter = Some(listener.acknowledged().clone());
        journal.persist()?;
    }
    next(listener, &filter).await?;
    let baseline: EffectiveTimeAuthority =
        read(caller, journal, TimeResource::AuthoritiesCurrent).await?;
    ensure!(
        baseline == initial,
        "subscription baseline authority differs"
    );
    {
        let mut journal = journal.lock().await;
        journal.evidence.initial_subscription_authority = Some(baseline);
        journal.phase(Phase::InitialObserved)?;
    }
    journal.lock().await.phase(Phase::Acquisition)?;
    let (tz, leap_a, leap_b, leap_c) = tokio::try_join!(
        acquired(admin, &input.tzdb, &input.tzdb.acquisition_keys[0]),
        acquired(admin, &input.leaps, &input.leaps.acquisition_keys[0]),
        acquired(admin, &input.leaps, &input.leaps.acquisition_keys[1]),
        acquired(admin, &input.leaps, &input.leaps.acquisition_keys[2]),
    )?;
    ensure!(
        leap_a.release_id != leap_b.release_id
            && leap_b.release_id != leap_c.release_id
            && leap_a.release_id != leap_c.release_id,
        "acquisitions reused release identities"
    );
    let replay: CreateAcquisitionRequest = CreateAcquisitionRequestValue {
        source_id: input.tzdb.create.source.source_id.clone(),
        expected_source_digest_sha256: Some(input.tzdb.digest.clone()),
        idempotency_key: input.tzdb.acquisition_keys[0].clone(),
    }
    .try_into()?;
    let replayed: TimeAcquisition = admin.ok(Request::AcquisitionCreate(replay)).await?;
    ensure!(
        replayed.status == TimeAcquisitionStatus::Succeeded
            && replayed.staged_release_id.as_ref() == Some(&tz.release_id),
        "known acquisition idempotency replay differs"
    );
    let conflict: CreateAcquisitionRequest = CreateAcquisitionRequestValue {
        source_id: input.leaps.create.source.source_id.clone(),
        expected_source_digest_sha256: Some(input.leaps.digest.clone()),
        idempotency_key: input.tzdb.acquisition_keys[0].clone(),
    }
    .try_into()?;
    let (status, _, code) = admin
        .request::<TimeAcquisition>(Request::AcquisitionCreate(conflict))
        .await?;
    refused(status, code)?;
    journal.lock().await.phase(Phase::Activation)?;
    let mut selected = vec![];
    let mut current = initial;
    for candidate in [&tz, &leap_a] {
        let activated: AuthorityRelease = admin
            .ok(Request::Activate {
                id: candidate.release_id.clone(),
                request: activation(candidate, &selected),
            })
            .await?;
        ensure!(
            activated.release_id == candidate.release_id
                && activated.state == AuthorityReleaseState::Active,
            "activation acknowledgement differs"
        );
        let previous = selected;
        selected = snapshot(admin, journal).await?;
        ensure!(
            selected.len() == previous.len() + 1
                && previous.iter().all(|entry| selected.contains(entry)),
            "first activation changed another family"
        );
        ensure!(
            binding(&selected, candidate.dataset_kind)?
                .release
                .release_id
                == candidate.release_id
                && binding(&selected, candidate.dataset_kind)?
                    .pointer_version
                    .get()
                    == 1
                && binding(&selected, candidate.dataset_kind)?
                    .release
                    .record_version
                    .get()
                    == 2,
            "first pointer version must differ from active release version"
        );
        current = changed(caller, listener, &filter, &current, &selected, journal).await?;
        provenance(admin, &current, &selected).await?;
    }
    journal.lock().await.phase(Phase::ConcurrentActivation)?;
    let prior = selected.clone();
    let request_b = activation(&leap_b, &prior);
    let request_c = activation(&leap_c, &prior);
    let (b, c) = tokio::try_join!(
        admin.request::<AuthorityRelease>(Request::Activate {
            id: leap_b.release_id.clone(),
            request: request_b
        }),
        admin.request::<AuthorityRelease>(Request::Activate {
            id: leap_c.release_id.clone(),
            request: request_c
        })
    )?;
    let (winner, loser, winning, losing) =
        match ((200..300).contains(&b.0), (200..300).contains(&c.0)) {
            (true, false) => (&leap_b, &leap_c, b, c),
            (false, true) => (&leap_c, &leap_b, c, b),
            _ => anyhow::bail!("concurrent activation requires exactly one success"),
        };
    let acknowledged = winning
        .1
        .context("winning activation acknowledgement missing")?;
    ensure!(
        acknowledged.release_id == winner.release_id
            && acknowledged.state == AuthorityReleaseState::Active
            && acknowledged.record_version.get() == 2,
        "winning activation acknowledgement differs"
    );
    refused(losing.0, losing.2)?;
    selected = snapshot(admin, journal).await?;
    ensure!(
        binding(&selected, AuthorityDatasetKind::Tzdb)?
            == binding(&prior, AuthorityDatasetKind::Tzdb)?
            && binding(&selected, AuthorityDatasetKind::LeapSeconds)?
                .pointer_version
                .get()
                == 2
            && binding(&selected, AuthorityDatasetKind::LeapSeconds)?
                .release
                .release_id
                == winner.release_id,
        "concurrent cut advanced authority incorrectly"
    );
    let retired: AuthorityRelease = admin
        .ok(Request::ReleaseGet(leap_a.release_id.clone()))
        .await?;
    let staged: AuthorityRelease = admin
        .ok(Request::ReleaseGet(loser.release_id.clone()))
        .await?;
    ensure!(
        retired.state == AuthorityReleaseState::Retired
            && retired.record_version.get() == 3
            && staged == *loser,
        "conflict did not preserve release rollback"
    );
    current = changed(caller, listener, &filter, &current, &selected, journal).await?;
    provenance(admin, &current, &selected).await?;
    journal.lock().await.phase(Phase::StaleRefusal)?;
    let (status, _, code) = admin
        .request::<AuthorityRelease>(Request::Activate {
            id: loser.release_id.clone(),
            request: activation(loser, &prior),
        })
        .await?;
    refused(status, code)?;
    ensure!(
        snapshot(admin, journal).await? == selected,
        "stale guard mutated active selection"
    );
    let after_loser: AuthorityRelease = admin
        .ok(Request::ReleaseGet(loser.release_id.clone()))
        .await?;
    let after_previous: AuthorityRelease = admin
        .ok(Request::ReleaseGet(leap_a.release_id.clone()))
        .await?;
    ensure!(
        after_loser == staged && after_previous == retired,
        "stale guard mutated retained releases"
    );
    let retained: MissionEpoch = read(
        caller,
        journal,
        TimeResource::EpochVersion {
            id: epoch.epoch_id.clone(),
            version: epoch.version,
        },
    )
    .await?;
    ensure!(retained == epoch, "activation rewrote immutable epoch v1");
    let stale = resolve(caller, journal, input).await;
    let stale_error = stale.err().context("stale epoch unexpectedly resolved")?;
    ensure!(stale_error.chain().any(|cause| matches!(cause.downcast_ref::<rmcp::ServiceError>(),
        Some(rmcp::ServiceError::McpError(data)) if data.code.0 == -32602 && data.message == "instant references a non-active temporal authority")),
        "stale epoch must fail for inactive authority, not transport or permissions");
    let epoch_v2 = MissionEpoch {
        epoch_id: epoch.epoch_id.clone(),
        name: epoch.name.clone(),
        version: veoveo_time_mcp::TimeVersion::new(2)?,
        instant: TimeInstant::from_total_nanoseconds(
            epoch.instant.total_nanoseconds(),
            epoch.instant.uncertainty_nanoseconds,
            current.binding(),
        )?,
    };
    let request: UpsertMissionEpochRequest = UpsertMissionEpochRequestValue {
        epoch: epoch_v2.clone(),
        idempotency_key: input.epoch_idempotency_keys[1].clone(),
    }
    .try_into()?;
    let published: MissionEpoch = admin.ok(Request::EpochCreate(request)).await?;
    ensure!(published == epoch_v2, "epoch v2 acknowledgement differs");
    let latest: MissionEpoch =
        read(caller, journal, TimeResource::Epoch(epoch.epoch_id.clone())).await?;
    let retained: MissionEpoch = read(
        caller,
        journal,
        TimeResource::EpochVersion {
            id: epoch.epoch_id.clone(),
            version: epoch.version,
        },
    )
    .await?;
    ensure!(
        latest == epoch_v2 && retained == epoch,
        "epoch latest/history changed incorrectly"
    );
    let resolved = resolve(caller, journal, input).await?;
    ensure!(
        resolved.instant().total_nanoseconds() == input.expected_relative_tai_nanoseconds
            && resolved.instant().uncertainty_nanoseconds == epoch.instant.uncertainty_nanoseconds
            && resolved.effective_authority() == &current
            && resolved.projection().utc_rfc3339 == input.expected_utc,
        "rebound epoch differs from independent physical/projection expectation"
    );
    let request = ConvertTimeRequest {
        instant: resolved.instant().clone(),
        zone_ids: vec!["UTC".into()],
        scales: vec![TimeScale::Utc, TimeScale::Tai],
    };
    let converted: ConvertTimeOutput = {
        let mut journal = journal.lock().await;
        journal
            .evidence
            .trace
            .begin(trace::Request::Convert(request.clone()));
        journal.persist()?;
        let result = veoveo_testing_support::installed::tools::call(
            caller.peer(),
            "time__convert_time".parse()?,
            &request,
        )
        .await;
        journal.evidence.trace.finish(&result);
        journal.persist()?;
        result?
    };
    ensure!(
        converted.canonical == resolved
            && converted.zoned.len() == 1
            && converted.zoned[0].zone_id == "UTC"
            && converted
                .scales
                .iter()
                .map(|scale| scale.scale)
                .collect::<Vec<_>>()
                == request.scales,
        "rebound conversion selections or physical result differ"
    );
    ensure!(
        snapshot(admin, journal).await? == selected,
        "authority changed during epoch qualification"
    );
    journal.lock().await.phase(Phase::EpochRebound)?;
    Ok(())
}
