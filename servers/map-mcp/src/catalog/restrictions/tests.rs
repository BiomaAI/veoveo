use super::*;
use crate::contract::*;
use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::{PlatformStore, PrincipalKind};

fn id(n: usize) -> RestrictionId {
    format!("restriction-{n:08x}-0000-7000-8000-000000000000")
        .parse()
        .unwrap()
}
async fn scope(store: &PlatformStore, tenant: &str, principal: &str) -> MapAccessContext {
    MapAccessContext {
        identity: store
            .ensure_identity(
                tenant,
                principal,
                "https://fixture.local",
                principal,
                PrincipalKind::Service,
            )
            .await
            .unwrap(),
    }
}
fn restriction(n: usize, at: DateTime<Utc>) -> Restriction {
    Restriction::new(crate::contract::RestrictionValue {
        restriction_id: id(n),
        kind: RestrictionKind::NavigationalWarning,
        geometry: Wgs84Polygon {
            exterior: [(0., 0.), (1., 0.), (1., 1.), (0., 1.), (0., 0.)]
                .into_iter()
                .map(|(x, y)| Wgs84Position::new(x, y, None).unwrap())
                .collect(),
            interiors: vec![],
        },
        vertical_band: None,
        affected_mobility_families: BTreeSet::from([
            MobilityFamily::Human,
            MobilityFamily::FixedWing,
        ]),
        effect: RestrictionEffect {
            kind: RestrictionEffectKind::Advise,
            limit: None,
            explanation: None,
        },
        valid_from: at,
        valid_until: None,
        authority: AuthorityClass::SyntheticTest,
        source_release_id: None,
        issued_at: at,
        cancelled_by: None,
        record_version: 1,
    })
    .expect("admitted Map fixture")
}
async fn change(store: &PlatformStore, id: &RestrictionId, sql: &str) {
    store
        .client()
        .query(sql)
        .bind(("id", RecordId::new("map_restriction", id.as_str())))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn pages_and_completion_apply_tenant_before_limits_and_recheck_continuations() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "restrictions", "author").await;
        let peer = scope(&db.a, "restrictions", "peer").await;
        let foreign = scope(&db.a, "foreign", "author").await;
        let at = Utc::now();
        for n in 0..110 {
            writer
                .create_restriction(&foreign, restriction(n, at))
                .await
                .unwrap();
            change(&db.a, &id(n), include_str!("../../queries/catalog/restrictions/tests/mutation_01.surql")).await;
        }
        for n in 1000..1125 {
            writer
                .create_restriction(&owner, restriction(n, at))
                .await
                .unwrap();
        }
        let first = reader
            .restrictions_page(&peer, &MapRestrictionsUri::new(None))
            .await
            .unwrap();
        assert_eq!(first.items().len(), 100);
        assert_eq!(first.items()[0].restriction_id(), &id(1000));
        let continuation = MapRestrictionsUri::new(first.next_cursor().cloned());
        assert!(
            reader
                .restrictions_page(&foreign, &continuation)
                .await
                .unwrap()
                .items()
                .is_empty()
        );
        let last = reader
            .restrictions_page(&peer, &continuation)
            .await
            .unwrap();
        assert_eq!(last.items().len(), 25);
        assert!(last.next_cursor().is_none());
        assert_eq!(last.items()[24].restriction_id(), &id(1124));
        assert!(
            reader
                .restriction(&foreign, &id(1124))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reader
                .restriction(&peer, &id(1124))
                .await
                .unwrap()
                .unwrap()
                .restriction_id,
            id(1124)
        );
        assert_eq!(
            reader.complete_restrictions(&peer, "").await.unwrap().len(),
            101
        );
        assert_eq!(
            reader
                .complete_restrictions(&peer, &id(1124).to_string().to_uppercase())
                .await
                .unwrap(),
            vec![id(1124)]
        );
        assert!(
            reader
                .complete_restrictions(&peer, "' OR true")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(reader.complete_restrictions(&peer, "\n").await.is_err());
        assert!(
            reader
                .complete_restrictions(&peer, &"x".repeat(513))
                .await
                .is_err()
        );
        // A cursor supplies position only; moving a row to another tenant immediately
        // changes both exact reads and the next page.
        db.a.client()
            .query(include_str!("../../queries/catalog/restrictions/tests/pages_and_completion_apply_tenant_before_limits_and_recheck_continuations/statement_1.surql"))
            .bind(("id", RecordId::new("map_restriction", id(1100).as_str())))
            .bind(("tenant", foreign.identity.tenant_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            reader
                .restrictions_page(&peer, &continuation)
                .await
                .unwrap()
                .items()
                .len(),
            24
        );
        assert!(
            reader
                .restriction(&peer, &id(1100))
                .await
                .unwrap()
                .is_none()
        );
        // The page contract admits only sorted unique items and a cursor at its tail.
        let mut wire = serde_json::to_value(&first).unwrap();
        assert_eq!(
            serde_json::from_value::<MapRestrictionPage>(wire.clone()).unwrap(),
            first
        );
        wire["next_cursor"] = serde_json::json!(MapRestrictionCursor::new(id(1098)));
        assert!(serde_json::from_value::<MapRestrictionPage>(wire).is_err());
        let mut items = first.items().to_vec();
        items.swap(0, 1);
        assert!(MapRestrictionPage::from_lookahead(items).is_err());
        assert!(
            MapRestrictionPage::from_lookahead(vec![
                RestrictionSummary::new(&restriction(1, at))
                    .unwrap();
                102
            ])
            .is_err()
        );
    })
    .await
    .expect("restriction page qualification exceeded 90 seconds");
}

#[tokio::test]
async fn operational_selection_uses_half_open_time_family_and_withdrawal_in_sql() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "operational", "author").await;
        let at = Utc::now();
        let hour = chrono::Duration::hours(1);
        let base = restriction(1000, at);
        writer
            .create_restriction(&owner, base.clone())
            .await
            .unwrap();
        let mut upper = restriction(1001, at - hour);
        upper.valid_until = Some(at);
        writer.create_restriction(&owner, upper).await.unwrap();
        let mut future = restriction(1002, at + hour);
        future.valid_until = Some(at + hour + hour);
        writer.create_restriction(&owner, future).await.unwrap();
        let mut other = restriction(1003, at);
        other.affected_mobility_families = BTreeSet::from([MobilityFamily::SurfaceVessel]);
        writer.create_restriction(&owner, other).await.unwrap();
        let mut cancelled = restriction(1004, at);
        cancelled.cancelled_by = Some(id(1));
        writer.create_restriction(&owner, cancelled).await.unwrap();
        // Nonmatching malformed documents prove the SQL predicates precede decoding.
        for n in [1001, 1002, 1003, 1004] {
            change(
                &db.a,
                &id(n),
                include_str!("../../queries/catalog/restrictions/tests/mutation_02.surql"),
            )
            .await;
        }
        let selected = reader
            .effective_restrictions(&owner, at, Some(MobilityFamily::Human))
            .await
            .unwrap();
        assert_eq!(selected, vec![base.clone()]);
        assert!(
            reader
                .effective_restrictions(&owner, at, None)
                .await
                .is_err()
        ); // other family is now selected
        assert!(
            reader
                .effective_restrictions(&owner, at - hour - hour, Some(MobilityFamily::Human))
                .await
                .unwrap()
                .is_empty()
        );
        writer
            .withdraw_restriction(&owner, base, 1, at + hour, id(2))
            .await
            .unwrap();
        assert!(
            reader
                .effective_restrictions(&owner, at, Some(MobilityFamily::Human))
                .await
                .unwrap()
                .is_empty()
        );
        let withdrawn = reader
            .restriction(&owner, &id(1000))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(withdrawn.record_version, 2);
        assert_eq!(withdrawn.cancelled_by, Some(id(2)));
    })
    .await
    .expect("effective restriction qualification exceeded 90 seconds");
}

#[tokio::test]
async fn selected_record_must_agree_with_indexed_identity_filters_and_version() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let owner = scope(&db.a, "retained", "author").await;
        let at = Utc::now();
        for (n, mutation) in [
            include_str!("../../queries/catalog/restrictions/tests/mutation_03.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_04.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_05.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_06.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_07.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_08.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_09.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_10.surql"),
            include_str!("../../queries/catalog/restrictions/tests/mutation_11.surql"),
        ]
        .into_iter()
        .enumerate()
        {
            writer
                .create_restriction(&owner, restriction(n, at))
                .await
                .unwrap();
            change(&db.a, &id(n), mutation).await;
            let key = if n == 0 { id(u32::MAX as usize) } else { id(n) };
            assert!(
                reader.restriction(&owner, &key).await.is_err(),
                "{mutation}"
            );
            assert!(
                reader
                    .restrictions_page(&owner, &MapRestrictionsUri::new(None))
                    .await
                    .is_err()
            );
            db.a.client()
                .query(include_str!("../../queries/catalog/restrictions/tests/selected_record_must_agree_with_indexed_identity_filters_and_version/statement_1.surql"))
                .bind(("id", RecordId::new("map_restriction", id(n).as_str())))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        writer
            .create_restriction(&owner, restriction(99, at))
            .await
            .unwrap();
        change(&db.a, &id(99), include_str!("../../queries/catalog/restrictions/tests/mutation_12.surql")).await;
        assert!(
            reader
                .complete_restrictions(&owner, "malformed")
                .await
                .is_err()
        );
    })
    .await
    .expect("retained restriction qualification exceeded 90 seconds");
}

#[tokio::test]
async fn effective_selection_never_silently_truncates_constraints() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let owner = scope(&db.a, "limit", "author").await;
        let at = Utc::now();
        let rows: Vec<_> = (0..=MAX_EFFECTIVE_RESTRICTIONS)
            .map(|n| MapRestrictionRecord {
                id: RecordId::new("map_restriction", id(n).as_str()),
                tenant: owner.identity.tenant_id.record_id(),
                owner: owner.identity.principal_id.record_id(),
                restriction_key: id(n).to_string(),
                kind: "navigational_warning".into(),
                effect_kind: "advise".into(),
                affected_mobility_families: vec!["human".into()],
                valid_from: at,
                valid_until: None,
                cancelled_by: None,
                canonical_json: "{}".into(),
                record_version: 1,
                created_at: at,
                updated_at: at,
            })
            .collect();
        // Keep fixture writes below the driver's WebSocket buffer bound and do
        // not let a transport error print an encoded multi-megabyte request.
        for (batch, rows) in rows.chunks(250).enumerate() {
            db.a.client()
                .query(include_str!(
                    "../../queries/catalog/restrictions/tests/bulk_restrictions.surql"
                ))
                .bind(("rows", rows.to_vec()))
                .await
                .unwrap_or_else(|_| panic!("restriction fixture batch {batch} transport failed"))
                .check()
                .unwrap_or_else(|_| panic!("restriction fixture batch {batch} insertion failed"));
        }
        let reader = MapCatalog::new(db.b.clone());
        let error = reader
            .effective_restrictions(&owner, at, Some(MobilityFamily::Human))
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("10000 effective-restriction limit"),
            "{error:#}"
        );
        // All 10001 malformed records disappear in SQL for another family,
        // leaving the valid record that follows them in ID order.
        let mut selected = restriction(MAX_EFFECTIVE_RESTRICTIONS + 1, at);
        selected.affected_mobility_families = BTreeSet::from([MobilityFamily::Uas]);
        reader
            .create_restriction(&owner, selected.clone())
            .await
            .unwrap();
        assert_eq!(
            reader
                .effective_restrictions(&owner, at, Some(MobilityFamily::Uas))
                .await
                .unwrap(),
            vec![selected]
        );
    })
    .await
    .expect("effective restriction bound qualification exceeded 90 seconds");
}
