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
