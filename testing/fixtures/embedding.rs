//! Synthetic execution facts for structural/native tests. These never qualify a deployment.
use veoveo_embedding_contract::*;
use veoveo_types::Sha256Digest;

pub fn profile(space: EmbeddingSpace, image: u8) -> EmbeddingExecutionProfile {
    let serving = EmbeddingServingConfiguration {
        precision: space.precision,
        scheduling: EmbeddingScheduling::Priority,
        graph_allowance: EmbeddingGraphAllowance::GraphsAllowed,
        observed_graph_execution: EmbeddingGraphExecution::Eager,
        attention_backend: EmbeddingAttentionBackend::new("SYNTHETIC_FIXTURE".into()).unwrap(),
        explicit_kv_cache_bytes: None,
        max_input_tokens: space.max_input_tokens,
        max_num_batched_tokens: 8192,
        max_num_sequences: 16,
        gpu_memory_basis_points: 9000,
    };
    EmbeddingExecutionProfile::new(EmbeddingExecutionContents {
        space,
        runtime_image: Sha256Digest::from_bytes([image; 32]),
        checkpoint_manifest: Sha256Digest::from_bytes([2; 32]),
        vllm_version: EmbeddingRuntimeVersion::new("synthetic-fixture".into()).unwrap(),
        environment: EmbeddingNvidiaEnvironment {
            gpu: EmbeddingGpuName::new("NVIDIA synthetic fixture".into()).unwrap(),
            driver: EmbeddingRuntimeVersion::new("synthetic-driver".into()).unwrap(),
            cuda: EmbeddingRuntimeVersion::new("synthetic-cuda".into()).unwrap(),
        },
        serving,
    })
    .unwrap()
}

pub fn qualification(
    query: &EmbeddingExecutionProfile,
    producer: &EmbeddingExecutionProfile,
) -> EmbeddingQualification {
    EmbeddingQualification::new(
        query,
        producer,
        EmbeddingQualificationEvidence {
            reference_report: Sha256Digest::from_bytes([3; 32]),
            retrieval_report: Sha256Digest::from_bytes([4; 32]),
            scheduling_report: Sha256Digest::from_bytes([5; 32]),
            capacity_report: Sha256Digest::from_bytes([6; 32]),
            minimum_cosine_millionths: 1_000_000,
            retrieval_passed: true,
            interactive_priority_passed: true,
            capacity_passed: true,
        },
    )
    .unwrap()
}

pub fn runtime(space: EmbeddingSpace) -> QualifiedEmbeddingRuntime {
    let profile = profile(space, 1);
    let qualification = qualification(&profile, &profile);
    QualifiedEmbeddingRuntime::new(profile.clone(), vec![profile], vec![qualification]).unwrap()
}
