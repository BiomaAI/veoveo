use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_media_mcp::contract::{
    MEDIA_USAGE_PAGE_SIZE, MediaTaskUsageUri, MediaUsageCursor, MediaUsageEntry,
    MediaUsageIndexUri, MediaUsagePage,
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
    let usage = MediaTaskUsageUri::new(task_id).unwrap();
    assert_eq!(
        usage.as_str(),
        "media://usage/task/0195dabe-7777-7abc-8def-000000000007"
    );
    assert_eq!(MediaTaskUsageUri::parse(usage.as_str()).unwrap(), usage);
    assert_eq!(
        <MediaTaskUsageUri as ResourceAddress>::parse(&usage.to_uri().unwrap()).unwrap(),
        usage
    );
    let entry = MediaUsageEntry::new(task_id).unwrap();
    assert_eq!(entry.task_id(), task_id);
    assert_eq!(entry.usage_uri(), &usage);
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({"taskId": task_id, "usageUri": usage})
    );
    let cursor = MediaUsageCursor::new(task_id).unwrap();
    assert_eq!(
        MediaUsageCursor::parse(cursor.as_str()).unwrap().after(),
        task_id
    );
    for index in [
        MediaUsageIndexUri::new(None),
        MediaUsageIndexUri::new(Some(&cursor)),
    ] {
        assert_eq!(MediaUsageIndexUri::parse(index.as_str()).unwrap(), index);
        assert_eq!(
            serde_json::from_value::<MediaUsageIndexUri>(json!(index)).unwrap(),
            index
        );
    }
}

#[test]
fn usage_admission_rejects_invalid_envelopes_and_address_aliases() {
    let id = task(7);
    let cursor = MediaUsageCursor::new(id).unwrap();
    for envelope in [
        json!({"version":2,"collection":MediaUsageIndexUri::ROOT,"after":id}),
        json!({"version":1,"collection":"timeseries://usage","after":id}),
        json!({"version":1,"collection":MediaUsageIndexUri::ROOT,"after":"not-a-task"}),
        json!({"version":1,"collection":MediaUsageIndexUri::ROOT,"after":id,"extra":true}),
        json!({"version":1,"taskId":id}),
    ] {
        assert!(
            MediaUsageCursor::parse(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope).unwrap()))
                .is_err()
        );
    }
    for invalid in ["".to_owned(), "bad".to_owned(), "a".repeat(1025)] {
        assert!(MediaUsageCursor::parse(invalid).is_err());
    }
    for invalid in [
        "media://usage/".to_owned(),
        "media://usage?".to_owned(),
        "media://usage#fragment".to_owned(),
        "media://usage?limit=20".to_owned(),
        format!("media://usage?cursor={0}&cursor={0}", cursor.as_str()),
        format!("media://usage?%63ursor={}", cursor.as_str()),
        format!("media://usage/task/{id}?cursor={}", cursor.as_str()),
    ] {
        assert!(MediaUsageIndexUri::parse(&invalid).is_err(), "{invalid}");
    }
    for id in [
        "550e8400-e29b-41d4-a716-446655440000",
        "00000000-0000-0000-0000-000000000000",
        "0195dabe-7777-7abc-0def-000000000007",
    ] {
        let id: TaskId = id.parse().unwrap();
        assert!(MediaTaskUsageUri::new(id).is_err());
        assert!(MediaUsageCursor::new(id).is_err());
    }
    let valid = MediaTaskUsageUri::new(id).unwrap();
    for invalid in [
        valid.as_str().to_uppercase(),
        format!("{valid}/"),
        format!("{valid}?extra=x"),
        format!("{valid}#fragment"),
        format!("media://usage/task/{{{id}}}"),
        format!("media://usage/task/{}", id.to_string().replace('-', "")),
        valid.as_str().replacen("0195", "%30195", 1),
        format!("media://usage/task/{id}/extra"),
        format!("other://usage/task/{id}"),
    ] {
        assert!(MediaTaskUsageUri::parse(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn usage_pages_and_entries_reject_inconsistent_construction_and_decoding() {
    let ids = (1..=MEDIA_USAGE_PAGE_SIZE as u64)
        .map(task)
        .collect::<Vec<_>>();
    let page = MediaUsagePage::from_task_ids(ids.clone(), ids.last().copied()).unwrap();
    assert_eq!(page.items().len(), MEDIA_USAGE_PAGE_SIZE);
    let encoded = serde_json::to_value(&page).unwrap();
    assert_eq!(encoded["limit"], 100);
    assert_eq!(
        serde_json::from_value::<MediaUsagePage>(encoded.clone()).unwrap(),
        page
    );
    assert!(
        MediaUsagePage::from_task_ids(Vec::new(), None)
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
        assert!(MediaUsagePage::from_task_ids(ids, next).is_err());
    }
    let mut wrong_entry = encoded["items"][0].clone();
    wrong_entry["taskId"] = json!(task(9));
    assert!(serde_json::from_value::<MediaUsageEntry>(wrong_entry).is_err());
    let mut wrong_page = encoded;
    wrong_page["limit"] = json!(200);
    assert!(serde_json::from_value::<MediaUsagePage>(wrong_page).is_err());
}

#[test]
fn usage_cursor_and_page_follow_the_declared_collection_profile() {
    let id = task(7);
    // The cursor belongs to this collection and carries only a Task position.
    let published = URL_SAFE_NO_PAD.encode(format!(
        r#"{{"version":1,"collection":"media://usage","after":"{id}"}}"#
    ));
    let cursor = MediaUsageCursor::new(id).unwrap();
    assert_eq!(cursor.as_str(), published);
    assert_eq!(MediaUsageCursor::parse(published).unwrap(), cursor);
    let page = MediaUsagePage::from_task_ids(vec![id], None).unwrap();
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({
            "items": [{"taskId": id, "usageUri": MediaTaskUsageUri::new(id).unwrap()}],
            "limit": 100, "nextCursor": null
        })
    );
    assert!(serde_json::from_value::<MediaUsagePage>(json!([])).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(MediaUsagePage)).unwrap();
    assert_eq!(schema["properties"]["items"]["maxItems"], 100);
    assert_eq!(schema["properties"]["limit"]["minimum"], 100);
    assert_eq!(schema["properties"]["limit"]["maximum"], 100);
}

#[test]
fn current_page_requires_cursor_and_refuses_retired_root_and_entry_members() {
    let page = MediaUsagePage::from_task_ids(vec![task(1)], None).unwrap();
    let wire = serde_json::to_value(&page).unwrap();
    let schema = serde_json::to_value(schemars::schema_for!(MediaUsagePage)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&wire));
    assert_eq!(
        serde_json::from_value::<MediaUsagePage>(wire.clone()).unwrap(),
        page
    );
    for (current, retired, nested) in [
        ("nextCursor", "next_cursor", false),
        ("taskId", "task_id", true),
        ("usageUri", "usage_uri", true),
    ] {
        for retain_current in [false, true] {
            let mut bad = wire.clone();
            let object = if nested {
                bad["items"][0].as_object_mut().unwrap()
            } else {
                bad.as_object_mut().unwrap()
            };
            let value = object.get(current).unwrap().clone();
            if !retain_current {
                object.remove(current);
            }
            object.insert(retired.into(), value);
            assert!(
                !validator.is_valid(&bad),
                "{retired} mixed={retain_current}"
            );
            assert!(
                serde_json::from_value::<MediaUsagePage>(bad).is_err(),
                "{retired} mixed={retain_current}"
            );
        }
    }
    let mut omitted = wire;
    omitted.as_object_mut().unwrap().remove("nextCursor");
    assert!(!validator.is_valid(&omitted));
    assert!(serde_json::from_value::<MediaUsagePage>(omitted).is_err());
}
