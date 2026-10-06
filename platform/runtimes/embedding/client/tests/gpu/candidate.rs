//! First hardware qualification before a production bundle exists.
use super::*;
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
use veoveo_embedding_client::verification::*;
use veoveo_embedding_contract::EmbeddingExecutionProfile;
use veoveo_types::Sha256Digest;

fn read_env<T: serde::de::DeserializeOwned>(key: &str) -> T {
    serde_json::from_slice(
        &std::fs::read(std::env::var(key).expect(key)).expect("read measurement input"),
    )
    .expect("admitted measurement input")
}
fn write_new(key: &str, value: &impl serde::Serialize) -> Vec<u8> {
    let path = PathBuf::from(std::env::var(key).expect(key));
    assert!(path.is_absolute(), "measurement output must be absolute");
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("new measurement output");
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    bytes
}
async fn client() -> CandidateEmbeddingClient {
    let _ = rustls::crypto::ring::default_provider().install_default();
    CandidateEmbeddingClient::connect(CandidateClientConfig::new(
        EmbeddingEndpoint::parse(
            &std::env::var("VEOVEO_EMBEDDING_URL").expect("set measured candidate endpoint"),
        )
        .unwrap(),
        secrecy::SecretString::from(
            std::fs::read_to_string(std::env::var("VEOVEO_EMBEDDING_API_KEY_FILE").unwrap())
                .unwrap(),
        ),
        read_env("VEOVEO_EMBEDDING_PROFILE_FILE"),
    ))
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires measured NVIDIA candidate endpoint and explicit scheduling/capacity acceptance thresholds"]
async fn measure_candidate_priority_and_capacity_on_cuda() {
    timeout(Duration::from_secs(120), async {
        let client = client().await;
        let minimum: f64 = std::env::var("VEOVEO_EMBEDDING_MINIMUM_INPUTS_PER_SECOND")
            .expect("set capacity acceptance threshold")
            .parse()
            .unwrap();
        let maximum: u64 = std::env::var("VEOVEO_EMBEDDING_MAXIMUM_QUERY_LATENCY_MICROS")
            .expect("set latency acceptance threshold")
            .parse()
            .unwrap();
        assert!(minimum.is_finite() && minimum > 0.0 && maximum > 0);
        let text = EmbeddingText::new(
            "Inspect the flood boundary, shelter capacity and bridge access. ".repeat(60),
        )
        .unwrap();
        let batch = EmbeddingBatch::new(vec![text; 32]).unwrap();
        let began = Instant::now();
        let mut pending = tokio::task::JoinSet::new();
        for _ in 0..6 {
            let client = client.clone();
            let batch = batch.clone();
            pending.spawn(async move {
                client
                    .embed_documents(&batch, EmbeddingPriority::Bulk)
                    .await
                    .unwrap();
                u64::try_from(began.elapsed().as_micros()).unwrap()
            });
        }
        // Arrival offset only; no readiness polling or production fallback.
        tokio::time::sleep(Duration::from_millis(25)).await;
        let query_began = Instant::now();
        client
            .embed_query(
                &EmbeddingTask::new("Retrieve relevant operational records.").unwrap(),
                &EmbeddingText::new("Which roads need inspection after flooding?").unwrap(),
            )
            .await
            .unwrap();
        let query_latency = u64::try_from(query_began.elapsed().as_micros()).unwrap();
        let query_completed = u64::try_from(began.elapsed().as_micros()).unwrap();
        let mut bulk_completed = Vec::new();
        while let Some(result) = pending.join_next().await {
            bulk_completed.push(result.unwrap());
        }
        let elapsed = u64::try_from(began.elapsed().as_micros()).unwrap();
        let priority_passed = bulk_completed
            .iter()
            .filter(|time| **time > query_completed)
            .count()
            >= 4;
        let scheduling = CandidateSchedulingReport {
            format: "veoveo.ai/embedding-candidate-scheduling/v1".into(),
            profile_id: client.profile().id().clone(),
            space: client.profile().space().clone(),
            query_completed_micros: query_completed,
            bulk_completed_micros: bulk_completed,
            passed: priority_passed,
        };
        let scheduling_bytes = write_new("VEOVEO_EMBEDDING_SCHEDULING_REPORT", &scheduling);
        let capacity_passed =
            192.0 * 1_000_000.0 / elapsed as f64 >= minimum && query_latency <= maximum;
        write_new(
            "VEOVEO_EMBEDDING_CAPACITY_REPORT",
            &CandidateCapacityReport {
                format: "veoveo.ai/embedding-candidate-capacity/v1".into(),
                profile_id: client.profile().id().clone(),
                space: client.profile().space().clone(),
                scheduling_report: Sha256Digest::from_bytes(
                    Sha256::digest(scheduling_bytes).into(),
                ),
                bulk_inputs: 192,
                elapsed_micros: elapsed,
                query_latency_micros: query_latency,
                minimum_inputs_per_second: minimum,
                maximum_query_latency_micros: maximum,
                passed: capacity_passed,
            },
        );
        assert!(
            priority_passed && capacity_passed,
            "candidate priority/capacity failed; reports retained"
        );
    })
    .await
    .expect("candidate scheduling/capacity exceeded 120 seconds");
}
#[test]
#[ignore = "forms first bundle from actual passing CUDA reference/retrieval/scheduling/capacity reports"]
fn write_initial_runtime_from_hardware_reports() {
    let profile: EmbeddingExecutionProfile = read_env("VEOVEO_EMBEDDING_PROFILE_FILE");
    let read = |key: &str| {
        std::fs::read(std::env::var(key).expect(key)).expect("read actual hardware report")
    };
    let runtime = initial_runtime_from_reports(
        profile,
        &read("VEOVEO_EMBEDDING_REFERENCE_REPORT"),
        &read("VEOVEO_EMBEDDING_RETRIEVAL_REPORT"),
        &read("VEOVEO_EMBEDDING_SCHEDULING_REPORT"),
        &read("VEOVEO_EMBEDDING_CAPACITY_REPORT"),
    )
    .expect("all four matching reports must pass");
    write_new("VEOVEO_EMBEDDING_RUNTIME_OUTPUT", &runtime);
    println!(
        "{}",
        serde_json::json!({"result":"pass","profileId":runtime.profile().id(),"qualificationId":runtime.qualifications()[0].id(),"scope":"candidate_self_compatibility","productionKnowledgeAcceptance":"pending"})
    );
}

#[test]
#[ignore = "encodes operator-measured effective NVIDIA profile contents; does not attest or qualify them"]
fn write_measured_execution_profile() {
    let contents: veoveo_embedding_contract::EmbeddingExecutionContents =
        read_env("VEOVEO_EMBEDDING_PROFILE_CONTENTS_FILE");
    let profile =
        EmbeddingExecutionProfile::new(contents).expect("admitted actual effective serving facts");
    write_new("VEOVEO_EMBEDDING_PROFILE_OUTPUT", &profile);
    println!(
        "{}",
        serde_json::json!({"profileId":profile.id(),"qualification":"pending_hardware_reports"})
    );
}
