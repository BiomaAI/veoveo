use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_optimization_mcp::contract::{
    OptimizationCollection, OptimizationCollectionUri, OptimizationIndexCursor,
};
use veoveo_types::{ResourceAddress, TaskId};

#[test]
fn collection_positions_preserve_version_one_wire_bytes() {
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
            r#"{{"version":1,"collection":{},"created_at":"2026-09-28T00:00:00Z","task_id":"{task}"}}"#,
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
    let valid = json!({"version":1,"collection":"runs","created_at":"2026-09-28T00:00:00Z","task_id":"0195dabe-7777-7abc-8def-000000000001"});
    for (field, value) in [
        ("version", json!(2)),
        ("collection", json!("unknown")),
        ("task_id", json!("0195dabe-7777-4abc-8def-000000000001")),
        ("task_id", json!("0195dabe-7777-7abc-0def-000000000001")),
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
