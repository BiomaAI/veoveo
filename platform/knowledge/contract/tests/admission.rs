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
    let space = EmbeddingSpace {
        model: EmbeddingModelId::new("fixture").unwrap(),
        revision: EmbeddingModelRevision::new("v1").unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        runtime_image: Sha256Digest::from_bytes([2; 32]),
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
    let vector = || EmbeddingVector::new(space.clone(), vec![1.0, 0.0, 0.0]).unwrap();
    let body_chunk = IndexedChunk::from_range(text, 0..text.len(), vector(), &spec).unwrap();
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
    let chunk = IndexedChunk::from_range(&metadata, 0..metadata.len(), vector(), &spec).unwrap();
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
