//! Service wiring for current caller authority and optional grant reuse.
use super::{GatewayInternalIdentity, RecordingDatasetId, RecordingId, RecordingService};
use veoveo_platform_store::{RecordingReadGrantClass, deterministic_work_context_id};
use veoveo_recording_mcp::contract::RecordingReadGrantId;
use veoveo_types::{PolicyVersion, WorkContextId};

pub(super) async fn qualify(
    service: &RecordingService,
    caller: &GatewayInternalIdentity,
    dataset: RecordingDatasetId,
    recording: RecordingId,
) {
    #[cfg(feature = "redap")]
    qualify_live_manifest(service, caller, recording).await;
    let class = RecordingReadGrantClass::ViewerSegment;
    let grant = service
        .issue_read_grant(
            caller,
            dataset,
            class,
            vec![recording],
            "catalog-1".into(),
            None,
        )
        .await
        .unwrap();
    let id = RecordingReadGrantId::try_from(
        veoveo_recording_reader::access::record_uuid(&grant.id, "recording_read_grant").unwrap(),
    )
    .unwrap();
    assert_eq!(
        grant,
        service
            .issue_read_grant(
                caller,
                dataset,
                class,
                vec![recording, recording],
                "catalog-1".into(),
                Some(id)
            )
            .await
            .unwrap()
    );
    let mut changed = caller.clone();
    changed.authority.work_context = WorkContextId::new("investigation").unwrap();
    let replacement = service
        .issue_read_grant(
            &changed,
            dataset,
            class,
            vec![recording],
            "catalog-1".into(),
            Some(id),
        )
        .await
        .unwrap();
    assert_ne!(grant.id, replacement.id);
    let platform = service.platform_identity(&changed).await.unwrap();
    assert_eq!(
        replacement.work_context,
        deterministic_work_context_id(&platform.tenant_key, "investigation")
            .unwrap()
            .record_id()
    );
    changed.authority.policy_revision = PolicyVersion::new("r2").unwrap();
    let replacement = service
        .issue_read_grant(
            &changed,
            dataset,
            class,
            vec![recording],
            "catalog-1".into(),
            Some(id),
        )
        .await
        .unwrap();
    assert_ne!(grant.id, replacement.id);
    assert_eq!(replacement.policy_revision, "r2");
    changed.actor.data_labels.clear();
    assert!(
        service
            .issue_read_grant(
                &changed,
                dataset,
                class,
                vec![recording],
                "catalog-1".into(),
                Some(id)
            )
            .await
            .is_err()
    );
}

#[cfg(feature = "redap")]
async fn qualify_live_manifest(
    service: &RecordingService,
    caller: &GatewayInternalIdentity,
    recording: RecordingId,
) {
    use base64::Engine as _;
    use veoveo_recording_mcp::{playback::PlaybackManager, service::PlaybackArchiveSelection};
    let plan = service
        .playback_plan(
            caller,
            None,
            recording,
            PlaybackArchiveSelection::SealedViewer,
        )
        .await
        .unwrap()
        .unwrap();
    let grant = service
        .issue_read_grant(
            caller,
            plan.dataset_id,
            RecordingReadGrantClass::ViewerSegment,
            vec![recording],
            plan.catalog_revision.clone(),
            None,
        )
        .await
        .unwrap();
    let key = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
    let manager =
        PlaybackManager::new(&key, "https://[::1]:8443", service.platform_store().clone()).unwrap();
    let manifest = manager.prepare_manifest(plan, grant).await.unwrap();
    assert_eq!(
        manifest.state,
        veoveo_recording_mcp::contract::RecordingState::Live
    );
    assert_eq!(manifest.recording_segment_id.as_uuid(), recording.as_uuid());
    assert!(manifest.archive.is_none());
    assert!(manifest.live.is_none());
    let wire = serde_json::to_vec(&manifest).unwrap();
    let admitted: veoveo_recording_mcp::contract::PlaybackManifest =
        serde_json::from_slice(&wire).unwrap();
    assert_eq!(admitted.recording_segment_id, manifest.recording_segment_id);
}
