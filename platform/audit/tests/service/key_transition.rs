//! Persisted key transition and externally anchored verification, without installation secrets.
use super::*;
use veoveo_audit::integrity::{AuditKeyRing, AuditVerifier, IntegrityError};
use veoveo_platform_store::PlatformStore;

async fn archive(store: &PlatformStore) -> Vec<(AuditBlock, Vec<AuditRecord>)> {
    let scope = AuditReadScope::new(None, true);
    let mut after = None;
    let mut result = Vec::new();
    loop {
        let blocks = store
            .audit_blocks(&scope, &AuditPartition::Installation, after, 2)
            .await
            .unwrap();
        if blocks.is_empty() {
            return result;
        }
        for block in blocks {
            let records = store.audit_block_records(&scope, &block).await.unwrap();
            after = Some(block.head.sequence);
            result.push((block, records));
        }
        assert!(
            result.len() <= 32,
            "fixture archive exceeded its page budget"
        );
    }
}

fn verifier(keys: &AuditKeyRing, anchor: Option<AuditCheckpoint>) -> AuditVerifier<'_> {
    AuditVerifier::new(
        keys,
        AuditPartition::Installation,
        anchor,
        chrono::TimeDelta::seconds(30),
    )
    .unwrap()
}

#[tokio::test]
async fn drained_key_transition_preserves_archive_and_requires_post_cutover_trust() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let old = Arc::new(AuditSigningKey::from_seed(&[61; 32]));
        let replacement = Arc::new(AuditSigningKey::from_seed(&[62; 32]));
        let first = AuditService::start(
            db.a.clone(), old.clone(), NonZeroU32::new(1).unwrap(), Default::default(),
        ).unwrap();
        let writer = AuditWriter::start(db.a.clone());
        let before = (0..3).map(|_| draft()).collect::<Vec<_>>();
        for record in &before {
            writer.record(record.clone()).await.unwrap();
        }
        let final_old = draft();
        writer.record_completion(final_old.clone()).await;
        writer.shutdown(Duration::from_secs(10)).await.unwrap();
        first.shutdown(Duration::from_secs(15)).await.unwrap();
        assert!(matches!(writer.record(draft()).await, Err(AuditWriteError::Closed)));

        let prefix = archive(&db.b).await;
        assert!(!prefix.is_empty());
        assert_eq!(prefix.iter().map(|(_, r)| r.len()).sum::<usize>(), before.len() + 1);
        assert!(prefix.iter().all(|(b, _)| b.head.key_id == old.key_id()));
        let expected_before = before.iter().chain(std::iter::once(&final_old)).map(AuditDraft::id).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(prefix.iter().flat_map(|(_, records)| records.iter().map(|r| r.draft.id())).collect::<std::collections::BTreeSet<_>>(), expected_before);
        // In operation these checkpoints come from a separately protected archive,
        // not from an untrusted database or the historical signing seed.
        let cutover = prefix.last().unwrap().0.checkpoint();
        let old_lease = db.a.acquire_audit_seal_lease(uuid::Uuid::now_v7()).await.unwrap();
        db.a.release_audit_seal_lease(&old_lease).await.unwrap();
        let second = AuditService::start(
            db.b.clone(), replacement.clone(), NonZeroU32::new(1).unwrap(), Default::default(),
        ).unwrap();
        let mut health = second.health();
        state(&mut health, AuditHealthState::Active).await;
        let fenced = db.a.renew_audit_seal_lease(&old_lease, false).await;
        let writer = AuditWriter::start(db.b.clone());
        let after = (0..3).map(|_| draft()).collect::<Vec<_>>();
        for record in &after {
            writer.record(record.clone()).await.unwrap();
        }
        writer.shutdown(Duration::from_secs(10)).await.unwrap();
        second.shutdown(Duration::from_secs(15)).await.unwrap();
        assert!(matches!(fenced, Err(StoreError::AuditLeaseLost)));

        let complete = archive(&db.a).await;
        assert_eq!(&complete[..prefix.len()], prefix.as_slice(), "rotation rewrote retained blocks or records");
        let suffix = &complete[prefix.len()..];
        assert!(!suffix.is_empty());
        assert!(suffix.iter().all(|(b, _)| b.head.key_id == replacement.key_id()));
        assert_eq!(suffix.iter().map(|(_, r)| r.len()).sum::<usize>(), after.len());
        assert_eq!(suffix.iter().flat_map(|(_, records)| records.iter().map(|r| r.draft.id())).collect::<std::collections::BTreeSet<_>>(), after.iter().map(AuditDraft::id).collect());
        let expected_tail = complete.last().unwrap().0.checkpoint();
        let mut archive_keys = AuditKeyRing::default();
        archive_keys.insert(old.public_key()).unwrap();
        archive_keys.insert(replacement.public_key()).unwrap();
        let mut retained = verifier(&archive_keys, None);
        for (block, records) in &prefix {
            assert!(retained.verify(block, records).unwrap().is_empty());
        }
        retained.finish(&cutover).unwrap();
        let mut full = verifier(&archive_keys, None);
        for (block, records) in &complete {
            assert!(full.verify(block, records).unwrap().is_empty());
        }
        full.finish(&expected_tail).unwrap();

        let mut current_keys = AuditKeyRing::default();
        current_keys.insert(replacement.public_key()).unwrap();
        let mut current = verifier(&current_keys, Some(cutover.clone()));
        for (block, records) in suffix {
            assert!(current.verify(block, records).unwrap().is_empty());
        }
        current.finish(&expected_tail).unwrap();
        assert!(matches!(verifier(&current_keys, Some(cutover.clone())).finish(&expected_tail), Err(IntegrityError::Incomplete)));
        let (tail, records) = &suffix[0];
        assert!(matches!(verifier(&current_keys, None).verify(tail, records), Err(IntegrityError::Chain)), "a removed prefix requires an independently trusted anchor");
        assert!(matches!(verifier(&current_keys, None).verify(&prefix[0].0, &prefix[0].1), Err(IntegrityError::UnknownKey)));
        let mut missing = AuditKeyRing::default();
        missing.insert(old.public_key()).unwrap();
        assert!(matches!(verifier(&missing, Some(cutover.clone())).verify(tail, records), Err(IntegrityError::UnknownKey)));
        let mut wrong = AuditKeyRing::default();
        wrong.insert(AuditSigningKey::from_seed(&[63; 32]).public_key()).unwrap();
        assert!(matches!(verifier(&wrong, Some(cutover.clone())).verify(tail, records), Err(IntegrityError::UnknownKey)));
        let forged = old.seal(
            AuditPartition::Installation, Some(&cutover), tail.head.members.clone(), records, tail.head.sealed_at,
        ).unwrap();
        // Archive membership does not revoke the old key. The new-only profile is
        // what refuses a valid historical-key signature beyond the trusted anchor.
        verifier(&archive_keys, Some(cutover.clone())).verify(&forged, records).unwrap();
        assert!(matches!(verifier(&current_keys, Some(cutover.clone())).verify(&forged, records), Err(IntegrityError::UnknownKey)), "compromised historical key authorized a new suffix");
        let mut changed = records.clone();
        changed[0].recorded_at += chrono::TimeDelta::seconds(1);
        assert!(matches!(verifier(&current_keys, Some(cutover.clone())).verify(tail, &changed), Err(IntegrityError::RecordHash)));
        let mut tampered = tail.clone();
        tampered.head.sealed_at += chrono::TimeDelta::seconds(1);
        assert!(matches!(verifier(&current_keys, Some(cutover)).verify(&tampered, records), Err(IntegrityError::HeadHash)));
        println!("{{\"schema\":\"veoveo.ai/audit-key-transition-qualification/v1\",\"retainedRecords\":{},\"replacementRecords\":{},\"sequence\":{}}}", before.len() + 1, after.len(), expected_tail.sequence.get());
    }).await.expect("audit key transition exceeded90seconds");
}
