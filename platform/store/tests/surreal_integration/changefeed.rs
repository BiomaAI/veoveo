use super::*;
use std::collections::BTreeSet;

#[tokio::test]
async fn database_replay_crosses_unrelated_pages_and_keeps_whole_transactions() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let endpoint =
        std::env::var("VEOVEO_SURREAL_URL").unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let store = PlatformStore::connect(
        StoreConfig::builder(
            endpoint,
            "veoveo_integration",
            format!("changefeed_sparse_{}", Uuid::now_v7().simple()),
            StoreCredentials::root(username, SecretString::from(password)),
        )
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    store
        .client()
        .query(
            "DEFINE TABLE agent SCHEMAFULL CHANGEFEED 1h INCLUDE ORIGINAL;
         DEFINE FIELD lease ON agent TYPE datetime;
         DEFINE TABLE audit_event SCHEMAFULL CHANGEFEED 1h INCLUDE ORIGINAL;
         DEFINE FIELD ordinal ON audit_event TYPE int;",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut cursor = store.changefeed_cursor_now().await.unwrap();
    for ordinal in 0..20 {
        store
            .client()
            .query(format!(
                "CREATE audit_event:noise{ordinal} SET ordinal = {ordinal};"
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    store
        .client()
        .query(
            "BEGIN TRANSACTION;
         CREATE agent:pilot SET lease = time::now() + 30s;
         CREATE audit_event:coupled SET ordinal = 99;
         COMMIT TRANSACTION;",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut rows = BTreeSet::new();
    let mut agent_batches = 0;
    let mut exhausted = false;
    for _ in 0..30 {
        // LIMIT 1 cuts through the multi-table transaction and through pages
        // containing no agent. Neither case may hide the renewed runtime lease.
        let batches = store.replay_changes(cursor, 1).await.unwrap();
        let Some(last) = batches.last() else {
            exhausted = true;
            break;
        };
        let next = ChangefeedCursor::from_versionstamp(last.versionstamp + 1).unwrap();
        assert!(next > cursor);
        for batch in batches {
            let entries: Vec<_> = batch
                .changes
                .iter()
                .map(|change| decode_changefeed_entry(change).unwrap())
                .collect();
            if entries.iter().any(|entry| entry.table() == Some("agent")) {
                agent_batches += 1;
                assert!(
                    entries
                        .iter()
                        .any(|entry| entry.table() == Some("audit_event")),
                    "the transaction tail must include the audit table before advancing"
                );
            }
            for entry in entries {
                if let ChangefeedEntry::Upsert(row) = entry {
                    let veoveo_platform_store::Value::RecordId(id) = row.get("id") else {
                        panic!("changefeed upsert has no record id");
                    };
                    let RecordIdKey::String(key) = &id.key else {
                        panic!("fixture records use string keys");
                    };
                    assert!(
                        rows.insert((id.table.as_str().to_owned(), key.clone())),
                        "cursor replay duplicated a row"
                    );
                }
            }
        }
        cursor = next;
    }
    assert!(
        exhausted,
        "bounded replay must catch up past unrelated traffic"
    );
    assert_eq!(agent_batches, 1);
    assert_eq!(rows.len(), 22);
    store
        .client()
        .query(format!("REMOVE DATABASE {};", store.config().database()))
        .await
        .unwrap()
        .check()
        .unwrap();
}
