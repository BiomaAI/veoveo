//! Structural fixtures do not qualify an inference runtime.
#[path = "../../../../../testing/fixtures/embedding.rs"]
mod embedding_fixture;
use veoveo_embedding_contract::*;
use veoveo_types::Sha256Digest;

fn space() -> EmbeddingSpace {
    EmbeddingSpace {
        model: "synthetic".parse().unwrap(),
        revision: "checkpoint-revision".parse().unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        pooling: EmbeddingPooling::LastToken,
        normalization: EmbeddingNormalization::L2,
        precision: EmbeddingPrecision::Float32,
        max_input_tokens: EmbeddingMaxInputTokens::new(8192).unwrap(),
    }
}

#[test]
fn execution_changes_preserve_space_and_change_immutable_profile_identity() {
    let a = embedding_fixture::profile(space(), 1);
    let b = embedding_fixture::profile(space(), 2);
    assert_eq!(a.space().revision(), b.space().revision());
    assert_ne!(a.id(), b.id());
    let mut contents = a.contents().clone();
    contents.checkpoint_manifest = Sha256Digest::from_bytes([9; 32]);
    let other = EmbeddingExecutionProfile::new(contents).unwrap();
    assert_eq!(other.space(), a.space());
    assert_ne!(other.id(), a.id());
    for change in [
        "model",
        "revision",
        "dimension",
        "pooling",
        "precision",
        "maxInputTokens",
    ] {
        let mut wire = serde_json::to_value(space()).unwrap();
        wire[change] = match change {
            "model" => "other-model".into(),
            "revision" => "other-revision".into(),
            "dimension" => 4.into(),
            "pooling" => "mean".into(),
            "precision" => "float16".into(),
            _ => 4096.into(),
        };
        let changed: EmbeddingSpace = serde_json::from_value(wire).unwrap();
        assert_ne!(changed.revision(), space().revision());
    }
}

#[test]
fn all_profile_and_qualification_decoders_check_contents_and_reject_forged_identity() {
    let runtime = embedding_fixture::runtime(space());
    let mut profile = serde_json::to_value(runtime.profile()).unwrap();
    profile["contents"]["runtimeImage"] =
        serde_json::to_value(Sha256Digest::from_bytes([8; 32])).unwrap();
    assert!(serde_json::from_value::<EmbeddingExecutionProfile>(profile).is_err());
    for field in [
        "retrievalPassed",
        "interactivePriorityPassed",
        "capacityPassed",
    ] {
        let mut failed = serde_json::to_value(&runtime.qualifications()[0]).unwrap();
        failed["contents"]["evidence"][field] = false.into();
        assert!(serde_json::from_value::<EmbeddingQualification>(failed).is_err());
    }
    let mut qualification = serde_json::to_value(&runtime.qualifications()[0]).unwrap();
    qualification["contents"]["evidence"]["retrievalPassed"] = false.into();
    assert!(serde_json::from_value::<EmbeddingQualification>(qualification).is_err());
    let mut bundle = serde_json::to_value(&runtime).unwrap();
    bundle["qualifications"] = serde_json::json!([]);
    assert!(serde_json::from_value::<QualifiedEmbeddingRuntime>(bundle).is_err());
    let mut wrong = runtime.profile().contents().clone();
    wrong.serving.precision = EmbeddingPrecision::Float16;
    assert!(EmbeddingExecutionProfile::new(wrong).is_err());
    assert!(EmbeddingGpuName::new("software renderer".into()).is_err());
}

#[test]
fn compatibility_is_directional_and_unknown_producers_fail() {
    let a = embedding_fixture::profile(space(), 1);
    let b = embedding_fixture::profile(space(), 2);
    let selected = QualifiedEmbeddingRuntime::new(
        b.clone(),
        vec![a.clone(), b.clone()],
        vec![
            embedding_fixture::qualification(&b, &b),
            embedding_fixture::qualification(&b, &a),
        ],
    )
    .unwrap();
    assert_eq!(
        selected
            .qualification_for(a.id())
            .unwrap()
            .producer_profile(),
        a.id()
    );
    let unknown = embedding_fixture::profile(space(), 3);
    assert!(selected.producer(unknown.id()).is_err());
    assert!(selected.qualification_for(unknown.id()).is_err());
    let reverse = QualifiedEmbeddingRuntime::new(
        a.clone(),
        vec![a.clone(), b.clone()],
        vec![embedding_fixture::qualification(&a, &a)],
    )
    .unwrap();
    assert!(reverse.qualification_for(b.id()).is_err());
    let mut incompatible = space();
    incompatible.precision = EmbeddingPrecision::Float16;
    assert!(
        EmbeddingQualification::new(
            &a,
            &embedding_fixture::profile(incompatible, 4),
            selected.qualifications()[0].evidence().clone()
        )
        .is_err()
    );
}

#[test]
fn effective_execution_settings_change_profile_identity_and_stale_contents_reject() {
    let a = embedding_fixture::profile(space(), 1);
    let mut variants = vec![];
    let mut contents = a.contents().clone();
    contents.serving.graph_allowance = EmbeddingGraphAllowance::EnforceEager;
    variants.push(contents);
    let mut contents = a.contents().clone();
    contents.serving.observed_graph_execution = EmbeddingGraphExecution::CudaGraph;
    variants.push(contents);
    let mut contents = a.contents().clone();
    contents.serving.attention_backend =
        EmbeddingAttentionBackend::new("OTHER_MEASURED_BACKEND".into()).unwrap();
    variants.push(contents);
    let mut contents = a.contents().clone();
    contents.serving.explicit_kv_cache_bytes = Some(EmbeddingKvCacheBytes::new(4096).unwrap());
    variants.push(contents);
    let mut contents = a.contents().clone();
    contents.serving.max_num_sequences += 1;
    variants.push(contents);
    for contents in variants {
        let changed = EmbeddingExecutionProfile::new(contents).unwrap();
        assert_eq!(changed.space(), a.space());
        assert_ne!(changed.id(), a.id());
        let mut stale = serde_json::to_value(&changed).unwrap();
        stale["id"] = serde_json::to_value(a.id()).unwrap();
        assert!(serde_json::from_value::<EmbeddingExecutionProfile>(stale).is_err());
    }
    let mut impossible = a.contents().clone();
    impossible.serving.graph_allowance = EmbeddingGraphAllowance::EnforceEager;
    impossible.serving.observed_graph_execution = EmbeddingGraphExecution::CudaGraph;
    assert!(EmbeddingExecutionProfile::new(impossible).is_err());
    let mut missing = serde_json::to_value(&a).unwrap();
    missing["contents"]["serving"]
        .as_object_mut()
        .unwrap()
        .remove("explicitKvCacheBytes");
    assert!(serde_json::from_value::<EmbeddingExecutionProfile>(missing).is_err());
    let mut unsupported = serde_json::to_value(&a).unwrap();
    unsupported["contents"]["serving"]["scheduling"] = "fifo".into();
    assert!(serde_json::from_value::<EmbeddingExecutionProfile>(unsupported).is_err());
}

#[test]
fn actual_profile_schema_requires_explicit_nullable_cache_reservation() {
    let profile = embedding_fixture::profile(space(), 1);
    let schema = serde_json::to_value(schemars::schema_for!(EmbeddingExecutionProfile)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let original = serde_json::to_value(&profile).unwrap();
    for cache in [None, Some(1), Some(EmbeddingKvCacheBytes::MAX)] {
        let mut contents = profile.contents().clone();
        contents.serving.explicit_kv_cache_bytes =
            cache.map(|value| EmbeddingKvCacheBytes::new(value).unwrap());
        let admitted = EmbeddingExecutionProfile::new(contents).unwrap();
        let wire = serde_json::to_value(&admitted).unwrap();
        assert!(validator.is_valid(&wire));
        assert_eq!(
            serde_json::from_value::<EmbeddingExecutionProfile>(wire).unwrap(),
            admitted
        );
    }
    for value in [
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!(u64::MAX),
    ] {
        let mut wire = original.clone();
        wire["contents"]["serving"]["explicitKvCacheBytes"] = value;
        assert!(!validator.is_valid(&wire));
        assert!(serde_json::from_value::<EmbeddingExecutionProfile>(wire).is_err());
    }
    let mut missing = original;
    missing["contents"]["serving"]
        .as_object_mut()
        .unwrap()
        .remove("explicitKvCacheBytes");
    assert!(!validator.is_valid(&missing));
    assert!(serde_json::from_value::<EmbeddingExecutionProfile>(missing).is_err());
}

#[test]
fn space_and_profile_schemas_match_scalar_byte_admission_limits() {
    let space_schema = serde_json::to_value(schemars::schema_for!(EmbeddingSpace)).unwrap();
    let space_validator = jsonschema::validator_for(&space_schema).unwrap();
    let profile_schema =
        serde_json::to_value(schemars::schema_for!(EmbeddingExecutionProfile)).unwrap();
    let profile_validator = jsonschema::validator_for(&profile_schema).unwrap();
    for (dimension, tokens) in [
        (1, 1),
        (EmbeddingDimension::MAX, EmbeddingMaxInputTokens::MAX),
    ] {
        let mut admitted = space();
        admitted.dimension = EmbeddingDimension::new(dimension).unwrap();
        admitted.max_input_tokens = EmbeddingMaxInputTokens::new(tokens).unwrap();
        let space_wire = serde_json::to_value(&admitted).unwrap();
        assert!(space_validator.is_valid(&space_wire));
        assert_eq!(
            serde_json::from_slice::<EmbeddingSpace>(&serde_json::to_vec(&space_wire).unwrap())
                .unwrap(),
            admitted
        );
        let profile = embedding_fixture::profile(admitted, 1);
        let wire = serde_json::to_value(&profile).unwrap();
        assert!(profile_validator.is_valid(&wire));
        assert_eq!(
            serde_json::from_slice::<EmbeddingExecutionProfile>(
                &serde_json::to_vec(&wire).unwrap()
            )
            .unwrap(),
            profile
        );
    }
    for (field, invalid) in [
        ("dimension", serde_json::json!(0)),
        ("dimension", serde_json::json!(8193)),
        ("dimension", serde_json::json!(1.5)),
        ("dimension", serde_json::json!(-1)),
        ("maxInputTokens", serde_json::json!(0)),
        ("maxInputTokens", serde_json::json!(131073)),
        ("maxInputTokens", serde_json::json!(1.5)),
        ("maxInputTokens", serde_json::json!(-1)),
    ] {
        let mut wire = serde_json::to_value(space()).unwrap();
        wire[field] = invalid.clone();
        assert!(!space_validator.is_valid(&wire), "{field}");
        assert!(
            serde_json::from_slice::<EmbeddingSpace>(&serde_json::to_vec(&wire).unwrap()).is_err()
        );
        let mut profile = serde_json::to_value(embedding_fixture::profile(space(), 1)).unwrap();
        profile["contents"]["space"][field] = invalid;
        assert!(!profile_validator.is_valid(&profile), "{field}");
        assert!(
            serde_json::from_slice::<EmbeddingExecutionProfile>(
                &serde_json::to_vec(&profile).unwrap()
            )
            .is_err()
        );
    }
}
