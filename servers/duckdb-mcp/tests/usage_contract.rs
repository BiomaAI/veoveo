use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_duckdb_mcp::contract::{
    DUCKDB_USAGE_PAGE_SIZE, DuckDbTaskUsageUri, DuckDbUsageCursor, DuckDbUsageEntry,
    DuckDbUsageIndexUri, DuckDbUsagePage,
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
    let usage = DuckDbTaskUsageUri::new(task_id).unwrap();
    assert_eq!(
        usage.as_str(),
        "duckdb://usage/task/0195dabe-7777-7abc-8def-000000000007"
    );
    assert_eq!(DuckDbTaskUsageUri::parse(usage.as_str()).unwrap(), usage);
    assert_eq!(
        <DuckDbTaskUsageUri as ResourceAddress>::parse(&usage.to_uri().unwrap()).unwrap(),
        usage
    );
    let entry = DuckDbUsageEntry::new(task_id).unwrap();
    assert_eq!(entry.task_id(), task_id);
    assert_eq!(entry.usage_uri(), &usage);
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({"task_id": task_id, "usage_uri": usage})
    );
    let cursor = DuckDbUsageCursor::new(task_id).unwrap();
    assert_eq!(
        DuckDbUsageCursor::parse(cursor.as_str()).unwrap().after(),
        task_id
    );
    for index in [
        DuckDbUsageIndexUri::new(None),
        DuckDbUsageIndexUri::new(Some(&cursor)),
    ] {
        assert_eq!(DuckDbUsageIndexUri::parse(index.as_str()).unwrap(), index);
        assert_eq!(
            serde_json::from_value::<DuckDbUsageIndexUri>(json!(index)).unwrap(),
            index
        );
    }
}

#[test]
fn usage_admission_rejects_invalid_envelopes_and_address_aliases() {
    let id = task(7);
    let cursor = DuckDbUsageCursor::new(id).unwrap();
    for envelope in [
        json!({"version":2,"collection":DuckDbUsageIndexUri::ROOT,"after":id}),
        json!({"version":1,"collection":"timeseries://usage","after":id}),
        json!({"version":1,"collection":DuckDbUsageIndexUri::ROOT,"after":"not-a-task"}),
        json!({"version":1,"collection":DuckDbUsageIndexUri::ROOT,"after":id,"extra":true}),
        json!({"version":1,"task_id":id}),
    ] {
        assert!(
            DuckDbUsageCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope).unwrap())
            )
            .is_err()
        );
    }
    for invalid in ["".to_owned(), "bad".to_owned(), "a".repeat(1025)] {
        assert!(DuckDbUsageCursor::parse(invalid).is_err());
    }
    for invalid in [
        "duckdb://usage/".to_owned(),
        "duckdb://usage?".to_owned(),
        "duckdb://usage#fragment".to_owned(),
        "duckdb://usage?limit=20".to_owned(),
        format!("duckdb://usage?cursor={0}&cursor={0}", cursor.as_str()),
        format!("duckdb://usage?%63ursor={}", cursor.as_str()),
        format!("duckdb://usage/task/{id}?cursor={}", cursor.as_str()),
    ] {
        assert!(DuckDbUsageIndexUri::parse(&invalid).is_err(), "{invalid}");
    }
    for id in [
        "550e8400-e29b-41d4-a716-446655440000",
        "00000000-0000-0000-0000-000000000000",
        "0195dabe-7777-7abc-0def-000000000007",
    ] {
        let id: TaskId = id.parse().unwrap();
        assert!(DuckDbTaskUsageUri::new(id).is_err());
        assert!(DuckDbUsageCursor::new(id).is_err());
    }
    let valid = DuckDbTaskUsageUri::new(id).unwrap();
    for invalid in [
        valid.as_str().to_uppercase(),
        format!("{valid}/"),
        format!("{valid}?extra=x"),
        format!("{valid}#fragment"),
        format!("duckdb://usage/task/{{{id}}}"),
        format!("duckdb://usage/task/{}", id.to_string().replace('-', "")),
        valid.as_str().replacen("0195", "%30195", 1),
        format!("duckdb://usage/task/{id}/extra"),
        format!("other://usage/task/{id}"),
    ] {
        assert!(DuckDbTaskUsageUri::parse(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn usage_pages_and_entries_reject_inconsistent_construction_and_decoding() {
    let ids = (1..=DUCKDB_USAGE_PAGE_SIZE as u64)
        .map(task)
        .collect::<Vec<_>>();
    let page = DuckDbUsagePage::from_task_ids(ids.clone(), ids.last().copied()).unwrap();
    assert_eq!(page.items().len(), DUCKDB_USAGE_PAGE_SIZE);
    let encoded = serde_json::to_value(&page).unwrap();
    assert_eq!(encoded["limit"], 100);
    assert_eq!(
        serde_json::from_value::<DuckDbUsagePage>(encoded.clone()).unwrap(),
        page
    );
    assert!(
        DuckDbUsagePage::from_task_ids(Vec::new(), None)
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
        assert!(DuckDbUsagePage::from_task_ids(ids, next).is_err());
    }
    let mut wrong_entry = encoded["items"][0].clone();
    wrong_entry["task_id"] = json!(task(9));
    assert!(serde_json::from_value::<DuckDbUsageEntry>(wrong_entry).is_err());
    let mut wrong_page = encoded;
    wrong_page["limit"] = json!(200);
    assert!(serde_json::from_value::<DuckDbUsagePage>(wrong_page).is_err());
}

#[test]
fn usage_cursor_and_page_follow_the_declared_collection_profile() {
    let id = task(7);
    // The cursor belongs to this collection and carries only a Task position.
    let published = URL_SAFE_NO_PAD.encode(format!(
        r#"{{"version":1,"collection":"duckdb://usage","after":"{id}"}}"#
    ));
    let cursor = DuckDbUsageCursor::new(id).unwrap();
    assert_eq!(cursor.as_str(), published);
    assert_eq!(DuckDbUsageCursor::parse(published).unwrap(), cursor);
    let page = DuckDbUsagePage::from_task_ids(vec![id], None).unwrap();
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "items": [{"task_id": id, "usage_uri": DuckDbTaskUsageUri::new(id).unwrap()}],
            "limit": 100, "next_cursor": null
        })
    );
    assert!(serde_json::from_value::<DuckDbUsagePage>(json!([])).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(DuckDbUsagePage)).unwrap();
    assert_eq!(schema["properties"]["items"]["maxItems"], 100);
    assert_eq!(schema["properties"]["limit"]["minimum"], 100);
    assert_eq!(schema["properties"]["limit"]["maximum"], 100);
}
