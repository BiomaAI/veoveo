//! Immutable signing-key replacement after actual HTTP/2 serving-instance drain.
use super::*;
use anyhow::{Context, Result, ensure};
use futures::FutureExt as _;
use re_auth::{Jwt, RedapProvider, VerificationOptions};
use re_protos::cloud::v1alpha1 as proto;
use veoveo_recording_store::RecordingReadGrantRecord;

#[derive(Default)]
struct Wires {
    old: Option<RedapWire>,
    diagnostic: Option<RedapWire>,
    replacement: Option<RedapWire>,
}
impl Wires {
    async fn close(&mut self) -> Result<()> {
        // Attempt every retained transport even if a preceding drain failed.
        let mut failed = false;
        for slot in [&mut self.replacement, &mut self.diagnostic, &mut self.old] {
            if let Some(wire) = slot.as_mut() {
                if wire.close().await.is_ok() {
                    *slot = None;
                } else {
                    failed = true;
                }
            }
        }
        ensure!(
            !failed,
            "one or more playback fixture transports did not drain"
        );
        Ok(())
    }
}

#[tokio::test]
async fn signing_key_retirement_requires_drained_old_transport() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = fixture::TestDb::with_modules(vec![veoveo_recording_store::schema::module_setup(
        fixture::module_lanes::execution("recordings")?,
    )?])
    .await;
    let spool = tempfile::tempdir()?;
    let cache = tempfile::tempdir()?;
    let caller = identity("key-retirement", "reader", &["restricted"]);
    let mut artifact = None;
    let mut wires = Wires::default();
    // Timeout/panic drops only the operation. Retained transports are closed afterward.
    let outcome =
        std::panic::AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(90), async {
            artifact = Some(
                super::super::seal_recovery::PlaybackFixture::new(&db, &caller, spool.path()).await,
            );
            let artifact = artifact.as_ref().unwrap();
            exercise(
                &db,
                artifact,
                spool.path(),
                cache.path(),
                &caller,
                &mut wires,
            )
            .await
        }))
        .catch_unwind()
        .await;
    let redap_closed = wires.close().await;
    let artifact_closed = if let Some(artifact) = artifact.as_mut() {
        artifact.close().await
    } else {
        Ok(())
    };
    // Teardown precedes propagation of every operation error, timeout and assertion panic.
    redap_closed?;
    artifact_closed?;
    match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => anyhow::bail!("playback signing-key qualification exceeded90seconds"),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

async fn prepare(
    service: &RecordingService,
    manager: &PlaybackManager,
    caller: &GatewayInternalIdentity,
    recording: RecordingId,
    reuse: Option<veoveo_recording_mcp::contract::RecordingReadGrantId>,
) -> Result<(PlaybackManifest, RecordingReadGrantRecord)> {
    let reader = artifact_reader(caller);
    let plan = service
        .playback_plan(
            caller,
            Some(&reader),
            recording,
            PlaybackArchiveSelection::Complete,
        )
        .await?
        .context("current caller cannot admit the selected playback recording")?;
    let grant = service
        .issue_read_grant(
            caller,
            plan.dataset_id,
            RecordingReadGrantClass::ViewerSegment,
            vec![recording],
            plan.catalog_revision.clone(),
            reuse,
        )
        .await?;
    let manifest = manager.prepare_manifest(plan, grant.clone()).await?;
    Ok((manifest, grant))
}

async fn exercise(
    db: &fixture::TestDb,
    artifact: &super::super::seal_recovery::PlaybackFixture,
    spool: &Path,
    cache: &Path,
    caller: &GatewayInternalIdentity,
    wires: &mut Wires,
) -> Result<()> {
    let dataset = artifact.dataset().await;
    let (recording, expected) = artifact
        .recording(caller, dataset, spool, "key-retained")
        .await;
    let (excluded, _) = artifact
        .recording(caller, dataset, spool, "key-excluded")
        .await;
    let source_bytes = std::fs::read(spool.join("key-retained.rrd"))?;
    let source_sha =
        veoveo_types::Sha256Digest::from_bytes(sha2::Sha256::digest(&source_bytes).into());
    let service = artifact.service(db.b.clone(), spool, cache);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let retired_address = listener.local_addr()?;
    let origin = format!("http://{retired_address}");
    let old_key = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
    let new_key = base64::engine::general_purpose::STANDARD.encode([9_u8; 32]);
    let old_manager = PlaybackManager::new(&old_key, &origin, db.b.clone())?;
    let (old, retained_grant) = prepare(&service, &old_manager, caller, recording, None).await?;
    let grant_id = old.access.grant_id;
    ensure!(
        retained_grant.recordings == vec![recording.record_id()]
            && retained_grant.dataset == dataset.record_id()
            && retained_grant.grant_class == RecordingReadGrantClass::ViewerSegment,
        "initial grant scope differs"
    );
    wires.old = Some(RedapWire::from_listener(&old_manager, listener));
    exact_bytes(
        wires.old.as_mut().unwrap(),
        &old.access.redap_token,
        dataset,
        &expected,
    )
    .await?;
    read_only(wires.old.as_mut().unwrap(), &old.access.redap_token).await?;

    // Independently verify a live, unexpired token under each immutable key.
    let old_provider = RedapProvider::from_secret_key_base64(&old_key)?;
    let new_provider = RedapProvider::from_secret_key_base64(&new_key)?;
    let token = Jwt::try_from(old.access.redap_token.clone())?;
    let old_claims = old_provider.verify(&token, VerificationOptions::default())?;
    ensure!(
        old_claims.has_read_permission() && old_claims.sub() == grant_id.to_string(),
        "old token claims differ"
    );
    ensure!(
        new_provider
            .verify(&token, VerificationOptions::default())
            .is_err(),
        "new key accepted the retired signature"
    );

    // Deliberate unsupported overlap: a prepared new-key server does not revoke
    // the separate still-serving old-key instance. Retire both before cutover.
    let diagnostic_manager = PlaybackManager::new(&new_key, &origin, db.b.clone())?;
    let (diagnostic, same_grant) = prepare(
        &service,
        &diagnostic_manager,
        caller,
        recording,
        Some(grant_id),
    )
    .await?;
    ensure!(
        same_grant == retained_grant,
        "overlap diagnostic changed durable authority"
    );
    wires.diagnostic = Some(RedapWire::new(&diagnostic_manager).await);
    unauthenticated(
        wires.diagnostic.as_mut().unwrap(),
        &old.access.redap_token,
        dataset,
    )
    .await?;
    exact_bytes(
        wires.diagnostic.as_mut().unwrap(),
        &diagnostic.access.redap_token,
        dataset,
        &expected,
    )
    .await?;
    exact_bytes(
        wires.old.as_mut().unwrap(),
        &old.access.redap_token,
        dataset,
        &expected,
    )
    .await?;

    let diagnostic_address = wires.diagnostic.as_mut().unwrap().close().await?;
    wires.diagnostic = None;
    endpoint_closed(diagnostic_address).await?;
    drop(diagnostic_manager);
    let closed = wires.old.as_mut().unwrap().close().await?;
    ensure!(closed == retired_address, "drained another endpoint");
    endpoint_closed(retired_address).await?;
    let retired = unary(wires.old.as_mut().unwrap().client.who_am_i(redap_request(
        proto::WhoAmIRequest {},
        &old.access.redap_token,
    )))
    .await;
    ensure!(
        matches!(retired, Err(ref error) if matches!(error.code(), tonic::Code::Unavailable | tonic::Code::DeadlineExceeded)),
        "retired transport still served authenticated calls"
    );
    wires.old = None;
    drop(old_manager);

    // Supported replacement starts only after all previous serving tasks joined.
    let replacement_manager = PlaybackManager::new(&new_key, &origin, db.b.clone())?;
    let mut current_caller = caller.clone();
    current_caller.jwt_id = JwtId::parse(uuid::Uuid::now_v7().to_string())?;
    current_caller.issued_at = Utc::now();
    current_caller.not_before = current_caller.issued_at;
    current_caller.expires_at = current_caller.issued_at + TimeDelta::minutes(5);
    let (fresh, fresh_grant) = prepare(
        &service,
        &replacement_manager,
        &current_caller,
        recording,
        Some(grant_id),
    )
    .await?;
    ensure!(
        fresh_grant == retained_grant && fresh.access.grant_id == grant_id,
        "replacement changed retained grant identity or scope"
    );
    let stored = RecordingRepository::new(db.a.clone())
        .recording_redap_grant(veoveo_recording_store::RecordingReadGrantId::from_uuid(
            grant_id.as_uuid(),
        ))
        .await?
        .context("retained Store grant disappeared")?;
    ensure!(
        stored == retained_grant,
        "replacement changed durable Store grant fields"
    );
    let before = old.archive.as_ref().context("old archive absent")?;
    let after = fresh
        .archive
        .as_ref()
        .context("replacement archive absent")?;
    ensure!(
        before.uri == after.uri
            && before.dataset_id == after.dataset_id
            && before.recording_segment_id == after.recording_segment_id
            && before.catalog_revision == after.catalog_revision
            && before.byte_len == after.byte_len
            && before.layer_count == after.layer_count,
        "replacement changed admitted archive identity or byte inventory"
    );
    ensure!(
        fresh.access.redap_token != old.access.redap_token,
        "replacement did not issue a new signature"
    );
    wires.replacement = Some(RedapWire::at(&replacement_manager, retired_address).await);
    let wire = wires.replacement.as_mut().unwrap();
    // The cache is prepared with this exact retained grant before testing old auth.
    // A missing catalog would return Unavailable, not this Unauthenticated result.
    unauthenticated(wire, &old.access.redap_token, dataset).await?;
    exact_bytes(wire, &fresh.access.redap_token, dataset, &expected).await?;
    read_only(wire, &fresh.access.redap_token).await?;
    let excluded_parts = wire
        .try_query(&fresh.access.redap_token, dataset, Some(excluded))
        .await
        .map_err(|_| anyhow::anyhow!("replacement scoped query failed"))?;
    let rows: usize = excluded_parts
        .into_iter()
        .map(|part| {
            let batch: re_chunk::external::arrow::array::RecordBatch = part.try_into().unwrap();
            batch.num_rows()
        })
        .sum();
    ensure!(rows == 0, "replacement widened the admitted recording set");
    let retained_bytes = std::fs::read(spool.join("key-retained.rrd"))?;
    ensure!(
        retained_bytes == source_bytes
            && veoveo_types::Sha256Digest::from_bytes(sha2::Sha256::digest(&retained_bytes).into())
                == source_sha,
        "signing-key replacement changed retained source bytes"
    );
    Ok(())
}

async fn exact_bytes(
    wire: &mut RedapWire,
    token: &str,
    dataset: RecordingDatasetId,
    expected: &[LogMsg],
) -> Result<()> {
    let parts = wire
        .try_query(token, dataset, None)
        .await
        .map_err(|_| anyhow::anyhow!("admitted Redap query failed"))?;
    let chunks = wire
        .fetch(token, parts)
        .await
        .map_err(|_| anyhow::anyhow!("admitted Redap fetch failed"))?;
    assert_source_chunks(&chunks, &[expected]);
    Ok(())
}
async fn unauthenticated(
    wire: &mut RedapWire,
    token: &str,
    dataset: RecordingDatasetId,
) -> Result<()> {
    let result = unary(
        wire.client
            .who_am_i(redap_request(proto::WhoAmIRequest {}, token)),
    )
    .await;
    ensure!(
        matches!(result, Err(ref error) if error.code() == tonic::Code::Unauthenticated),
        "retired signature did not fail cryptographic wire authentication"
    );
    let result = wire.try_query(token, dataset, None).await;
    ensure!(
        matches!(result, Err(ref error) if error.code() == tonic::Code::Unauthenticated),
        "retired signature reached query admission"
    );
    Ok(())
}
async fn read_only(wire: &mut RedapWire, token: &str) -> Result<()> {
    let who = unary(
        wire.client
            .who_am_i(redap_request(proto::WhoAmIRequest {}, token)),
    )
    .await
    .map_err(|_| anyhow::anyhow!("read-only identity query failed"))?
    .into_inner();
    ensure!(
        who.can_read
            && !who.can_write
            && who
                .capabilities
                .is_some_and(|capabilities| capabilities.capabilities.is_empty()),
        "playback signing key widened read-only permissions"
    );
    let deleted = unary(
        wire.client
            .delete_entry(redap_request(proto::DeleteEntryRequest::default(), token)),
    )
    .await;
    ensure!(
        matches!(deleted, Err(ref error) if error.code() == tonic::Code::PermissionDenied),
        "playback signing key permitted entry mutation"
    );
    let written = unary(wire.client.write_chunks(redap_request(
        futures::stream::empty::<proto::WriteChunksRequest>(),
        token,
    )))
    .await;
    ensure!(
        matches!(written, Err(ref error) if error.code() == tonic::Code::PermissionDenied),
        "playback signing key permitted chunk writes"
    );
    Ok(())
}
async fn unary<T>(
    call: impl std::future::Future<Output = Result<T, tonic::Status>>,
) -> Result<T, tonic::Status> {
    tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .map_err(|_| tonic::Status::deadline_exceeded("fixture RPC exceeded five seconds"))?
}
async fn endpoint_closed(address: std::net::SocketAddr) -> Result<()> {
    let connected = tokio::time::timeout(
        Duration::from_secs(1),
        tokio::net::TcpStream::connect(address),
    )
    .await;
    ensure!(
        matches!(connected, Ok(Err(_))),
        "retired fixture endpoint still accepts transports or closure was unobservable"
    );
    Ok(())
}
