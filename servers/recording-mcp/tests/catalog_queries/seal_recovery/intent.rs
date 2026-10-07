//! Process replacement uses the persisted request rather than mutable catalog epochs.
use super::*;
use surrealdb::types::SurrealValue;
use veoveo_recording_store::{
    ManifestPublicationBody, ManifestPublicationDescriptor, RecordingBlueprintCommit,
    RecordingBlueprintDraft,
};

pub(super) async fn qualify(
    db: &fixture::TestDb,
    state: &PublisherState,
    http: &HttpFixture,
    caller: &GatewayInternalIdentity,
    dataset: RecordingDatasetId,
    spool: &Path,
    cache: &Path,
) {
    Box::pin(properties_source_barrier(
        db, state, http, caller, dataset, spool, cache,
    ))
    .await;
    for (published, corrupt_reply) in [(false, false), (true, false), (true, true)] {
        Box::pin(qualify_publication_case(
            db,
            state,
            http,
            caller,
            dataset,
            spool,
            cache,
            published,
            corrupt_reply,
        ))
        .await;
    }
}

async fn qualify_publication_case(
    db: &fixture::TestDb,
    state: &PublisherState,
    http: &HttpFixture,
    caller: &GatewayInternalIdentity,
    dataset: RecordingDatasetId,
    spool: &Path,
    cache: &Path,
    published: bool,
    corrupt_reply: bool,
) {
    let repo = RecordingRepository::new(db.a.clone());
    let key = if corrupt_reply {
        "reply-intent"
    } else if published {
        "published-intent"
    } else {
        "prepared-intent"
    };
    let id = recording(state, caller, dataset, spool, key).await;
    state.refuse_manifest.store(!published, Ordering::SeqCst);
    let original_derived = derived_layer(state, &repo, id, "derived-original", true).await;
    state
        .lose_manifest_reply
        .store(published && !corrupt_reply, Ordering::SeqCst);
    state
        .corrupt_manifest_reply
        .store(corrupt_reply, Ordering::SeqCst);
    let service = http.service(db.b.clone(), spool, cache);
    let error = service
        .seal(caller, &artifact_reader(caller), id)
        .await
        .unwrap_err();
    let diagnostic = if corrupt_reply {
        "explicit publication descriptor"
    } else {
        "503"
    };
    assert!(error.to_string().contains(diagnostic), "{error}");
    drop(service);
    state.refuse_manifest.store(false, Ordering::SeqCst);
    state.lose_manifest_reply.store(false, Ordering::SeqCst);
    state.corrupt_manifest_reply.store(false, Ordering::SeqCst);
    let intent = repo
        .manifest_publication(&state.identity, id)
        .await
        .unwrap()
        .unwrap();
    let bytes = intent.bytes().unwrap();
    assert_eq!(
        serde_json::to_value(&intent.body.0).unwrap(),
        serde_json::to_value(
            ManifestPublicationBody::from_value(intent.body.clone().into_value())
                .unwrap()
                .0
        )
        .unwrap()
    );
    let first_attempt = state
        .manifest_attempts
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(first_attempt.0, intent.descriptor.0);
    let descriptor_bytes = serde_json::to_vec(&first_attempt.0).unwrap();
    assert_eq!(
        descriptor_bytes,
        serde_json::to_vec(&intent.descriptor.0).unwrap()
    );
    let decoded = ManifestPublicationDescriptor::from_value(
        ManifestPublicationDescriptor(first_attempt.0.clone()).into_value(),
    )
    .unwrap();
    assert_eq!(descriptor_bytes, serde_json::to_vec(&decoded.0).unwrap());
    let mut unknown_descriptor = intent.descriptor.clone().into_value();
    if let surrealdb::types::Value::Object(fields) = &mut unknown_descriptor {
        let surrealdb::types::Value::Object(artifact) = fields.get_mut("artifact").unwrap() else {
            panic!("typed descriptor artifact");
        };
        let surrealdb::types::Value::Object(metadata) = artifact.get_mut("metadata").unwrap()
        else {
            panic!("typed descriptor metadata");
        };
        metadata.insert("unexpected", surrealdb::types::Value::None);
    }
    assert!(ManifestPublicationDescriptor::from_value(unknown_descriptor).is_err());
    assert_eq!(first_attempt.1, bytes);
    let before = repo
        .recording(state.identity.tenant_id, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.state, RecordingState::Sealing);
    assert!(before.manifest_artifact.is_none());
    if !published {
        let mut reordered = first_attempt.0.clone();
        let fields = reordered.artifact.metadata["provenance"]
            .as_object()
            .unwrap()
            .clone();
        reordered.artifact.metadata["provenance"] =
            serde_json::Value::Object(fields.into_iter().rev().collect());
        let different_order = descriptor_bytes != serde_json::to_vec(&reordered).unwrap();
        let dataset_row = repo
            .recording_dataset(state.identity.tenant_id, dataset)
            .await
            .unwrap()
            .unwrap();
        let layers = repo
            .recording_layers(state.identity.tenant_id, id, 8)
            .await
            .unwrap();
        let attempts = state.requests.lock().unwrap().len();
        if different_order {
            let error = repo
                .reserve_manifest_publication(
                    &state.identity,
                    &before,
                    &dataset_row,
                    &layers,
                    None,
                    intent.body.0.clone(),
                    reordered,
                    intent.authority.clone(),
                    intent.publisher.0.clone(),
                )
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("owner serialization order"),
                "{error}"
            );
        }
        assert_eq!(attempts, state.requests.lock().unwrap().len());
        assert_eq!(
            descriptor_bytes,
            serde_json::to_vec(
                &repo
                    .manifest_publication(&state.identity, id,)
                    .await
                    .unwrap()
                    .unwrap()
                    .descriptor
                    .0
            )
            .unwrap()
        );
    }
    for malformed in [false, true] {
        let mut descriptor = first_attempt.0.clone();
        if malformed {
            descriptor.artifact.metadata["provenance"]["sha256"] = serde_json::json!("invalid");
        } else {
            descriptor.artifact.metadata["provenance"]["recording_id"] =
                serde_json::json!(id.to_string());
        }
        assert!(
            ManifestPublicationDescriptor::from_value(
                ManifestPublicationDescriptor(descriptor).into_value(),
            )
            .is_err()
        );
    }
    let occurrence = veoveo_platform_store::ArtifactId::from_uuid(id.as_uuid());
    assert_eq!(
        db.a.artifact_aggregate(occurrence).await.unwrap().is_some(),
        published
    );
    blueprint_fence(db, state, &repo, &before, &intent).await;
    let attempts = state.requests.lock().unwrap().len();
    let retained = cache.join("manifests").join(format!("{id}.v10.json"));
    assert_eq!(std::fs::read(&retained).unwrap(), bytes);
    let mut unknown_none = intent.body.clone().into_value();
    if let surrealdb::types::Value::Object(fields) = &mut unknown_none {
        fields.insert("unexpected", surrealdb::types::Value::None);
    }
    assert!(ManifestPublicationBody::from_value(unknown_none).is_err());
    // The current native decoder refuses old and mixed bodies on entry.
    for mixed in [false, true] {
        let mut value = serde_json::to_value(&intent.body.0).unwrap();
        if mixed {
            value["recording_segment_id"] = value["recordingSegmentId"].clone();
        } else {
            value["schema"] = serde_json::json!("veoveo.ai/recording-manifest/v9");
        }
        assert!(
            ManifestPublicationBody::from_value(veoveo_platform_store::native_json_into_value(
                value
            ))
            .is_err()
        );
    }
    let properties = repo
        .recording_layer_by_name(state.identity.tenant_id, id, "properties")
        .await
        .unwrap()
        .unwrap();
    let original_digest = properties
        .properties_preparation
        .as_ref()
        .unwrap()
        .body
        .immutable_manifest_digest
        .hex()
        .to_owned();
    for digest in ["f".repeat(64), original_digest] {
        db.a.client()
            .query(include_str!(
                "../../queries/catalog_queries/seal_recovery/properties_snapshot_digest.surql"
            ))
            .bind(("layer", properties.id.clone()))
            .bind(("digest", digest.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        if digest == "f".repeat(64) {
            let error = http
                .service(db.b.clone(), spool, cache)
                .seal(caller, &artifact_reader(caller), id)
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains(
                    "properties preparation differs from selected immutable source facts"
                ),
                "{error}"
            );
            assert_eq!(attempts, state.requests.lock().unwrap().len());
            assert_eq!(
                before,
                repo.recording(state.identity.tenant_id, id)
                    .await
                    .unwrap()
                    .unwrap()
            );
            assert_eq!(
                bytes,
                repo.manifest_publication(&state.identity, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .bytes()
                    .unwrap()
            );
        }
    }
    // A schema-admitted corruption of a queried intent fact cannot cause replay.
    db.a.client()
        .query(include_str!(
            "../../queries/catalog_queries/seal_recovery/intent_digest.surql"
        ))
        .bind(("publication", intent.id.clone()))
        .bind(("digest", "0".repeat(64)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let resumed = http.service(db.b.clone(), spool, cache);
    let error = resumed
        .seal(caller, &artifact_reader(caller), id)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("current intent descriptor digest disagrees"),
        "{error}"
    );
    assert_eq!(attempts, state.requests.lock().unwrap().len());
    assert_eq!(
        before,
        repo.recording(state.identity.tenant_id, id)
            .await
            .unwrap()
            .unwrap()
    );
    assert_eq!(std::fs::read(&retained).unwrap(), bytes);
    db.a.client()
        .query(include_str!(
            "../../queries/catalog_queries/seal_recovery/intent_digest.surql"
        ))
        .bind(("publication", intent.id.clone()))
        .bind(("digest", intent.descriptor_sha256.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    // Mutating the dataset epoch after publication preparation does not rewrite
    // the original request, seal time, or body during process replacement.
    db.a.client()
        .query(include_str!(
            "../../queries/catalog_queries/seal_recovery/dataset_epoch.surql"
        ))
        .bind(("dataset", dataset.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let current_dataset = repo
        .recording_dataset(state.identity.tenant_id, dataset)
        .await
        .unwrap()
        .unwrap();
    assert!(current_dataset.revision > intent.dataset_revision);
    if published {
        // A confirmed occurrence needs fresh permission to read, and an
        // unavailable read never falls through to another publication attempt.
        state.revoke_read.store(true, Ordering::SeqCst);
        let error = resumed
            .seal(caller, &artifact_reader(caller), id)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("denied or unavailable"),
            "{error}"
        );
        state.revoke_read.store(false, Ordering::SeqCst);
        assert_eq!(attempts, state.requests.lock().unwrap().len());
        assert_eq!(
            before,
            repo.recording(state.identity.tenant_id, id)
                .await
                .unwrap()
                .unwrap()
        );
    }
    let mut denied = caller.clone();
    denied.actor.scopes.remove(RecordingScope::Seal.name());
    assert!(
        resumed
            .seal(&denied, &artifact_reader(&denied), id)
            .await
            .is_err()
    );
    assert_eq!(attempts, state.requests.lock().unwrap().len());
    assert_eq!(
        before,
        repo.recording(state.identity.tenant_id, id)
            .await
            .unwrap()
            .unwrap()
    );
    if !published {
        // A divergent installed final is never treated as a recoverable partial.
        std::fs::write(&retained, b"divergent installed manifest").unwrap();
        let count = state.requests.lock().unwrap().len();
        let error = resumed
            .seal(caller, &artifact_reader(caller), id)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("staged Recording manifest differs from durable intent"),
            "{error}"
        );
        assert_eq!(count, state.requests.lock().unwrap().len());
        assert_eq!(
            before,
            repo.recording(state.identity.tenant_id, id)
                .await
                .unwrap()
                .unwrap()
        );
        // Model process replacement after a short derivative write, before promotion.
        std::fs::remove_file(&retained).unwrap();
        let partial = retained
            .parent()
            .unwrap()
            .join(format!("{id}.v10.interrupted.partial"));
        std::fs::write(partial, b"short derivative").unwrap();
    }
    let mut current = caller.clone();
    if published {
        current.authority.policy_revision = PolicyVersion::parse("r2").unwrap();
    }
    resumed
        .seal(&current, &artifact_reader(&current), id)
        .await
        .unwrap();
    assert_eq!(
        repo.recording(state.identity.tenant_id, id)
            .await
            .unwrap()
            .unwrap()
            .state,
        RecordingState::Sealed
    );
    assert!(!retained.exists());
    let final_intent = repo
        .manifest_publication(&state.identity, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_intent.bytes().unwrap(), bytes);
    assert_eq!(final_intent.descriptor.0, first_attempt.0);
    let attempts_now = state.requests.lock().unwrap().len();
    assert_eq!(attempts_now, attempts + usize::from(!published));
    if !published {
        let replay = state
            .manifest_attempts
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .clone();
        assert_eq!(replay, first_attempt);
    }
    assert_eq!(state.objects.lock().unwrap()[&id.to_string()].1, bytes);
    let selected_layer = repo
        .recording_layer(state.identity.tenant_id, original_derived)
        .await
        .unwrap()
        .unwrap();
    let mut response = db.a.client().query(include_str!("../../../../../platform/recordings/store/src/queries/recording_catalog/fail_recording_layer.surql"))
        .bind(("layer", selected_layer.id.clone())).bind(("revision", selected_layer.revision))
        .bind(("recording", id.record_id())).bind(("tenant", state.identity.tenant_id.record_id()))
        .bind(("publication", intent.id.clone())).bind(("reason", "forbidden selected mutation"))
        .await.unwrap();
    let error = veoveo_platform_store::primary_transaction_error(response.take_errors()).unwrap();
    assert!(error.is_thrown(), "{error}");
    assert!(
        error
            .to_string()
            .contains("recording_source_selection_frozen"),
        "{error}"
    );
    assert_eq!(
        selected_layer,
        repo.recording_layer(state.identity.tenant_id, original_derived)
            .await
            .unwrap()
            .unwrap()
    );
    let expected = serde_json::to_value(
        resumed
            .seal(&current, &artifact_reader(&current), id)
            .await
            .unwrap(),
    )
    .unwrap();
    let late = derived_layer(state, &repo, id, "derived-000-before-original", false).await;
    let limited = repo
        .recording_layers(state.identity.tenant_id, id, 2)
        .await
        .unwrap();
    assert!(limited.iter().any(|row| row.id == late.record_id()));
    assert!(final_intent.body.0.layers.len() > limited.len());
    assert_eq!(
        final_intent.body.0.layers.len(),
        repo.manifest_publication_layers(&state.identity, id, &final_intent)
            .await
            .unwrap()
            .len()
    );
    let catalog = checked_catalog_view(&resumed, &current, id).await;
    assert_eq!(catalog.layer_count, final_intent.body.0.layers.len() + 1);
    assert_eq!(
        catalog.committed_layer_count,
        final_intent.body.0.layers.len()
    );
    assert_eq!(
        expected,
        serde_json::to_value(
            resumed
                .seal(&current, &artifact_reader(&current), id)
                .await
                .unwrap()
        )
        .unwrap()
    );
    let failed = derived_layer(state, &repo, id, "derived-zz-failed-after-seal", false).await;
    repo.fail_recording_layer(&state.identity, failed, "fixture derivation failed")
        .await
        .unwrap();
    let catalog = checked_catalog_view(&resumed, &current, id).await;
    assert_eq!(catalog.layer_count, final_intent.body.0.layers.len() + 2);
    assert_eq!(
        catalog.committed_layer_count,
        final_intent.body.0.layers.len()
    );
    assert_eq!(
        expected,
        serde_json::to_value(
            resumed
                .seal(&current, &artifact_reader(&current), id)
                .await
                .unwrap()
        )
        .unwrap()
    );
    finish_derived(state, &repo, id, late).await;
    let catalog = checked_catalog_view(&resumed, &current, id).await;
    assert_eq!(catalog.layer_count, final_intent.body.0.layers.len() + 2);
    assert_eq!(
        catalog.committed_layer_count,
        final_intent.body.0.layers.len() + 1
    );
    assert_eq!(
        expected,
        serde_json::to_value(
            resumed
                .seal(&current, &artifact_reader(&current), id)
                .await
                .unwrap()
        )
        .unwrap()
    );
    for kind in ["capture", "derived"] {
        db.a.client()
            .query(include_str!(
                "../../queries/catalog_queries/seal_recovery/extra_layer_kind.surql"
            ))
            .bind(("layer", failed.record_id()))
            .bind(("kind", kind.to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
        if kind == "capture" {
            assert!(
                !repo
                    .recording_layers(state.identity.tenant_id, id, 2)
                    .await
                    .unwrap()
                    .iter()
                    .any(|row| row.id == failed.record_id())
            );
            let before = repo
                .recording(state.identity.tenant_id, id)
                .await
                .unwrap()
                .unwrap();
            let count = state.requests.lock().unwrap().len();
            let error = resumed
                .seal(&current, &artifact_reader(&current), id)
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("unselected source layer"),
                "{error}"
            );
            assert_eq!(
                before,
                repo.recording(state.identity.tenant_id, id)
                    .await
                    .unwrap()
                    .unwrap()
            );
            assert_eq!(count, state.requests.lock().unwrap().len());
        }
    }
    assert_eq!(
        bytes,
        repo.manifest_publication(&state.identity, id)
            .await
            .unwrap()
            .unwrap()
            .bytes()
            .unwrap()
    );
}

#[derive(Clone, surrealdb::types::SurrealValue)]
struct LayerSelection {
    layer: surrealdb::types::RecordId,
    revision: i64,
}
async fn blueprint_fence(
    db: &fixture::TestDb,
    state: &PublisherState,
    repo: &RecordingRepository,
    recording: &veoveo_recording_store::RecordingRecord,
    intent: &veoveo_recording_store::RecordingManifestPublicationRecord,
) {
    let id = RecordingId::from_uuid(intent.body.0.recording_segment_id.as_uuid());
    let layers = repo
        .recording_layers(state.identity.tenant_id, id, 8)
        .await
        .unwrap();
    let draft = RecordingBlueprintDraft {
        identity: state.identity.clone(),
        recording_id: id,
        stream_id: None,
        work_context: recording.work_context.clone(),
        producer_id: "fixture".into(),
        application_id: recording.application_id.clone(),
        blueprint_id: "fixture-blueprint".into(),
        revision: 1,
        relative_path: "fixture-blueprint.rbl".into(),
        sha256: "1".repeat(64),
        byte_len: 128,
        message_count: 1,
        maximum_revisions: 1,
    };
    let error = repo
        .commit_recording_blueprint(RecordingBlueprintCommit {
            draft,
            created_at: Utc::now(),
        })
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("recording_source_selection_frozen"),
        "{error}"
    );
    assert!(
        repo.current_recording_blueprint(state.identity.tenant_id, id)
            .await
            .unwrap()
            .is_none()
    );
    // A delayed create that passed an earlier lifecycle read still refuses at
    // the actual SQL mutation, before allocating another source member.
    let delayed = layers[0].clone().into_value();
    let mut response = db.a.client().query(include_str!("../../../../../platform/recordings/store/src/queries/recording_catalog/open_recording_layer.surql"))
        .bind(("layer", RecordingLayerId::new().record_id())).bind(("content", delayed))
        .bind(("publication", intent.id.clone())).await.unwrap();
    let error = veoveo_platform_store::primary_transaction_error(response.take_errors()).unwrap();
    assert!(error.is_thrown(), "{error}");
    assert!(
        error
            .to_string()
            .contains("recording_source_selection_frozen"),
        "{error}"
    );
    // Exact committed occurrence retries are admitted without another write.
    for layer in &layers {
        let artifact = veoveo_platform_store::ArtifactId::from_uuid(
            record_uuid(layer.artifact.as_ref().unwrap(), "artifact_occurrence").unwrap(),
        );
        assert_eq!(
            *layer,
            repo.commit_recording_layer(
                &state.identity,
                RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer").unwrap()),
                artifact
            )
            .await
            .unwrap()
        );
    }
    let claimed = veoveo_recording_store::RecordingBlueprintRecord {
        id: veoveo_recording_store::RecordingBlueprintId::new().record_id(),
        tenant: intent.tenant.clone(),
        recording: recording.id.clone(),
        stream: None,
        artifact: None,
        owner: state.identity.principal_id.record_id(),
        work_context: recording.work_context.clone(),
        producer_id: "fixture".into(),
        application_id: recording.application_id.clone(),
        blueprint_id: "stale-blueprint".into(),
        revision: 1,
        relative_path: "fixture.rbl".into(),
        sha256: "1".repeat(64),
        byte_len: 128,
        message_count: 1,
        created_at: Utc::now(),
    };
    for omit_member in [false, true] {
        let mut selected = layers
            .iter()
            .map(|layer| LayerSelection {
                layer: layer.id.clone(),
                revision: layer.revision,
            })
            .collect::<Vec<_>>();
        if omit_member {
            selected.pop().unwrap();
        }
        let mut response = db.a.client().query(include_str!("../../../../../platform/recordings/store/src/queries/recordings/reserve_manifest_publication.surql"))
            .bind(("publication", intent.id.clone())).bind(("content", intent.clone()))
            .bind(("recording", recording.id.clone())).bind(("dataset", intent.dataset.clone()))
            .bind(("tenant", intent.tenant.clone())).bind(("actor", intent.actor.clone()))
            .bind(("revision", recording.revision)).bind(("dataset_revision", intent.dataset_revision))
            .bind(("layers", selected)).bind(("blueprint", Some(claimed.clone())))
            .await.unwrap();
        let error =
            veoveo_platform_store::primary_transaction_error(response.take_errors()).unwrap();
        assert!(error.is_thrown(), "{error}");
        let expected = if omit_member {
            "recording_manifest_publication_layer_set_conflict"
        } else {
            "recording_manifest_publication_blueprint_conflict"
        };
        assert!(error.to_string().contains(expected), "{error}");
    }
    assert_eq!(
        *recording,
        repo.recording(state.identity.tenant_id, id)
            .await
            .unwrap()
            .unwrap()
    );
    assert_eq!(
        layers,
        repo.recording_layers(state.identity.tenant_id, id, 8)
            .await
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
}

// Native lifecycle fixtures carry the actual captured RRD payload descriptor.
async fn derived_layer(
    state: &PublisherState,
    repo: &RecordingRepository,
    recording: RecordingId,
    name: &str,
    committed: bool,
) -> RecordingLayerId {
    let row = repo
        .open_recording_layer(RecordingLayerDraft {
            identity: state.identity.clone(),
            recording_id: recording,
            layer_name: name.into(),
            kind: veoveo_recording_store::RecordingLayerKind::Derived,
            ordinal: None,
            staging_path: None,
            start_time: None,
        })
        .await
        .unwrap();
    let id = RecordingLayerId::from_uuid(record_uuid(&row.id, "recording_layer").unwrap());
    if committed {
        finish_derived(state, repo, recording, id).await;
    }
    id
}
async fn finish_derived(
    state: &PublisherState,
    repo: &RecordingRepository,
    recording: RecordingId,
    layer: RecordingLayerId,
) {
    let source = repo
        .recording_layers(state.identity.tenant_id, recording, 8)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.kind == veoveo_recording_store::RecordingLayerKind::Capture)
        .unwrap();
    let digest = Sha256Digest::from_hex(source.sha256.as_deref().unwrap()).unwrap();
    let schema_digest = source
        .schema_digest
        .as_deref()
        .map(Sha256Digest::from_hex)
        .transpose()
        .unwrap();
    repo.stage_recording_layer(
        &state.identity,
        layer,
        source.byte_len,
        source.message_count,
        &digest,
        source.rrd_version.as_deref(),
        schema_digest.as_ref(),
        source.end_time,
    )
    .await
    .unwrap();
    let parent = repo
        .recording(state.identity.tenant_id, recording)
        .await
        .unwrap()
        .unwrap();
    let artifact = veoveo_artifact_contract::ArtifactId::try_from(layer.as_uuid()).unwrap();
    let request = StreamArtifactRequest {
        artifact_id: artifact,
        artifact: veoveo_artifact_contract::PutArtifactRequest {
            mime_type: Some("application/vnd.rerun.rrd".into()),
            filename: Some("derived.rrd".into()),
            classification: None,
            data_labels: BTreeSet::new(),
            retention_expires_at: None,
            metadata: serde_json::to_value(veoveo_recording_contract::RecordingArtifactMetadata {
                provenance:
                    veoveo_recording_contract::RecordingArtifactProvenance::RecordingLayer {
                        dataset_id: crate_contract_dataset(RecordingDatasetId::from_uuid(
                            record_uuid(&parent.dataset, "recording_dataset").unwrap(),
                        )),
                        recording_id: veoveo_recording_contract::RecordingId::try_from(
                            recording.as_uuid(),
                        )
                        .unwrap(),
                        layer_id: veoveo_recording_contract::RecordingLayerId::try_from(
                            layer.as_uuid(),
                        )
                        .unwrap(),
                        layer_kind: veoveo_recording_contract::RecordingLayerKind::Derived,
                        sha256: Sha256Digest::from_hex(source.sha256.as_deref().unwrap()).unwrap(),
                    },
            })
            .unwrap(),
        },
        expected_byte_len: source.byte_len.try_into().unwrap(),
        expected_sha256: veoveo_artifact_contract::UploadSha256::parse(
            source.sha256.as_deref().unwrap(),
        )
        .unwrap(),
    };
    persist(state, &request).await;
    repo.commit_recording_layer(
        &state.identity,
        layer,
        veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()),
    )
    .await
    .unwrap();
}

async fn properties_source_barrier(
    db: &fixture::TestDb,
    state: &PublisherState,
    http: &HttpFixture,
    caller: &GatewayInternalIdentity,
    dataset: RecordingDatasetId,
    spool: &Path,
    cache: &Path,
) {
    let initial = RecordingRepository::new(db.a.clone());
    let mutator = RecordingRepository::new(db.b.clone());
    for pending in [false, true] {
        let id = recording(
            state,
            caller,
            dataset,
            spool,
            if pending {
                "pending-source-barrier"
            } else {
                "complete-source-barrier"
            },
        )
        .await;
        // A real repository boundary fixes the first read before the other
        // connection completes or opens a Derived row, then begins sealing.
        let old_layers = initial
            .recording_layers(state.identity.tenant_id, id, 8)
            .await
            .unwrap();
        let late = derived_layer(state, &mutator, id, "derived-barrier", !pending).await;
        if pending {
            let before = initial
                .recording(state.identity.tenant_id, id)
                .await
                .unwrap()
                .unwrap();
            let requests = state.requests.lock().unwrap().len();
            let error = initial
                .begin_recording_seal(&state.identity, id, None)
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("non-committed layers"),
                "{error}"
            );
            let service = http.service(db.b.clone(), spool, cache);
            let error = service
                .seal(caller, &artifact_reader(caller), id)
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("manifest layer is not committed"),
                "{error}"
            );
            assert_eq!(
                before,
                initial
                    .recording(state.identity.tenant_id, id)
                    .await
                    .unwrap()
                    .unwrap()
            );
            assert_eq!(requests, state.requests.lock().unwrap().len());
            assert!(
                initial
                    .recording_layer_by_name(state.identity.tenant_id, id, "properties")
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                initial
                    .manifest_publication(&state.identity, id)
                    .await
                    .unwrap()
                    .is_none()
            );
            finish_derived(state, &mutator, id, late).await;
        }
        initial
            .begin_recording_seal(&state.identity, id, None)
            .await
            .unwrap();
        let source = initial
            .recording(state.identity.tenant_id, id)
            .await
            .unwrap()
            .unwrap();
        let dataset_row = initial
            .recording_dataset(state.identity.tenant_id, dataset)
            .await
            .unwrap()
            .unwrap();
        // The epoch/time are fresh; only the original layer snapshot is stale.
        let body = veoveo_recording_contract::RecordingPropertiesBuilder {
            dataset_id: crate_contract_dataset(dataset),
            recording_id: veoveo_recording_contract::RecordingId::try_from(id.as_uuid()).unwrap(),
            dataset_key: dataset_row.dataset_key,
            producer_recording_key: source.recording_key.clone(),
            lifecycle_state: veoveo_recording_contract::RecordingState::Sealed,
            started_at: source.started_at.to_rfc3339(),
            ended_at: source.ended_at.unwrap().to_rfc3339(),
            sealed_at: source.updated_at.to_rfc3339(),
            source_revision: source.revision,
            immutable_manifest_digest: veoveo_recording_store::source_layer_manifest_digest(
                dataset,
                id,
                &old_layers,
            ),
            model_revisions: Default::default(),
            environment_revisions: Default::default(),
        }
        .build()
        .unwrap();
        let requests = state.requests.lock().unwrap().len();
        let error = initial
            .prepare_recording_properties_layer(
                RecordingLayerDraft {
                    identity: state.identity.clone(),
                    recording_id: id,
                    layer_name: "properties".into(),
                    kind: veoveo_recording_store::RecordingLayerKind::Properties,
                    ordinal: None,
                    staging_path: Some(format!("properties/{id}.rrd")),
                    start_time: None,
                },
                body,
                &old_layers,
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("recording_properties_preparation_source_conflict"),
            "{error}"
        );
        assert_eq!(
            source,
            initial
                .recording(state.identity.tenant_id, id)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(
            initial
                .recording_layer_by_name(state.identity.tenant_id, id, "properties")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(requests, state.requests.lock().unwrap().len());
        let service = http.service(db.b.clone(), spool, cache);
        service
            .seal(caller, &artifact_reader(caller), id)
            .await
            .unwrap();
        let all = initial
            .recording_layers(state.identity.tenant_id, id, 8)
            .await
            .unwrap();
        let prepared = all
            .iter()
            .find(|row| row.kind == veoveo_recording_store::RecordingLayerKind::Properties)
            .unwrap();
        assert_eq!(
            prepared
                .properties_preparation
                .as_ref()
                .unwrap()
                .body
                .immutable_manifest_digest,
            veoveo_recording_store::source_layer_manifest_digest(dataset, id, &all)
        );
        let intent = initial
            .manifest_publication(&state.identity, id)
            .await
            .unwrap()
            .unwrap();
        assert!(
            intent
                .body
                .0
                .layers
                .iter()
                .any(|row| row.layer_id.as_uuid() == late.as_uuid())
        );
    }
}

// Exercise the actual full-catalog producer and its checked public decoder.
async fn checked_catalog_view(
    service: &RecordingService,
    caller: &GatewayInternalIdentity,
    id: RecordingId,
) -> veoveo_recording_contract::RecordingView {
    let view = service.recording_view(caller, id).await.unwrap().unwrap();
    let value = serde_json::to_value(&view).unwrap();
    let admitted: veoveo_recording_contract::RecordingView =
        serde_json::from_value(value.clone()).unwrap();
    for (key, invalid) in [
        ("committedLayerCount", serde_json::json!(0)),
        (
            "committedLayerCount",
            serde_json::json!(view.layer_count + 1),
        ),
        ("manifestArtifactUri", serde_json::Value::Null),
        (
            "manifestArtifactUri",
            serde_json::json!(veoveo_artifact_contract::ArtifactId::new().plane_uri()),
        ),
    ] {
        let mut changed = value.clone();
        *changed.get_mut(key).unwrap() = invalid;
        let error = serde_json::from_value::<veoveo_recording_contract::RecordingView>(changed)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Recording catalog view or lifecycle relationships"),
            "{error}"
        );
    }
    admitted
}
