use veoveo_artifact_contract::ArtifactId;
use veoveo_reason_mcp::contract::*;
use veoveo_types::ResourceAddress;
#[path = "../support/finding.rs"]
mod fixture;

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
    let mut results = fixture::results();
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
        &results,
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
    let mut results = fixture::results();
    results.task = ReasoningTask::DetectEvents {
        prompt: "Vehicles entering".into(),
    };
    results.answer = ReasoningAnswer::Events {
        events: (0..12)
            .map(|index| ReasonedEvent {
                range: IndexRange {
                    start: index,
                    end: index,
                },
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
            r,
        )
    };
    let summary = make(&results).unwrap();
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
    assert!(make(&results).is_err());
    results = fixture::results();
    results.answer = ReasoningAnswer::Description {
        text: "Unexpected answer kind".into(),
    };
    assert!(make(&results).is_err());
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
        &fixture::results(),
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
