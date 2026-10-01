use veoveo_audit_contract::*;

fn actor() -> AuditActor {
    AuditActor {
        principal: "indexer".parse().unwrap(),
        kind: AuditPrincipalKind::Service,
        tenant: Some("tenant".parse().unwrap()),
        oauth_client: Some("indexer".parse().unwrap()),
        session_family: None,
        delegating_principal: None,
        managed_agent: None,
    }
}
#[test]
fn indexing_summary_checks_attribution_bounds_counts_and_collection_on_decode() {
    let start: chrono::DateTime<chrono::Utc> = "2026-10-01T12:00:00Z".parse().unwrap();
    let end = start + chrono::Duration::minutes(5);
    let draft = AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::Server {
            server: "time".parse().unwrap(),
        },
        AuditDetail::IndexingWindow {
            collection: "time.docs".parse().unwrap(),
            start,
            end,
            reads: 5,
            failed: 1,
            not_modified: 2,
            members_digest: veoveo_types::Sha256Digest::from_bytes([1; 32]),
        },
        AuditOutcome::Succeeded,
        AuditReason::Accepted,
    )
    .actor(actor())
    .occurred_at(end)
    .build()
    .unwrap();
    let value = serde_json::to_value(draft).unwrap();
    for (path, bad) in [
        ("/detail/start", serde_json::json!("2026-10-01T12:00:01Z")),
        ("/detail/end", serde_json::json!("2026-10-01T12:10:00Z")),
        ("/detail/reads", serde_json::json!(0)),
        ("/detail/failed", serde_json::json!(6)),
        ("/detail/not_modified", serde_json::json!(5)),
        ("/actor/kind", serde_json::json!("user")),
        ("/actor/oauth_client", serde_json::Value::Null),
        ("/target/server", serde_json::json!("map")),
        ("/occurred_at", serde_json::json!("2026-10-01T12:00:00Z")),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(path).unwrap() = bad;
        assert!(
            serde_json::from_value::<AuditDraft>(invalid).is_err(),
            "{path}"
        );
    }
    assert!(serde_json::from_value::<AuditDraft>(value).is_ok());
}
#[test]
fn indexing_reads_reject_denials_and_wrong_owners() {
    let draft = |outcome, reason| {
        AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Resource {
                server: "time".parse().unwrap(),
                uri: veoveo_types::ResourceUri::new("time://docs").unwrap(),
            },
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            outcome,
            reason,
        )
        .actor(actor())
        .build()
        .unwrap()
    };
    assert!(
        IndexingRead::new(
            draft(AuditOutcome::Denied, AuditReason::PolicyDenied),
            "time.docs".parse().unwrap()
        )
        .is_err()
    );
    assert!(
        IndexingRead::new(
            draft(AuditOutcome::Succeeded, AuditReason::Accepted),
            "map.docs".parse().unwrap()
        )
        .is_err()
    );
    assert!(
        IndexingRead::new(
            draft(AuditOutcome::Succeeded, AuditReason::Accepted),
            "time.docs".parse().unwrap()
        )
        .is_ok()
    );
}
