#[path = "../../../../testing/fixtures/embedding.rs"]
mod embedding_fixture;
use veoveo_knowledge_contract::{ChunkSettings, GenerationId};

#[test]
fn generation_and_chunk_settings_reject_unusable_values_on_deserialization() {
    let id = GenerationId::new();
    assert_eq!(
        serde_json::from_str::<GenerationId>(&serde_json::to_string(&id).unwrap()).unwrap(),
        id
    );
    assert!(GenerationId::try_from(uuid::Uuid::nil().to_string()).is_err());
    assert!(GenerationId::try_from("019A5E81-ABCD-7123-8123-123456789ABC".to_owned()).is_err());
    for value in [
        serde_json::json!({"version":"v1", "maxCharacters":100, "overlapCharacters":100}),
        serde_json::json!({"version":"v1", "maxCharacters":0, "overlapCharacters":0}),
        serde_json::json!({"version":"v1", "maxCharacters":100, "overlapCharacters":1, "extra":true}),
    ] {
        assert!(serde_json::from_value::<ChunkSettings>(value).is_err());
    }
}

#[test]
fn metadata_constructor_rejects_body_chunks_and_mismatched_mode() {
    use veoveo_embedding_contract::*;
    use veoveo_knowledge_contract::*;
    use veoveo_mcp_knowledge_extension::*;
    use veoveo_types::{ResourceTemplateUri, ResourceUri, Sha256Digest};
    let registration = CollectionRegistration {
        source_contract_revision: 3,
        tenant: "fixture".parse().unwrap(),
        descriptor: CollectionDescriptor::new(
            "fixture.records".parse().unwrap(),
            "record".parse().unwrap(),
            ResourceTemplateUri::new("fixture://records{?cursor}").unwrap(),
            Freshness::immutable(),
            ChangeSignal::Immutable,
            AccessModel::Profile,
            IndexingMode::Metadata,
        )
        .unwrap(),
        approval: KnowledgeCollectionApproval {
            collection: "fixture.records".parse().unwrap(),
            mode: CollectionApproval::Index,
            stewards: ["stewards".parse().unwrap()].into(),
            authoritative_for: Default::default(),
            data_labels: ["secret".parse().unwrap(), "restricted".parse().unwrap()].into(),
        },
        control_revision: Sha256Digest::from_bytes([1; 32]),
    };
    let current = serde_json::to_vec(&registration).unwrap();
    let admitted: CollectionRegistration = serde_json::from_slice(&current).unwrap();
    admitted.validate().unwrap();
    assert_eq!(admitted, registration);
    assert_eq!(admitted.revision(), registration.revision());
    let mut unrelated_control = registration.clone();
    unrelated_control.control_revision = Sha256Digest::from_bytes([9; 32]);
    assert_eq!(unrelated_control.revision(), registration.revision());
    let mut changed_approval = registration.clone();
    changed_approval.approval.mode = CollectionApproval::CatalogOnly;
    changed_approval.validate().unwrap();
    assert_ne!(changed_approval.revision(), registration.revision());
    for (canonical, retired) in [
        ("authoritativeFor", "authoritative_for"),
        ("dataLabels", "data_labels"),
    ] {
        for mixed in [false, true] {
            let mut bad = serde_json::to_value(&registration).unwrap();
            let approval = bad["approval"].as_object_mut().unwrap();
            let value = if mixed {
                approval[canonical].clone()
            } else {
                approval.remove(canonical).unwrap()
            };
            approval.insert(retired.into(), value);
            assert!(serde_json::from_value::<CollectionRegistration>(bad.clone()).is_err());
            assert!(
                serde_json::from_slice::<CollectionRegistration>(
                    &serde_json::to_vec(&bad).unwrap()
                )
                .is_err()
            );
        }
    }
    let space = EmbeddingSpace {
        model: EmbeddingModelId::parse("fixture").unwrap(),
        revision: EmbeddingModelRevision::parse("v1").unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        pooling: EmbeddingPooling::LastToken,
        normalization: EmbeddingNormalization::L2,
        precision: EmbeddingPrecision::Float32,
        max_input_tokens: EmbeddingMaxInputTokens::new(8192).unwrap(),
    };
    let spec = GenerationSpec::new(
        space.clone(),
        "Find passages",
        ChunkSettings::new("structure-v1", 1000, 0).unwrap(),
        [(
            registration.descriptor.collection().clone(),
            registration.revision(),
        )]
        .into(),
    )
    .unwrap();
    let text = "private body must not enter metadata";
    let observed = "2026-10-01T00:00:00Z".parse().unwrap();
    let observation = Observation::builder(
        registration.descriptor.collection().clone(),
        "1".parse().unwrap(),
        content_digest(text),
        observed,
    )
    .build(&registration.descriptor)
    .unwrap();
    let title = MemberTitle::new("Inspection").unwrap();
    let vector = || {
        EmbeddingVector::new(
            &embedding_fixture::runtime(space.clone()),
            vec![1.0, 0.0, 0.0],
        )
        .unwrap()
    };
    let body_chunk = IndexedChunk::from_range(
        text,
        0..text.len(),
        vector(),
        &spec,
        &embedding_fixture::runtime(spec.space().clone()),
    )
    .unwrap();
    let uri = ResourceUri::new("fixture://records/one").unwrap();
    assert!(
        IndexedMember::metadata(
            &registration,
            &spec,
            uri.clone(),
            observation.clone(),
            text,
            title.clone(),
            vec![body_chunk.clone()]
        )
        .is_err()
    );
    assert!(
        IndexedMember::new(
            &registration,
            &spec,
            uri.clone(),
            observation.clone(),
            text,
            title.clone(),
            vec![body_chunk]
        )
        .is_err()
    );
    let metadata = metadata_text(&title, &observation);
    let chunk = IndexedChunk::from_range(
        &metadata,
        0..metadata.len(),
        vector(),
        &spec,
        &embedding_fixture::runtime(spec.space().clone()),
    )
    .unwrap();
    assert!(
        IndexedMember::metadata(
            &registration,
            &spec,
            uri,
            observation,
            text,
            title,
            vec![chunk]
        )
        .is_ok()
    );
}

#[test]
fn generation_digest_excludes_execution_but_includes_query_chunking_and_approvals() {
    use veoveo_embedding_contract::*;
    use veoveo_knowledge_contract::*;
    let space = EmbeddingSpace {
        model: "synthetic".parse().unwrap(),
        revision: "fixture".parse().unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        pooling: EmbeddingPooling::LastToken,
        normalization: EmbeddingNormalization::L2,
        precision: EmbeddingPrecision::Float32,
        max_input_tokens: EmbeddingMaxInputTokens::new(8192).unwrap(),
    };
    let a = embedding_fixture::profile(space.clone(), 1);
    let b = embedding_fixture::profile(space, 2);
    let collections = [(
        "fixture.records".parse().unwrap(),
        veoveo_types::Sha256Digest::from_bytes([1; 32]),
    )]
    .into();
    let chunking = ChunkSettings::new("structure-v1", 1000, 100).unwrap();
    let first = GenerationSpec::new(
        a.space().clone(),
        "Find passages",
        chunking.clone(),
        collections,
    )
    .unwrap();
    let replaced = GenerationSpec::new(
        b.space().clone(),
        first.query_task(),
        chunking.clone(),
        first.collections().clone(),
    )
    .unwrap();
    assert_ne!(a.id(), b.id());
    assert_eq!(first.revision(), replaced.revision());
    let query_changed = GenerationSpec::new(
        a.space().clone(),
        "Find other passages",
        chunking.clone(),
        first.collections().clone(),
    )
    .unwrap();
    assert_ne!(query_changed.revision(), first.revision());
    let chunk_changed = GenerationSpec::new(
        a.space().clone(),
        first.query_task(),
        ChunkSettings::new("structure-v2", 1000, 100).unwrap(),
        first.collections().clone(),
    )
    .unwrap();
    assert_ne!(chunk_changed.revision(), first.revision());
    let collection_changed = GenerationSpec::new(
        a.space().clone(),
        first.query_task(),
        chunking,
        [(
            "fixture.records".parse().unwrap(),
            veoveo_types::Sha256Digest::from_bytes([2; 32]),
        )]
        .into(),
    )
    .unwrap();
    assert_ne!(collection_changed.revision(), first.revision());
}

#[test]
fn approvals_refuse_retired_and_mixed_names_and_vocabulary() {
    use veoveo_knowledge_contract::{CollectionApproval, KnowledgeCollectionApproval};
    let current = serde_json::json!({
        "collection":"fixture.records", "mode":"catalog_only", "stewards":["stewards"],
        "authoritativeFor":["facilities"], "dataLabels":["restricted"]
    });
    let approval: KnowledgeCollectionApproval = serde_json::from_value(current.clone()).unwrap();
    approval.validate().unwrap();
    assert_eq!(approval.mode, CollectionApproval::CatalogOnly);
    assert_eq!(serde_json::to_value(&approval).unwrap(), current);
    for (canonical, retired) in [
        ("authoritativeFor", "authoritative_for"),
        ("dataLabels", "data_labels"),
    ] {
        for mixed in [false, true] {
            let mut bad = current.clone();
            let fields = bad.as_object_mut().unwrap();
            let value = fields[canonical].clone();
            if !mixed {
                fields.remove(canonical);
            }
            fields.insert(retired.into(), value);
            assert!(serde_json::from_value::<KnowledgeCollectionApproval>(bad.clone()).is_err());
            assert!(
                serde_json::from_slice::<KnowledgeCollectionApproval>(
                    &serde_json::to_vec(&bad).unwrap()
                )
                .is_err()
            );
        }
    }
    let mut retired_mode = current.clone();
    retired_mode["mode"] = "catalog-only".into();
    assert!(serde_json::from_value::<KnowledgeCollectionApproval>(retired_mode).is_err());
    for name in ["catalog_only", "index"] {
        assert!(serde_json::from_value::<CollectionApproval>(name.into()).is_ok());
    }
    assert!(serde_json::from_value::<CollectionApproval>("catalog-only".into()).is_err());
}
