use super::*;

fn entry(version: u32, required: u32) -> DownstreamMigration {
    DownstreamMigration {
        migration: Migration {
            version,
            name: "fixture",
            filename: if version == 0 {
                "0000_fixture.surql"
            } else {
                "0001_fixture.surql"
            },
            sql: "RETURN true;",
        },
        requires_upstream: required,
    }
}

#[test]
fn dependencies_and_history_are_checked_before_execution() {
    assert!(validate(&[entry(0, 9)], 8).is_err());
    assert!(validate(&[entry(1, 1)], 8).is_err());
    assert!(validate(&[entry(0, 2), entry(1, 1)], 8).is_err());
    assert!(validate(&[], 8).is_ok());
    let item = entry(0, 8);
    let row = AppliedDownstreamMigration {
        id: RecordId::new("platform_downstream_migration", 0_i64),
        version: 0,
        name: "fixture".into(),
        filename: item.migration.filename.into(),
        checksum: item.migration.checksum(),
        requires_upstream: 8,
        applied_at: Utc::now(),
    };
    assert!(
        history_status(&[item], &[row.clone()], Some(8))
            .unwrap()
            .is_current()
    );
    assert!(history_status(&[], &[row.clone()], Some(8)).is_err());
    assert!(history_status(&[item], &[row.clone()], Some(7)).is_err());
    assert!(history_status(&[item], &[row.clone(), row.clone()], Some(8)).is_err());
    let mut drift = row;
    drift.requires_upstream = 7;
    assert!(matches!(
        history_status(&[item], &[drift], Some(8)),
        Err(DownstreamMigrationError::Drift { version: 0 })
    ));
}

#[test]
fn malformed_identities_and_history_gaps_fail_closed() {
    let catalog = [entry(0, 0), entry(1, 0)];
    let row = AppliedDownstreamMigration {
        id: RecordId::new("platform_downstream_migration", 1_i64),
        version: 1,
        name: "fixture".into(),
        filename: catalog[1].migration.filename.into(),
        checksum: catalog[1].migration.checksum(),
        requires_upstream: 0,
        applied_at: Utc::now(),
    };
    assert!(matches!(
        history_status(&catalog, &[row.clone()], Some(0)),
        Err(DownstreamMigrationError::HistoryGap { version: 0 })
    ));
    for version in [-1, i64::from(u32::MAX) + 1] {
        let mut malformed = row.clone();
        malformed.version = version;
        assert!(matches!(
            history_status(&catalog, &[malformed], Some(0)),
            Err(DownstreamMigrationError::InvalidHistory { .. })
        ));
    }
    let mut malformed = row;
    malformed.id = RecordId::new("other_table", 1_i64);
    assert!(matches!(
        history_status(&catalog, &[malformed], Some(0)),
        Err(DownstreamMigrationError::InvalidHistory { .. })
    ));
}
