use chrono::{TimeZone, Utc};
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_contract::*;
use veoveo_types::{ResourceUriBuilder, Sha256Digest, UriSegment};

fn member(n: usize) -> EvaluationMember {
    EvaluationMember {
        member: EvaluationMemberId {
            collection: "fixture.records".parse().unwrap(),
            uri: ResourceUriBuilder::new("fixture://members")
                .unwrap()
                .segment(UriSegment::new(n.to_string()).unwrap())
                .build()
                .unwrap(),
        },
        revision: veoveo_mcp_knowledge_extension::Revision::new("r1").unwrap(),
        content_sha256: Sha256Digest::from_bytes([1; 32]),
    }
}
fn case(id: &str, relevant: &[usize]) -> RetrievalCase {
    RetrievalCase::new(
        EvaluationCaseId::new(id).unwrap(),
        EmbeddingText::new("Find the inspection result").unwrap(),
        Default::default(),
        relevant.iter().map(|n| member(*n).member).collect(),
    )
    .unwrap()
}
fn dataset() -> RetrievalDataset {
    RetrievalDataset::new(
        (0..20).map(member).collect(),
        vec![case("one", &[0, 1]), case("two", &[2, 3, 4, 5])],
    )
    .unwrap()
}
fn report() -> RetrievalEvaluation {
    let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
    RetrievalEvaluation::builder(
        GenerationId::new(),
        Sha256Digest::from_bytes([2; 32]),
        Sha256Digest::from_bytes([3; 32]),
        dataset(),
        now,
    )
    .measure(RetrievalMeasurement {
        case: EvaluationCaseId::new("one").unwrap(),
        ranked: vec![member(0).member, member(9).member],
        elapsed_micros: 10,
    })
    .measure(RetrievalMeasurement {
        case: EvaluationCaseId::new("two").unwrap(),
        ranked: vec![member(2).member],
        elapsed_micros: 20,
    })
    .finish(now)
    .unwrap()
}

#[test]
fn recall_counts_members_once_and_weights_queries_equally() {
    let report = report();
    assert_eq!(report.recall_at_ten(), 0.375); // mean of 1/2 and 1/4; not pooled 2/6.
    let decoded: RetrievalEvaluation =
        serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
    assert_eq!(decoded.revision(), report.revision());
    assert_eq!(decoded.recall_at_ten(), report.recall_at_ten());
    let mut empty = serde_json::to_value(&report).unwrap();
    empty["measurements"][0]["ranked"] = serde_json::json!([]);
    empty["measurements"][1]["ranked"] = serde_json::json!([]);
    assert_eq!(
        serde_json::from_value::<RetrievalEvaluation>(empty)
            .unwrap()
            .recall_at_ten(),
        0.0
    );
}

#[test]
fn decoding_rejects_incomplete_duplicate_and_unjudged_results() {
    let base = serde_json::to_value(report()).unwrap();
    for edit in 0..8 {
        let mut value = base.clone();
        match edit {
            0 => {
                value["measurements"].as_array_mut().unwrap().pop();
            }
            1 => value["measurements"][1]["case"] = value["measurements"][0]["case"].clone(),
            2 => {
                let rank = value["measurements"][0]["ranked"][0].clone();
                value["measurements"][0]["ranked"]
                    .as_array_mut()
                    .unwrap()
                    .push(rank);
            }
            3 => {
                value["measurements"][0]["ranked"][0] =
                    serde_json::to_value(member(100).member).unwrap()
            }
            4 => value["measurements"][0]["elapsedMicros"] = 0.into(),
            5 => value["measurements"][0]["elapsedMicros"] = 60_000_001u64.into(),
            6 => value["completedAt"] = "2026-09-01T00:00:00Z".into(),
            7 => {
                value["measurements"][0]["ranked"] =
                    serde_json::to_value((0..11).map(|n| member(n).member).collect::<Vec<_>>())
                        .unwrap()
            }
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<RetrievalEvaluation>(value).is_err(),
            "invalid measurement {edit} admitted"
        );
    }
}

#[test]
fn corpus_identity_revision_and_judgments_bind_the_dataset() {
    let original = dataset();
    let mut value = serde_json::to_value(&original).unwrap();
    value["corpus"].as_array_mut().unwrap().reverse();
    value["cases"].as_array_mut().unwrap().reverse();
    assert_eq!(
        serde_json::from_value::<RetrievalDataset>(value.clone())
            .unwrap()
            .revision(),
        original.revision()
    );
    value["corpus"][0]["revision"] = "changed".into();
    assert_ne!(
        serde_json::from_value::<RetrievalDataset>(value)
            .unwrap()
            .revision(),
        original.revision()
    );
    assert!(RetrievalDataset::new(vec![member(0), member(0)], vec![case("one", &[0])]).is_err());
    assert!(RetrievalDataset::new(vec![member(0)], vec![case("one", &[1])]).is_err());
    assert!(
        RetrievalDataset::new(vec![member(0)], vec![case("one", &[0]), case("one", &[0])]).is_err()
    );
    assert!(
        RetrievalCase::new(
            EvaluationCaseId::new("one").unwrap(),
            EmbeddingText::new("query").unwrap(),
            ["fixture.other".parse().unwrap()].into(),
            [member(0).member].into()
        )
        .is_err()
    );
}
