use veoveo_platform_store::{PlatformStore, audit::AuditCommittedRecords, audit::AuditSealLease};

/// Advance schema-only pages before a fixture seals records it has just appended.
/// An empty page is terminal only when its cursor stops advancing.
pub async fn committed_records(
    store: &PlatformStore,
    lease: &mut AuditSealLease,
) -> AuditCommittedRecords {
    loop {
        let page = store.audit_committed_records(lease).await.unwrap();
        if !page.records.is_empty() {
            return page;
        }
        assert!(
            page.next.get() > lease.cursor as u64,
            "changefeed exhausted before acknowledged fixture records"
        );
        store
            .commit_audit_blocks(lease, page.next, &[])
            .await
            .unwrap();
        *lease = store.acquire_audit_seal_lease(lease.owner).await.unwrap();
    }
}
