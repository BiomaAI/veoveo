use serde_json::json;
use veoveo_frames_contract::{
    FRAME_USAGE_PAGE_SIZE, FrameTaskUsageUri, FrameUsageCursor, FrameUsageEntry,
    FrameUsageIndexUri, FrameUsagePage, FrameWorldCursor, FrameWorldId,
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
    let usage = FrameTaskUsageUri::new(task_id).unwrap();
    assert_eq!(
        usage.as_str(),
        "frames://usage/task/0195dabe-7777-7abc-8def-000000000007"
    );
    assert_eq!(FrameTaskUsageUri::parse(usage.as_str()).unwrap(), usage);
    assert_eq!(
        <FrameTaskUsageUri as ResourceAddress>::parse(&usage.to_uri().unwrap()).unwrap(),
        usage
    );
    let entry = FrameUsageEntry::new(task_id).unwrap();
    assert_eq!(entry.task_id(), task_id);
    assert_eq!(entry.usage_uri(), &usage);
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({"taskId": task_id, "usageUri": usage})
    );
    let cursor = FrameUsageCursor::new(task_id).unwrap();
    assert_eq!(
        FrameUsageCursor::parse(cursor.as_str()).unwrap().after(),
        task_id
    );
    for index in [
        FrameUsageIndexUri::new(None),
        FrameUsageIndexUri::new(Some(&cursor)),
    ] {
        assert_eq!(FrameUsageIndexUri::parse(index.as_str()).unwrap(), index);
        assert_eq!(
            serde_json::from_value::<FrameUsageIndexUri>(json!(index)).unwrap(),
            index
        );
    }
}

#[test]
fn usage_admission_rejects_wrong_collection_versions_and_address_aliases() {
    let id = task(7);
    let cursor = FrameUsageCursor::new(id).unwrap();
    let world = FrameWorldCursor::new(&FrameWorldId::parse("world").unwrap());
    assert!(FrameUsageCursor::parse(world.as_str()).is_err());
    for envelope in [
        json!({"version":2,"collection":FrameUsageIndexUri::ROOT,"after":id}),
        json!({"version":1,"collection":"other://usage","after":id}),
        json!({"version":1,"collection":FrameUsageIndexUri::ROOT,"after":"not-a-task"}),
        json!({"version":1,"collection":FrameUsageIndexUri::ROOT,"after":id,"extra":true}),
    ] {
        assert!(
            FrameUsageCursor::parse(hex::encode(serde_json::to_vec(&envelope).unwrap())).is_err()
        );
    }
    for invalid in ["".to_owned(), "bad".to_owned(), "a".repeat(1025)] {
        assert!(FrameUsageCursor::parse(invalid).is_err());
    }
    for invalid in [
        "frames://usage/".to_owned(),
        "frames://usage?".to_owned(),
        "frames://usage#fragment".to_owned(),
        "frames://usage?limit=20".to_owned(),
        format!("frames://usage?cursor={0}&cursor={0}", cursor.as_str()),
        format!("frames://usage?%63ursor={}", cursor.as_str()),
        format!("frames://usage/task/{id}?cursor={}", cursor.as_str()),
    ] {
        assert!(FrameUsageIndexUri::parse(&invalid).is_err(), "{invalid}");
    }
    for id in [
        "550e8400-e29b-41d4-a716-446655440000",
        "00000000-0000-0000-0000-000000000000",
    ] {
        let id: TaskId = id.parse().unwrap();
        assert!(FrameTaskUsageUri::new(id).is_err());
        assert!(FrameUsageCursor::new(id).is_err());
    }
    let valid = FrameTaskUsageUri::new(id).unwrap();
    for invalid in [
        valid.as_str().to_uppercase(),
        format!("{valid}/"),
        format!("{valid}?extra=x"),
        format!("{valid}#fragment"),
        format!("frames://usage/task/{{{id}}}"),
        format!("frames://usage/task/{}", id.to_string().replace('-', "")),
        valid.as_str().replacen("0195", "%30195", 1),
        format!("frames://usage/task/{id}/extra"),
        format!("other://usage/task/{id}"),
    ] {
        assert!(FrameTaskUsageUri::parse(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn usage_pages_and_entries_reject_inconsistent_construction_and_decoding() {
    let ids = (1..=FRAME_USAGE_PAGE_SIZE as u64)
        .map(task)
        .collect::<Vec<_>>();
    let page = FrameUsagePage::from_task_ids(ids.clone(), ids.last().copied()).unwrap();
    assert_eq!(page.items().len(), FRAME_USAGE_PAGE_SIZE);
    let encoded = serde_json::to_value(&page).unwrap();
    assert_eq!(encoded["limit"], 100);
    assert_eq!(
        serde_json::from_value::<FrameUsagePage>(encoded.clone()).unwrap(),
        page
    );
    assert!(
        FrameUsagePage::from_task_ids(Vec::new(), None)
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
        assert!(FrameUsagePage::from_task_ids(ids, next).is_err());
    }
    let mut wrong_entry = encoded["items"][0].clone();
    wrong_entry["taskId"] = json!(task(9));
    assert!(serde_json::from_value::<FrameUsageEntry>(wrong_entry).is_err());
    let mut wrong_page = encoded;
    wrong_page["limit"] = json!(200);
    assert!(serde_json::from_value::<FrameUsagePage>(wrong_page).is_err());
}
