use super::*;
use std::collections::BTreeSet;
use surrealdb::types::RecordId;

#[tokio::test]
async fn database_replay_crosses_unrelated_pages_and_keeps_whole_transactions() {
    let db = fixture::TestDb::new().await;
    let store = db.a.clone();
    store
        .client()
        .query(
            include_str!("../queries/surreal_integration/changefeed/database_replay_crosses_unrelated_pages_and_keeps_whole_transactions.surql"),
        )
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut cursor = store.changefeed_cursor_now().await.unwrap();
    for ordinal in 0..20 {
        store
            .client()
            .query(include_str!("../queries/surreal_integration/changefeed/database_replay_crosses_unrelated_pages_and_keeps_whole_transactions_2.surql"))
            .bind(("noise", RecordId::new("changefeed_noise_fixture", format!("noise{ordinal}"))))
            .bind(("ordinal", ordinal))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    store
        .client()
        .query(
            include_str!("../queries/surreal_integration/changefeed/database_replay_crosses_unrelated_pages_and_keeps_whole_transactions_3.surql"),
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
                        .any(|entry| entry.table() == Some("changefeed_noise_fixture")),
                    "the transaction tail must include the audit table before advancing"
                );
            }
            for entry in entries {
                if let ChangefeedEntry::Upsert(row) = entry {
                    let veoveo_platform_store::Value::RecordId(id) = row.get("id") else {
                        panic!("changefeed upsert has no record id");
                    };
                    // The inclusive clock anchor can overlap fresh kernel seed rows.
                    // Count the two fixture tables whose writes this test owns.
                    if !matches!(id.table.as_str(), "agent" | "changefeed_noise_fixture") {
                        continue;
                    }
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
}
