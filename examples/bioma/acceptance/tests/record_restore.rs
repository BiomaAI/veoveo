//! Recovery query binding qualification uses only disposable fixture records.
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

use anyhow::{Result, ensure};
use surrealdb::types::Value;
use veoveo_platform_store::{PrincipalKind, PrincipalRecord};

#[tokio::test]
async fn bound_record_restore_preserves_content_and_rolls_back_conflicting_batches() -> Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(180), restore_records())
        .await
        .map_err(|_| anyhow::anyhow!("bound record restore qualification exceeded 180 seconds"))?
}

async fn restore_records() -> Result<()> {
    let source = fixture::TestDb::new().await;
    let restored = fixture::TestDb::new().await;
    let mut ids = Vec::new();
    for key in ["restore-a", "restore-b", "restore-c"] {
        let identity = source
            .a
            .ensure_named_identity(
                "restore-fixture",
                key,
                "https://restore.test",
                key,
                PrincipalKind::User,
                "Name with 'quotes', \\slashes and ünicode",
            )
            .await?;
        ids.push(identity.principal_id.record_id());
    }
    let before: Vec<Value> = source
        .a
        .client()
        .query(include_str!("queries/record_restore/select.surql"))
        .bind(("records", ids.clone()))
        .await?
        .check()?
        .take(0)?;
    ensure!(before.len() == 3, "incomplete restore fixture");
    let Value::Object(fields) = &before[0] else {
        anyhow::bail!("record object required");
    };
    restored
        .a
        .client()
        .query(include_str!("queries/record_restore/restore_one.surql"))
        .bind(("id", fields.get("id").unwrap().clone()))
        .bind(("record", before[0].clone()))
        .await?
        .check()?;
    let single: Vec<Value> = restored
        .a
        .client()
        .query(include_str!("queries/record_restore/select.surql"))
        .bind(("records", ids.clone()))
        .await?
        .check()?
        .take(0)?;
    ensure!(
        single == [before[0].clone()],
        "single bound restore changed content"
    );
    for id in &ids {
        let _: Option<PrincipalRecord> = restored.a.client().delete(id.clone()).await?;
    }
    restored
        .a
        .client()
        .query(include_str!("queries/record_restore/restore_batch.surql"))
        .bind(("records", before.clone()))
        .await?
        .check()?;
    let after: Vec<Value> = restored
        .a
        .client()
        .query(include_str!("queries/record_restore/select.surql"))
        .bind(("records", ids.clone()))
        .await?
        .check()?
        .take(0)?;
    ensure!(
        after == before,
        "transactional bound restore changed content"
    );
    for row in &before[..2] {
        let Value::Object(fields) = row else {
            anyhow::bail!("record object required");
        };
        let Value::RecordId(id) = fields.get("id").unwrap() else {
            anyhow::bail!("record identity required");
        };
        let _: Option<PrincipalRecord> = restored.a.client().delete(id.clone()).await?;
    }
    ensure!(
        restored
            .a
            .client()
            .query(include_str!("queries/record_restore/restore_batch.surql"))
            .bind(("records", before.clone()))
            .await?
            .check()
            .is_err(),
        "restore accepted conflicting final record"
    );
    let retained: Vec<Value> = restored
        .a
        .client()
        .query(include_str!("queries/record_restore/select.surql"))
        .bind(("records", ids))
        .await?
        .check()?
        .take(0)?;
    ensure!(
        retained == [before[2].clone()],
        "failed restore left partial records"
    );
    Ok(())
}
