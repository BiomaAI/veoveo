use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_timeseries_mcp::contract::{
    TIMESERIES_USAGE_PAGE_SIZE, TimeseriesTaskUsageUri, TimeseriesUsageCursor,
    TimeseriesUsageEntry, TimeseriesUsageIndexUri, TimeseriesUsagePage,
};
use veoveo_types::{ResourceAddress, TaskId};

fn task(number: u64) -> TaskId {
    format!("0195dabe-7777-7abc-8def-{number:012x}")
        .parse()
        .unwrap()
}

#[test]
fn usage_builders_preserve_native_task_identity_and_wire_shape() {
    let task_id = task(7);
    let usage = TimeseriesTaskUsageUri::new(task_id).unwrap();
    assert_eq!(
        usage.as_str(),
        "timeseries://usage/task/0195dabe-7777-7abc-8def-000000000007"
    );
    assert_eq!(
        TimeseriesTaskUsageUri::parse(usage.as_str()).unwrap(),
        usage
    );
    assert_eq!(
        <TimeseriesTaskUsageUri as ResourceAddress>::parse(&usage.to_uri().unwrap()).unwrap(),
        usage
    );
    let entry = TimeseriesUsageEntry::new(task_id).unwrap();
    assert_eq!(entry.task_id(), task_id);
    assert_eq!(entry.usage_uri(), &usage);
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({"taskId": task_id, "usageUri": usage})
    );
    let cursor = TimeseriesUsageCursor::new(task_id).unwrap();
    assert_eq!(
        TimeseriesUsageCursor::parse(cursor.as_str())
            .unwrap()
            .after(),
        task_id
    );
    for index in [
        TimeseriesUsageIndexUri::new(None),
        TimeseriesUsageIndexUri::new(Some(&cursor)),
    ] {
        assert_eq!(
            TimeseriesUsageIndexUri::parse(index.as_str()).unwrap(),
            index
        );
        assert_eq!(
            serde_json::from_value::<TimeseriesUsageIndexUri>(json!(index)).unwrap(),
            index
        );
    }
}

#[test]
fn usage_admission_rejects_invalid_envelopes_and_address_aliases() {
    let id = task(7);
    let cursor = TimeseriesUsageCursor::new(id).unwrap();
    for envelope in [
        json!({"version":2,"taskId":id}),
        json!({"version":2,"after":id}),
        json!({"version":2,"taskId":"not-a-task"}),
        json!({"version":2,"taskId":id,"extra":true}),
    ] {
        assert!(
            TimeseriesUsageCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope).unwrap())
            )
            .is_err()
        );
    }
    for invalid in ["".to_owned(), "bad".to_owned(), "a".repeat(1025)] {
        assert!(TimeseriesUsageCursor::parse(invalid).is_err());
    }
    for invalid in [
        "timeseries://usage/".to_owned(),
        "timeseries://usage?".to_owned(),
        "timeseries://usage#fragment".to_owned(),
        "timeseries://usage?limit=20".to_owned(),
        format!("timeseries://usage?cursor={0}&cursor={0}", cursor.as_str()),
        format!("timeseries://usage?%63ursor={}", cursor.as_str()),
        format!("timeseries://usage/task/{id}?cursor={}", cursor.as_str()),
    ] {
        assert!(
            TimeseriesUsageIndexUri::parse(&invalid).is_err(),
            "{invalid}"
        );
    }
    for id in [
        "550e8400-e29b-41d4-a716-446655440000",
        "00000000-0000-0000-0000-000000000000",
        "0195dabe-7777-7abc-0def-000000000007",
    ] {
        let id: TaskId = id.parse().unwrap();
        assert!(TimeseriesTaskUsageUri::new(id).is_err());
        assert!(TimeseriesUsageCursor::new(id).is_err());
    }
    let valid = TimeseriesTaskUsageUri::new(id).unwrap();
    for invalid in [
        valid.as_str().to_uppercase(),
        format!("{valid}/"),
        format!("{valid}?extra=x"),
        format!("{valid}#fragment"),
        format!("timeseries://usage/task/{{{id}}}"),
        format!(
            "timeseries://usage/task/{}",
            id.to_string().replace('-', "")
        ),
        valid.as_str().replacen("0195", "%30195", 1),
        format!("timeseries://usage/task/{id}/extra"),
        format!("other://usage/task/{id}"),
    ] {
        assert!(
            TimeseriesTaskUsageUri::parse(&invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn usage_pages_and_entries_reject_inconsistent_construction_and_decoding() {
    let ids = (1..=TIMESERIES_USAGE_PAGE_SIZE as u64)
        .map(task)
        .collect::<Vec<_>>();
    let page = TimeseriesUsagePage::from_task_ids(ids.clone(), ids.last().copied()).unwrap();
    assert_eq!(page.usage().len(), TIMESERIES_USAGE_PAGE_SIZE);
    let encoded = serde_json::to_value(&page).unwrap();
    assert_eq!(encoded["limit"], 100);
    assert_eq!(
        serde_json::from_value::<TimeseriesUsagePage>(encoded.clone()).unwrap(),
        page
    );
    assert!(
        TimeseriesUsagePage::from_task_ids(Vec::new(), None)
            .unwrap()
            .next_cursor()
            .is_none()
    );
    for (ids, next) in [
        (vec![task(1)], Some(task(1))),
        (vec![task(1), task(1)], None),
        (vec![task(2), task(1)], None),
        (ids.clone(), Some(task(101))),
        ((1..=101).map(task).collect(), None),
    ] {
        assert!(TimeseriesUsagePage::from_task_ids(ids, next).is_err());
    }
    let mut wrong_entry = encoded["usage"][0].clone();
    wrong_entry["taskId"] = json!(task(9));
    assert!(serde_json::from_value::<TimeseriesUsageEntry>(wrong_entry).is_err());
    let mut wrong_page = encoded;
    wrong_page["limit"] = json!(200);
    assert!(serde_json::from_value::<TimeseriesUsagePage>(wrong_page).is_err());
}

#[test]
fn usage_preserves_published_cursor_envelope_and_page_fields() {
    let id = task(7);
    // Existing version 1 cursor bytes remain valid, including field order.
    let published = URL_SAFE_NO_PAD.encode(format!(r#"{{"version":2,"taskId":"{id}"}}"#));
    let cursor = TimeseriesUsageCursor::new(id).unwrap();
    assert_eq!(cursor.as_str(), published);
    assert_eq!(TimeseriesUsageCursor::parse(published).unwrap(), cursor);
    let page = TimeseriesUsagePage::from_task_ids(vec![id], None).unwrap();
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "usage": [{"taskId": id, "usageUri": TimeseriesTaskUsageUri::new(id).unwrap()}],
            "limit": 100
        })
    );
    let schema = serde_json::to_value(schemars::schema_for!(TimeseriesUsagePage)).unwrap();
    assert_eq!(schema["properties"]["usage"]["maxItems"], 100);
    assert_eq!(schema["properties"]["limit"]["minimum"], 100);
    assert_eq!(schema["properties"]["limit"]["maximum"], 100);
}

#[test]
fn hosted_resources_admit_only_their_typed_families() {
    use veoveo_artifact_contract::ArtifactId;
    use veoveo_timeseries_mcp::contract::{
        TimeseriesArtifactUri, TimeseriesDocument, TimeseriesResource,
    };
    let id = ArtifactId::new();
    let artifact = TimeseriesArtifactUri::new(id);
    assert_eq!(artifact.artifact_id(), id);
    assert_eq!(
        TimeseriesArtifactUri::parse(artifact.as_str()).unwrap(),
        artifact
    );
    let task = task(7);
    let cursor = TimeseriesUsageCursor::new(task).unwrap();
    for address in [
        TimeseriesResource::Docs,
        TimeseriesResource::Document(TimeseriesDocument::Agents),
        TimeseriesResource::Document(TimeseriesDocument::Design),
        TimeseriesResource::Contract,
        TimeseriesResource::ForecastApp,
        TimeseriesResource::Usage(TimeseriesUsageIndexUri::new(None)),
        TimeseriesResource::Usage(TimeseriesUsageIndexUri::new(Some(&cursor))),
        TimeseriesResource::TaskUsage(TimeseriesTaskUsageUri::new(task).unwrap()),
        TimeseriesResource::Artifact(artifact.clone()),
    ] {
        let uri = address.to_uri().unwrap();
        assert_eq!(TimeseriesResource::parse(uri.as_str()).unwrap(), address);
        assert_eq!(
            serde_json::from_value::<TimeseriesResource>(json!(uri)).unwrap(),
            address
        );
        for invalid in [
            format!("{uri}#fragment"),
            format!("{uri}/extra"),
            uri.as_str().replace("timeseries", "other"),
        ] {
            assert!(TimeseriesResource::parse(&invalid).is_err(), "{invalid}");
        }
    }
    for invalid in [
        "timeseries://docs/unknown",
        "timeseries://docs/%61gents",
        "timeseries://contract?extra=x",
        "timeseries://artifact/00000000-0000-0000-0000-000000000000",
        "ui://timeseries/forecast.html?token=secret",
    ] {
        assert!(TimeseriesResource::parse(invalid).is_err(), "{invalid}");
    }
    for invalid in [
        format!("{artifact}?query=x"),
        format!("{artifact}#fragment"),
        format!("{artifact}/extra"),
        format!("other://artifact/{id}"),
        format!("artifact://{id}"),
        format!("timeseries://artifact/{{{id}}}"),
        format!("timeseries://artifact/{}", id.to_string().replace('-', "")),
    ] {
        assert!(TimeseriesArtifactUri::parse(&invalid).is_err(), "{invalid}");
        assert!(serde_json::from_value::<TimeseriesArtifactUri>(json!(invalid)).is_err());
    }
}

#[test]
fn current_usage_cursor_refuses_previous_revision_and_retired_member() {
    let id = TaskId::new();
    let current = json!({"version":2,"taskId":id});
    let encode =
        |value: &serde_json::Value| URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).unwrap());
    assert_eq!(
        TimeseriesUsageCursor::parse(encode(&current))
            .unwrap()
            .after(),
        id
    );
    for bad in [
        json!({"version":1,"taskId":id}),
        json!({"version":2,"task_id":id}),
        json!({"version":2,"taskId":id,"task_id":id}),
    ] {
        assert!(TimeseriesUsageCursor::parse(encode(&bad)).is_err());
    }
}
