//! Actual owner queries overlap on two SDK transactions; neither order may skew.
use super::*;
use surrealdb::types::SurrealValue;
use veoveo_recording_store::RecordingBlueprintRecord;

#[derive(Clone, SurrealValue)]
struct LayerSelection {
    layer: surrealdb::types::RecordId,
    revision: i64,
}
fn transaction_body(sql: &str) -> &str {
    sql.strip_prefix("BEGIN TRANSACTION;\n")
        .unwrap()
        .strip_suffix("COMMIT TRANSACTION;\n")
        .unwrap()
}
pub(super) async fn qualify(
    db: &fixture::TestDb,
    state: &PublisherState,
    http: &HttpFixture,
    caller: &GatewayInternalIdentity,
    dataset: RecordingDatasetId,
    spool: &Path,
    cache: &Path,
) {
    let repo = RecordingRepository::new(db.a.clone());
    for intent_first in [false, true] {
        let id = recording(
            state,
            caller,
            dataset,
            spool,
            if intent_first {
                "race-intent-first"
            } else {
                "race-source-first"
            },
        )
        .await;
        state.refuse_manifest.store(true, Ordering::SeqCst);
        let service = http.service(db.b.clone(), spool, cache);
        let error = service
            .seal(caller, &artifact_reader(caller), id)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("503"), "{error}");
        state.refuse_manifest.store(false, Ordering::SeqCst);
        let intent = repo
            .manifest_publication(&state.identity, id)
            .await
            .unwrap()
            .unwrap();
        let before = repo
            .recording(state.identity.tenant_id, id)
            .await
            .unwrap()
            .unwrap();
        let layers = repo
            .recording_layers(state.identity.tenant_id, id, 8)
            .await
            .unwrap();
        // Fixture administration restores the pre-reservation point using the
        // actual producer's admitted immutable candidate. No provider is invoked.
        db.a.client()
            .query(include_str!(
                "../../queries/catalog_queries/seal_recovery/remove_intent.surql"
            ))
            .bind(("publication", intent.id.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let blueprint = RecordingBlueprintRecord {
            id: veoveo_recording_store::RecordingBlueprintId::new().record_id(),
            tenant: intent.tenant.clone(),
            recording: before.id.clone(),
            stream: None,
            artifact: None,
            owner: state.identity.principal_id.record_id(),
            work_context: before.work_context.clone(),
            producer_id: "race-fixture".into(),
            application_id: before.application_id.clone(),
            blueprint_id: "race-blueprint".into(),
            revision: 1,
            relative_path: "race.rbl".into(),
            sha256: "1".repeat(64),
            byte_len: 128,
            message_count: 1,
            created_at: Utc::now(),
        };
        let mut content = blueprint.clone().into_value();
        if let surrealdb::types::Value::Object(fields) = &mut content {
            fields.remove("id");
        }
        let source_tx = db.a.client().clone().begin().await.unwrap();
        let intent_tx = db.b.client().clone().begin().await.unwrap();
        let requests = state.requests.lock().unwrap().len();
        source_tx.query(transaction_body(include_str!("../../../../../platform/recordings/store/src/queries/recording_blueprints/commit_recording_blueprint.surql")))
            .bind(("blueprint", blueprint.id.clone())).bind(("content", content)).bind(("publication", intent.id.clone()))
            .await.unwrap().check().unwrap();
        intent_tx.query(transaction_body(include_str!("../../../../../platform/recordings/store/src/queries/recordings/reserve_manifest_publication.surql")))
            .bind(("publication", intent.id.clone())).bind(("content", intent.clone()))
            .bind(("recording", before.id.clone())).bind(("dataset", intent.dataset.clone()))
            .bind(("tenant", intent.tenant.clone())).bind(("actor", intent.actor.clone()))
            .bind(("revision", before.revision)).bind(("dataset_revision", intent.dataset_revision))
            .bind(("layers", layers.iter().map(|row| LayerSelection { layer: row.id.clone(), revision: row.revision }).collect::<Vec<_>>()))
            .bind(("blueprint", Option::<RecordingBlueprintRecord>::None)).await.unwrap().check().unwrap();
        if intent_first {
            intent_tx.commit().await.unwrap();
            let error = source_tx.commit().await.unwrap_err();
            assert!(
                error.to_string().to_ascii_lowercase().contains("conflict"),
                "{error}"
            );
            assert!(
                repo.current_recording_blueprint(state.identity.tenant_id, id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                before,
                repo.recording(state.identity.tenant_id, id)
                    .await
                    .unwrap()
                    .unwrap()
            );
            assert_eq!(
                intent.bytes().unwrap(),
                repo.manifest_publication(&state.identity, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .bytes()
                    .unwrap()
            );
        } else {
            source_tx.commit().await.unwrap();
            let error = intent_tx.commit().await.unwrap_err();
            assert!(
                error.to_string().to_ascii_lowercase().contains("conflict"),
                "{error}"
            );
            assert!(
                repo.manifest_publication(&state.identity, id)
                    .await
                    .unwrap()
                    .is_none()
            );
            let current = repo
                .recording(state.identity.tenant_id, id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(current.state, RecordingState::Sealing);
            assert_eq!(current.revision, before.revision + 1);
            assert_eq!(
                Some(blueprint),
                repo.current_recording_blueprint(state.identity.tenant_id, id)
                    .await
                    .unwrap()
            );
        }
        assert_eq!(requests, state.requests.lock().unwrap().len());
        assert_eq!(
            layers,
            repo.recording_layers(state.identity.tenant_id, id, 8)
                .await
                .unwrap()
        );
    }
}
