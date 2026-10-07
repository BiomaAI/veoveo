use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_optimization_mcp::contract::{
    OptimizationCollection, OptimizationCollectionUri, OptimizationIndexCursor,
};
use veoveo_types::{ResourceAddress, TaskId};

#[test]
fn collection_positions_publish_current_version_two_wire_bytes() {
    let task: TaskId = "0195dabe-7777-7abc-8def-000000000001".parse().unwrap();
    for collection in [
        OptimizationCollection::Problems,
        OptimizationCollection::Runs,
        OptimizationCollection::Solutions,
    ] {
        let cursor =
            OptimizationIndexCursor::new(collection, "2026-09-28T00:00:00Z".parse().unwrap(), task)
                .unwrap();
        let old_wire = format!(
            r#"{{"version":2,"collection":{},"createdAt":"2026-09-28T00:00:00Z","taskId":"{task}"}}"#,
            serde_json::to_string(&collection).unwrap()
        );
        assert_eq!(cursor.as_str(), URL_SAFE_NO_PAD.encode(old_wire));
        assert_eq!(
            OptimizationIndexCursor::parse(cursor.as_str()).unwrap(),
            cursor
        );
        let uri = OptimizationCollectionUri::new(collection, Some(cursor.clone())).unwrap();
        assert_eq!(OptimizationCollectionUri::parse(uri.as_str()).unwrap(), uri);
        assert_eq!(
            <OptimizationCollectionUri as ResourceAddress>::parse(&uri.to_uri().unwrap()).unwrap(),
            uri
        );
        assert_eq!(
            serde_json::to_value(&cursor).unwrap(),
            json!(cursor.as_str())
        );
        assert_eq!(
            serde_json::from_value::<OptimizationCollectionUri>(json!(uri.as_str())).unwrap(),
            uri
        );
        let other = if collection == OptimizationCollection::Runs {
            OptimizationCollection::Problems
        } else {
            OptimizationCollection::Runs
        };
        assert!(OptimizationCollectionUri::new(other, Some(cursor)).is_err());
        assert!(
            OptimizationCollectionUri::parse(uri.as_str().replace(collection.root(), other.root()))
                .is_err()
        );
    }
}

#[test]
fn collections_reject_ambiguous_addresses_and_invalid_native_positions() {
    for value in [
        "optimization://runs/",
        "optimization://runs?",
        "optimization://runs#fragment",
        "optimization://runs?limit=100",
        "optimization://runs?cursor=",
        "optimization://runs?cursor=x&cursor=x",
        "optimization://runs?cursor=x&unknown=y",
        "optimization://runs/item",
        "other://runs",
        "optimization://r%75ns",
        "OPTIMIZATION://runs",
    ] {
        assert!(
            OptimizationCollectionUri::parse(value).is_err(),
            "accepted {value}"
        );
    }
    let valid = json!({"version":2,"collection":"runs","createdAt":"2026-09-28T00:00:00Z","taskId":"0195dabe-7777-7abc-8def-000000000001"});
    for (field, value) in [
        ("version", json!(1)),
        ("collection", json!("unknown")),
        ("taskId", json!("0195dabe-7777-4abc-8def-000000000001")),
        ("taskId", json!("0195dabe-7777-7abc-0def-000000000001")),
        ("extra", json!(true)),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        assert!(
            OptimizationIndexCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&bad).unwrap())
            )
            .is_err()
        );
    }
    assert!(OptimizationIndexCursor::parse("x".repeat(1025)).is_err());
}

#[test]
fn current_cursor_refuses_retired_replacement_and_mixed_members() {
    let current = json!({"version":2,"collection":"runs","createdAt":"2026-09-28T00:00:00Z","taskId":"0195dabe-7777-7abc-8def-000000000001"});
    assert!(
        OptimizationIndexCursor::parse(
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&current).unwrap())
        )
        .is_ok()
    );
    for mixed in [false, true] {
        let mut bad = current.clone();
        let object = bad.as_object_mut().unwrap();
        let value = if mixed {
            object["createdAt"].clone()
        } else {
            object.remove("createdAt").unwrap()
        };
        object.insert("created_at".into(), value);
        assert!(
            OptimizationIndexCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&bad).unwrap())
            )
            .is_err()
        );
    }
    for mixed in [false, true] {
        let mut bad = current.clone();
        let object = bad.as_object_mut().unwrap();
        let value = if mixed {
            object["taskId"].clone()
        } else {
            object.remove("taskId").unwrap()
        };
        object.insert("task_id".into(), value);
        assert!(
            OptimizationIndexCursor::parse(
                URL_SAFE_NO_PAD.encode(serde_json::to_vec(&bad).unwrap())
            )
            .is_err()
        );
    }
}
