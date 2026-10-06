use veoveo_artifact_contract::ArtifactId;
use veoveo_reason_mcp::contract::*;
use veoveo_types::ResourceAddress;
#[path = "../support/finding.rs"]
pub(super) mod fixture;

fn analysis() -> AnalysisId {
    "01983da0-0000-7000-8000-000000000001".parse().unwrap()
}

#[test]
fn finding_addresses_preserve_collection_and_reject_ambiguous_routes() {
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    for collection in FindingCollection::ALL {
        let cursor = FindingCursor::new(collection, time, analysis());
        let page = FindingResource::Page {
            cursor: cursor.clone(),
        };
        for resource in [
            FindingResource::root(collection),
            page,
            FindingResource::Member {
                collection,
                analysis: analysis(),
            },
        ] {
            let uri = resource.to_uri().unwrap();
            assert_eq!(FindingResource::parse(uri.as_str()).unwrap(), resource);
            assert_eq!(
                serde_json::from_str::<FindingResource>(&serde_json::to_string(&resource).unwrap())
                    .unwrap(),
                resource
            );
            assert_eq!(
                ReasonResource::parse(uri.as_str()).unwrap(),
                ReasonResource::Knowledge(resource)
            );
        }
        let other = if collection == FindingCollection::Analyses {
            "results"
        } else {
            "analyses"
        };
        assert!(
            FindingResource::parse(&format!(
                "reason://knowledge/{other}?cursor={}",
                cursor.encode()
            ))
            .is_err()
        );
    }
    for uri in [
        "reason://knowledge/analyses?",
        "reason://knowledge/analyses?cursor=",
        "reason://knowledge/analyses?cursor=x&cursor=y",
        "reason://knowledge/analyses?unsupported=x",
        "reason://knowledge/analyses/",
        "reason://knowledge/%61nalyses",
        "reason://knowledge/analyses/not-an-id",
        "reason://knowledge/analyses/01983da0-0000-7000-8000-000000000001?cursor=x",
        "reason://knowledge/results#fragment",
        "reason://other/results",
    ] {
        assert!(FindingResource::parse(uri).is_err(), "{uri}");
    }
}

#[test]
fn summaries_bound_unicode_and_preserve_full_result_provenance() {
    let mut results = fixture::results().into_builder();
    results.answer = ReasoningAnswer::Answer {
        text: "交通🚘".repeat(2000),
    };
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    let artifact = ArtifactId::new();
    let summary = FindingSummary::new(
        FindingCollection::Results,
        analysis(),
        artifact,
        time,
        time,
        &FindingData::from_results(&results.clone().build().unwrap()).unwrap(),
    )
    .unwrap();
    let FindingContent::Answer { excerpt } = summary.content() else {
        panic!("answer")
    };
    assert!(excerpt.truncated());
    assert!(excerpt.text().len() <= 4096);
    assert!("交通🚘".repeat(2000).starts_with(excerpt.text()));
    let wire = serde_json::to_value(&summary).unwrap();
    assert_eq!(wire["result_artifact"], artifact.plane_uri().to_string());
    assert_eq!(wire["confidence_basis"], "model_reported");
    assert_eq!(wire["model_digest"], results.model_digest.unwrap());
    assert_eq!(wire["prompt_revision"], "traffic-v1");
    assert!(serde_json::to_vec(&summary).unwrap().len() <= FINDING_SUMMARY_BYTES);
}

#[test]
fn event_summary_declares_omissions_and_checks_source_relationships() {
    let mut results = fixture::results().into_builder();
    results.task = ReasoningTask::DetectEvents {
        prompt: "Vehicles entering".into(),
    };
    results.answer = ReasoningAnswer::Events {
        events: (0..12)
            .map(|index| ReasonedEvent {
                range: IndexRange::new(index, index).unwrap(),
                label: "entry".into(),
                description: "Vehicle enters. ".repeat(100),
                track_ids: vec![7],
            })
            .collect(),
    };
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    let make = |r: &ReasoningResults| {
        FindingSummary::new(
            FindingCollection::Results,
            analysis(),
            ArtifactId::new(),
            time,
            time,
            &FindingData::from_results(r)?,
        )
    };
    let summary = make(&results.clone().build().unwrap()).unwrap();
    let FindingContent::Events {
        events,
        total,
        truncated,
    } = summary.content()
    else {
        panic!("events")
    };
    assert_eq!((*total, events.len(), *truncated), (12, 8, true));
    assert!(events[0].description.truncated());
    results.recording_uri = "recording://recordings/01983da0-0000-7000-8000-000000000010"
        .parse()
        .unwrap();
    assert!(results.build().is_err());
    let mut results = fixture::results().into_builder();
    results.answer = ReasoningAnswer::Description {
        text: "Unexpected answer kind".into(),
    };
    assert!(results.build().is_err());
}

#[test]
fn decoded_summaries_reject_conflicting_identity_collection_and_bounds() {
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    let summary = FindingSummary::new(
        FindingCollection::Results,
        analysis(),
        ArtifactId::new(),
        time,
        time,
        &FindingData::from_results(&fixture::results()).unwrap(),
    )
    .unwrap();
    let original = serde_json::to_value(&summary).unwrap();
    assert!(serde_json::from_value::<FindingSummary>(original.clone()).is_ok());
    for (pointer, replacement) in [
        (
            "/analysis_id",
            serde_json::json!("01983da0-0000-7000-8000-000000000099"),
        ),
        (
            "/uri",
            serde_json::json!(FindingResource::root(FindingCollection::Results)),
        ),
        (
            "/uri",
            serde_json::json!(FindingResource::Member {
                collection: FindingCollection::Analyses,
                analysis: analysis()
            }),
        ),
        ("/modified_at", serde_json::json!("2020-01-01T00:00:00Z")),
        ("/content/excerpt/text", serde_json::json!("x".repeat(4097))),
        ("/requested_range/end", serde_json::json!(-1)),
    ] {
        let mut wire = original.clone();
        *wire.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<FindingSummary>(wire).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn retained_findings_reject_invalid_content_and_unbounded_provenance() {
    let results = fixture::results();
    let data = FindingData::from_results(&results).unwrap();
    let original = serde_json::to_value(&data).unwrap();
    assert_eq!(
        serde_json::from_value::<FindingData>(original.clone()).unwrap(),
        data
    );
    assert!(serde_json::to_vec(&data).unwrap().len() <= FINDING_DATA_BYTES);
    for (pointer, replacement) in [
        ("/answer/kind", serde_json::json!("description")),
        ("/answer/excerpt/text", serde_json::json!("x".repeat(4097))),
        ("/requested_range/end", serde_json::json!(-1)),
        (
            "/prompt_revision",
            serde_json::json!("x".repeat(FINDING_DATA_BYTES)),
        ),
    ] {
        let mut wire = original.clone();
        *wire.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<FindingData>(wire).is_err(),
            "{pointer}"
        );
    }
    let mut unsupported = results.into_builder();
    unsupported.schema = "unsupported".into();
    assert!(unsupported.build().is_err());
}

#[test]
fn full_reason_results_admit_each_answer_and_reject_detached_source_and_task() {
    let results = fixture::results();
    let original = serde_json::to_value(&results).unwrap();
    for (pointer, value) in [
        ("/schema", serde_json::json!("unsupported")),
        (
            "/recording_uri",
            serde_json::json!("recording://recordings/01983da0-0000-7000-8000-000000000001"),
        ),
        ("/entity_path", serde_json::json!("relative")),
        ("/task/kind", serde_json::json!("detect_events")),
        ("/answer/text", serde_json::json!(" ")),
        (
            "/decode",
            serde_json::json!({"mode":"sampled","temperature":0.0,"top_p":0.5,"seed":1}),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<ReasoningResults>(bad).is_err(),
            "{pointer}"
        );
    }
    for (task, answer) in [
        (
            ReasoningTask::DescribeSegment { prompt: None },
            ReasoningAnswer::Description {
                text: "A vehicle turns.".into(),
            },
        ),
        (
            ReasoningTask::DetectEvents {
                prompt: "Find turns".into(),
            },
            ReasoningAnswer::Events { events: Vec::new() },
        ),
    ] {
        let mut candidate = results.clone().into_builder();
        candidate.task = task;
        candidate.answer = answer;
        let admitted = candidate.build().unwrap();
        assert!(
            serde_json::from_value::<ReasoningResults>(serde_json::to_value(admitted).unwrap())
                .is_ok()
        );
    }
    let mut events = results.into_builder();
    events.task = ReasoningTask::DetectEvents {
        prompt: "Find turns".into(),
    };
    events.answer = ReasoningAnswer::Events {
        events: vec![ReasonedEvent {
            range: IndexRange::new(-1, 1).unwrap(),
            label: "turn".into(),
            description: "Vehicle turns.".into(),
            track_ids: Vec::new(),
        }],
    };
    assert!(events.build().is_err());
}

#[test]
fn full_result_construction_and_json_bytes_reject_zero_observation_success() {
    let valid = fixture::results();
    let mut draft = valid.clone().into_builder();
    draft.observed_frames = 0;
    assert!(draft.build().is_err());
    let mut wire = serde_json::to_value(&valid).unwrap();
    wire["observed_frames"] = serde_json::json!(0);
    assert!(
        serde_json::from_slice::<ReasoningResults>(&serde_json::to_vec(&wire).unwrap()).is_err()
    );
    let bytes = serde_json::to_vec(&valid).unwrap();
    let admitted: ReasoningResults = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&admitted).unwrap(), bytes);
}
