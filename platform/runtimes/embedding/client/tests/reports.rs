//! Synthetic report admission only; these records never qualify a deployment.
#![cfg(feature = "verification")]
#[path = "../../../../../testing/fixtures/embedding.rs"]
mod embedding_fixture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use veoveo_embedding_client::verification::initial_runtime_from_reports;
use veoveo_embedding_contract::*;
use veoveo_types::Sha256Digest;

fn fixture() -> (EmbeddingExecutionProfile, [Value; 4]) {
    let space = EmbeddingSpace {
        model: "fixture-model".parse().unwrap(),
        revision: "fixture-revision".parse().unwrap(),
        dimension: EmbeddingDimension::new(3).unwrap(),
        pooling: EmbeddingPooling::LastToken,
        normalization: EmbeddingNormalization::L2,
        precision: EmbeddingPrecision::Bfloat16,
        max_input_tokens: EmbeddingMaxInputTokens::new(32768).unwrap(),
    };
    let profile = embedding_fixture::runtime(space.clone()).profile().clone();
    let digest = Sha256Digest::from_bytes([8; 32]);
    let measured = json!({"profileId": profile.id(), "space":space,"captureDigest":digest,"datasetRevision":digest,"sourceCorpusRevision":digest,"queryTaskRevision":digest,"chunkingRevision":digest,"checkpointManifest":profile.contents().checkpoint_manifest});
    let reference = json!({"format":"veoveo.ai/embedding-candidate-reference/v1","measurement":measured,"minimumCosineMillionths":999500,"passed":true,"vectors":4,"cudaDevice":profile.contents().environment.gpu,"referencePrecision":"bfloat16","attentionBackend":"cudnn_attention"});
    let retrieval = json!({"format":"veoveo.ai/embedding-candidate-retrieval/v1","measurement":measured,"candidateRecallAtTen":1.0,"referenceRecallAtTen":1.0,"minimumRecallAtTen":0.9,"maximumReferenceRecallLoss":0.0,"passed":true,"cases":[{"id":"case-one","candidateHits":1,"referenceHits":1,"relevant":1}],"scope":"candidate_vector_ranking"});
    let scheduling = json!({"format":"veoveo.ai/embedding-candidate-scheduling/v1","profileId":profile.id(),"space":space,"queryCompletedMicros":100,"bulkCompletedMicros":[90,95,110,120,130,140],"passed":true});
    let capacity = json!({"format":"veoveo.ai/embedding-candidate-capacity/v1","profileId":profile.id(),"space":space,"schedulingReport":Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&scheduling).unwrap()).into()),"bulkInputs":192,"elapsedMicros":200,"queryLatencyMicros":50,"minimumInputsPerSecond":1.0,"maximumQueryLatencyMicros":100,"passed":true});
    (profile, [reference, retrieval, scheduling, capacity])
}
fn admit(
    profile: EmbeddingExecutionProfile,
    reports: [Value; 4],
) -> Result<QualifiedEmbeddingRuntime, veoveo_embedding_client::EmbeddingClientError> {
    let bytes: Vec<_> = reports
        .iter()
        .map(|report| serde_json::to_vec(report).unwrap())
        .collect();
    initial_runtime_from_reports(profile, &bytes[0], &bytes[1], &bytes[2], &bytes[3])
}
#[test]
fn matching_reports_bind_actual_bytes_and_do_not_change_space() {
    let (profile, reports) = fixture();
    let expected =
        Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&reports[0]).unwrap()).into());
    let runtime = admit(profile.clone(), reports).unwrap();
    assert_eq!(runtime.profile(), &profile);
    assert_eq!(
        runtime.qualifications()[0].evidence().reference_report,
        expected
    );
}
#[test]
fn report_mismatch_failed_claim_and_unmeasured_capacity_are_rejected() {
    let (profile, reports) = fixture();
    for (index, pointer, value) in [
        (0, "/passed", json!(false)),
        (0, "/minimumCosineMillionths", json!(998999)),
        (0, "/cudaDevice", json!("NVIDIA other GPU")),
        (
            1,
            "/measurement/captureDigest",
            json!(Sha256Digest::from_bytes([9; 32])),
        ),
        (1, "/candidateRecallAtTen", json!(0.5)),
        (1, "/cases/0/candidateHits", json!(0)),
        (1, "/scope", json!("production_sql")),
        (2, "/bulkCompletedMicros", json!([1, 2, 3, 4, 5, 6])),
        (3, "/elapsedMicros", json!(0)),
        (3, "/minimumInputsPerSecond", json!(1e20)),
        (3, "/maximumQueryLatencyMicros", json!(1)),
        (
            3,
            "/schedulingReport",
            json!(Sha256Digest::from_bytes([9; 32])),
        ),
    ] {
        let mut changed = reports.clone();
        *changed[index].pointer_mut(pointer).unwrap() = value;
        assert!(admit(profile.clone(), changed).is_err(), "{pointer}");
    }
    let mut unknown = reports;
    unknown[0]["unmeasuredFact"] = json!(true);
    assert!(admit(profile, unknown).is_err());
}
