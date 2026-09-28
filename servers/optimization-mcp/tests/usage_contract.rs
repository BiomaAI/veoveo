use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_optimization_mcp::contract::{
    OPTIMIZATION_USAGE_PAGE_SIZE, OptimizationTaskUsageUri, OptimizationUsageCursor,
    OptimizationUsageEntry, OptimizationUsageIndexUri, OptimizationUsagePage,
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
    let usage = OptimizationTaskUsageUri::new(task_id).unwrap();
    assert_eq!(
        usage.as_str(),
        "optimization://usage/task/0195dabe-7777-7abc-8def-000000000007"
    );
    assert_eq!(
        OptimizationTaskUsageUri::parse(usage.as_str()).unwrap(),
        usage
    );
    assert_eq!(
        <OptimizationTaskUsageUri as ResourceAddress>::parse(&usage.to_uri().unwrap()).unwrap(),
        usage
    );
    let entry = OptimizationUsageEntry::new(task_id).unwrap();
    assert_eq!(entry.task_id(), task_id);
    assert_eq!(entry.usage_uri(), &usage);
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({"task_id": task_id, "usage_uri": usage})
    );
    let cursor = OptimizationUsageCursor::new(task_id).unwrap();
    assert_eq!(
        OptimizationUsageCursor::parse(cursor.as_str())
            .unwrap()
            .after(),
        task_id
    );
    for index in [
        OptimizationUsageIndexUri::new(None),
        OptimizationUsageIndexUri::new(Some(&cursor)),
    ] {
        assert_eq!(
            OptimizationUsageIndexUri::parse(index.as_str()).unwrap(),
            index
        );
        assert_eq!(
            serde_json::from_value::<OptimizationUsageIndexUri>(json!(index)).unwrap(),
            index
        );
    }
}

#[test]
fn usage_admission_rejects_invalid_envelopes_and_address_aliases() {
    let id = task(7);
    let cursor = OptimizationUsageCursor::new(id).unwrap();
    for envelope in [
        json!({"version":2,"task_id":id}),
        json!({"version":1,"after":id}),
        json!({"version":1,"task_id":"not-a-task"}),
        json!({"version":1,"task_id":id,"extra":true}),
    ] {
        assert!(
            OptimizationUsageCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope).unwrap())
            )
            .is_err()
        );
    }
    for invalid in ["".to_owned(), "bad".to_owned(), "a".repeat(1025)] {
        assert!(OptimizationUsageCursor::parse(invalid).is_err());
    }
    for invalid in [
        "optimization://usage/".to_owned(),
        "optimization://usage?".to_owned(),
        "optimization://usage#fragment".to_owned(),
        "optimization://usage?limit=20".to_owned(),
        format!(
            "optimization://usage?cursor={0}&cursor={0}",
            cursor.as_str()
        ),
        format!("optimization://usage?%63ursor={}", cursor.as_str()),
        format!("optimization://usage/task/{id}?cursor={}", cursor.as_str()),
    ] {
        assert!(
            OptimizationUsageIndexUri::parse(&invalid).is_err(),
            "{invalid}"
        );
    }
    for id in [
        "550e8400-e29b-41d4-a716-446655440000",
        "00000000-0000-0000-0000-000000000000",
        "0195dabe-7777-7abc-0def-000000000007",
    ] {
        let id: TaskId = id.parse().unwrap();
        assert!(OptimizationTaskUsageUri::new(id).is_err());
        assert!(OptimizationUsageCursor::new(id).is_err());
    }
    let valid = OptimizationTaskUsageUri::new(id).unwrap();
    for invalid in [
        valid.as_str().to_uppercase(),
        format!("{valid}/"),
        format!("{valid}?extra=x"),
        format!("{valid}#fragment"),
        format!("optimization://usage/task/{{{id}}}"),
        format!(
            "optimization://usage/task/{}",
            id.to_string().replace('-', "")
        ),
        valid.as_str().replacen("0195", "%30195", 1),
        format!("optimization://usage/task/{id}/extra"),
        format!("other://usage/task/{id}"),
    ] {
        assert!(
            OptimizationTaskUsageUri::parse(&invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn usage_pages_and_entries_reject_inconsistent_construction_and_decoding() {
    let ids = (1..=OPTIMIZATION_USAGE_PAGE_SIZE as u64)
        .map(task)
        .collect::<Vec<_>>();
    let page = OptimizationUsagePage::from_task_ids(ids.clone(), ids.last().copied()).unwrap();
    assert_eq!(page.usage().len(), OPTIMIZATION_USAGE_PAGE_SIZE);
    let encoded = serde_json::to_value(&page).unwrap();
    assert_eq!(encoded["limit"], 100);
    assert_eq!(
        serde_json::from_value::<OptimizationUsagePage>(encoded.clone()).unwrap(),
        page
    );
    assert!(
        OptimizationUsagePage::from_task_ids(Vec::new(), None)
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
        assert!(OptimizationUsagePage::from_task_ids(ids, next).is_err());
    }
    let mut wrong_entry = encoded["usage"][0].clone();
    wrong_entry["task_id"] = json!(task(9));
    assert!(serde_json::from_value::<OptimizationUsageEntry>(wrong_entry).is_err());
    let mut wrong_page = encoded;
    wrong_page["limit"] = json!(200);
    assert!(serde_json::from_value::<OptimizationUsagePage>(wrong_page).is_err());
}

#[test]
fn usage_preserves_published_cursor_envelope_and_page_fields() {
    let id = task(7);
    // Existing version 1 cursor bytes remain valid, including field order.
    let published = URL_SAFE_NO_PAD.encode(format!(r#"{{"version":1,"task_id":"{id}"}}"#));
    let cursor = OptimizationUsageCursor::new(id).unwrap();
    assert_eq!(cursor.as_str(), published);
    assert_eq!(OptimizationUsageCursor::parse(published).unwrap(), cursor);
    let page = OptimizationUsagePage::from_task_ids(vec![id], None).unwrap();
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "usage": [{"task_id": id, "usage_uri": OptimizationTaskUsageUri::new(id).unwrap()}],
            "limit": 100
        })
    );
    let schema = serde_json::to_value(schemars::schema_for!(OptimizationUsagePage)).unwrap();
    assert_eq!(schema["properties"]["usage"]["maxItems"], 100);
    assert_eq!(schema["properties"]["limit"]["minimum"], 100);
    assert_eq!(schema["properties"]["limit"]["maximum"], 100);
}
