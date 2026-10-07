use serde_json::json;
use veoveo_duckdb_mcp::contract::*;

#[test]
fn database_pages_derive_identities_and_require_ordered_complete_continuations() {
    let ids = (0..100)
        .map(|n| format!("db_{n:03}").parse().unwrap())
        .collect();
    let page = DuckDbDatabasePage::from_ids(ids, true).unwrap();
    let wire = serde_json::to_value(&page).unwrap();
    assert_eq!(wire["limit"], 100);
    assert_eq!(
        wire["items"][0],
        json!({"dbId":"db_000","dbUri":"duckdb://db/db_000"})
    );
    assert_eq!(page.next_cursor().unwrap().after().as_str(), "db_099");
    assert_eq!(
        serde_json::from_value::<DuckDbDatabasePage>(wire.clone()).unwrap(),
        page
    );
    let schema = serde_json::to_value(schemars::schema_for!(DuckDbDatabasePage)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&wire));
    for (pointer, value, schema_valid) in [
        ("/limit", json!(99), false),
        ("/items/0/dbId", json!("different"), true),
        ("/items/0/dbUri", json!("duckdb://db/different"), true),
        (
            "/nextCursor",
            json!(DuckDbDatabaseCursor::new("different".parse().unwrap())),
            true,
        ),
    ] {
        let mut bad = wire.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert_eq!(validator.is_valid(&bad), schema_valid, "{pointer}");
        assert!(
            serde_json::from_value::<DuckDbDatabasePage>(bad).is_err(),
            "{pointer}"
        );
    }
    let mut bad = wire.clone();
    bad["items"].as_array_mut().unwrap().swap(0, 1);
    assert!(serde_json::from_value::<DuckDbDatabasePage>(bad).is_err());
    let mut bad = wire;
    bad["items"].as_array_mut().unwrap().pop();
    assert!(serde_json::from_value::<DuckDbDatabasePage>(bad).is_err());
    for ids in [vec!["b", "a"], vec!["a", "a"], vec!["a"; 101]] {
        assert!(
            DuckDbDatabasePage::from_ids(
                ids.into_iter().map(|s| s.parse().unwrap()).collect(),
                false
            )
            .is_err()
        );
    }
    assert!(DuckDbDatabasePage::from_ids(vec![], true).is_err());
    assert!(
        DuckDbDatabasePage::from_ids(vec![], false)
            .unwrap()
            .next_cursor()
            .is_none()
    );
}

#[test]
fn database_cursor_admits_only_its_collection_version_and_id_profile() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let cursor = DuckDbDatabaseCursor::new("metrics".parse().unwrap());
    assert_eq!(
        DuckDbDatabaseCursor::parse(cursor.as_str()).unwrap(),
        cursor
    );
    for wire in [
        json!({"version":2,"collection":"duckdb://dbs","after":"metrics"}),
        json!({"version":1,"collection":"duckdb://usage","after":"metrics"}),
        json!({"version":1,"collection":"duckdb://dbs","after":"../private"}),
        json!({"version":1,"collection":"duckdb://dbs","after":"metrics","extra":true}),
    ] {
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&wire).unwrap());
        assert!(DuckDbDatabaseCursor::parse(encoded).is_err());
    }
    for wire in ["".to_owned(), "private/token".to_owned(), "a".repeat(1025)] {
        let error = DuckDbDatabaseCursor::parse(&wire).unwrap_err();
        assert!(!error.to_string().contains("private/token"));
    }
}

#[test]
fn database_id_schema_and_diagnostics_match_the_admission_profile() {
    let schema = serde_json::to_value(schemars::schema_for!(DuckDbDatabaseId)).unwrap();
    assert_eq!(schema["pattern"], "^[a-z][a-z0-9_]{0,63}$");
    assert!(DuckDbDatabaseId::new("a".repeat(64)).is_ok());
    assert!(DuckDbDatabaseId::new("a".repeat(65)).is_err());
    assert!(
        !DuckDbDatabaseId::new("private/token")
            .unwrap_err()
            .to_string()
            .contains("private/token")
    );
}
