//! Kernel Identity qualification without selecting Workspace or another optional owner.
#![cfg(feature = "runtime")]
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use fixture::TestDb;
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::{PlatformIdentity, PlatformStore, PrincipalKind};

#[derive(Debug, PartialEq, SurrealValue)]
struct Person {
    id: RecordId,
    display_name: String,
}

#[derive(Debug, SurrealValue)]
struct Labels {
    person: Person,
    tenant_name: String,
    work_context_title: String,
}

async fn person(store: &PlatformStore, tenant: &str, key: &str, name: &str) -> PlatformIdentity {
    store
        .ensure_named_identity(
            tenant,
            key,
            "https://identity.test",
            key,
            PrincipalKind::User,
            name,
        )
        .await
        .unwrap()
}

async fn configure(store: &PlatformStore, identity: &PlatformIdentity, enabled: bool, kind: &str) {
    store
        .client()
        .query(include_str!("queries/identity_api/configure.surql"))
        .bind(("principal", identity.principal_id.record_id()))
        .bind(("enabled", enabled))
        .bind(("kind", kind.to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
}

async fn summaries(
    store: &PlatformStore,
    tenant: RecordId,
    principals: Vec<RecordId>,
) -> Vec<Person> {
    store
        .client()
        .query(include_str!("queries/identity_api/summaries.surql"))
        .bind(("tenant", tenant))
        .bind(("principals", principals))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

#[tokio::test]
async fn summaries_preserve_attribution_and_reject_oversized_inputs() {
    let db = TestDb::new().await;
    let alice = person(&db.a, "identity-api", "alice", "Alice").await;
    let disabled = person(&db.a, "identity-api", "disabled", "Historical user").await;
    let service = person(&db.a, "identity-api", "service", "Service actor").await;
    let foreign = person(&db.a, "identity-api-foreign", "foreign", "Foreign user").await;
    configure(&db.a, &disabled, false, "user").await;
    configure(&db.a, &service, true, "service").await;
    let missing = RecordId::new(
        "principal",
        surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
    );
    let mut names = summaries(
        &db.a,
        alice.tenant_id.record_id(),
        vec![
            alice.principal_id.record_id(),
            disabled.principal_id.record_id(),
            service.principal_id.record_id(),
            foreign.principal_id.record_id(),
            missing.clone(),
            alice.principal_id.record_id(),
        ],
    )
    .await;
    names.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    assert_eq!(
        names,
        vec![
            Person {
                id: alice.principal_id.record_id(),
                display_name: "Alice".into()
            },
            Person {
                id: disabled.principal_id.record_id(),
                display_name: "Historical user".into()
            },
            Person {
                id: service.principal_id.record_id(),
                display_name: "Service actor".into()
            },
        ]
    );
    assert!(
        summaries(&db.a, alice.tenant_id.record_id(), vec![])
            .await
            .is_empty()
    );
    assert_eq!(
        summaries(
            &db.a,
            alice.tenant_id.record_id(),
            vec![alice.principal_id.record_id(); 256]
        )
        .await
        .len(),
        1
    );
    assert!(
        db.a.client()
            .query(include_str!("queries/identity_api/summaries.surql"))
            .bind(("tenant", alice.tenant_id.record_id()))
            .bind(("principals", vec![alice.principal_id.record_id(); 257]))
            .await
            .unwrap()
            .check()
            .is_err()
    );
    for (principal, expected) in [
        (alice.principal_id.record_id(), true),
        (disabled.principal_id.record_id(), false),
        (service.principal_id.record_id(), false),
        (foreign.principal_id.record_id(), false),
        (missing, false),
    ] {
        let enabled: surrealdb::types::Value =
            db.a.client()
                .query(include_str!("queries/identity_api/enabled.surql"))
                .bind(("tenant", alice.tenant_id.record_id()))
                .bind(("principal", principal))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        assert_eq!(bool::from_value(enabled).unwrap(), expected);
    }
}

#[tokio::test]
async fn search_filters_before_limiting_and_labels_validate_both_relationships() {
    let db = TestDb::new().await;
    let alice = person(&db.a, "identity-search", "alice", "Alice").await;
    let disabled = person(&db.a, "identity-search", "disabled", "00 Match disabled").await;
    let service = person(&db.a, "identity-search", "service", "00 Match service").await;
    let foreign = person(
        &db.a,
        "identity-search-foreign",
        "foreign",
        "00 Match foreign",
    )
    .await;
    configure(&db.a, &disabled, false, "user").await;
    configure(&db.a, &service, true, "service").await;
    for index in (0..25).rev() {
        person(
            &db.a,
            "identity-search",
            &format!("match-{index}"),
            &format!("Match {index:02}"),
        )
        .await;
    }
    let names: Vec<Person> =
        db.a.client()
            .query(include_str!("queries/identity_api/search.surql"))
            .bind(("tenant", alice.tenant_id.record_id()))
            .bind(("search", "match"))
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(
        names
            .iter()
            .map(|p| p.display_name.clone())
            .collect::<Vec<_>>(),
        (0..20)
            .map(|index| format!("Match {index:02}"))
            .collect::<Vec<_>>()
    );
    let context = RecordId::new(
        "work_context",
        surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
    );
    let foreign_context = RecordId::new(
        "work_context",
        surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
    );
    for (context, tenant) in [
        (context.clone(), alice.tenant_id.record_id()),
        (foreign_context.clone(), foreign.tenant_id.record_id()),
    ] {
        db.a.client()
            .query(include_str!("queries/identity_api/context.surql"))
            .bind(("tenant", tenant))
            .bind(("context", context))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let labels: Option<Labels> =
        db.a.client()
            .query(include_str!("queries/identity_api/labels.surql"))
            .bind(("tenant", alice.tenant_id.record_id()))
            .bind(("context", context.clone()))
            .bind(("principal", alice.principal_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    let labels = labels.unwrap();
    assert_eq!(
        labels.person,
        Person {
            id: alice.principal_id.record_id(),
            display_name: "Alice".into()
        }
    );
    assert_eq!(labels.work_context_title, "Identity labels");
    let tenant: veoveo_platform_store::TenantRecord =
        db.a.client()
            .select(alice.tenant_id.record_id())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(labels.tenant_name, tenant.name);
    for (tenant, context, principal) in [
        (
            alice.tenant_id.record_id(),
            context.clone(),
            foreign.principal_id.record_id(),
        ),
        (
            alice.tenant_id.record_id(),
            foreign_context,
            alice.principal_id.record_id(),
        ),
        (
            alice.tenant_id.record_id(),
            context.clone(),
            RecordId::new(
                "principal",
                surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
            ),
        ),
        (
            alice.tenant_id.record_id(),
            RecordId::new(
                "work_context",
                surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
            ),
            alice.principal_id.record_id(),
        ),
        (
            RecordId::new("tenant", surrealdb::types::Uuid::from(uuid::Uuid::new_v4())),
            context,
            alice.principal_id.record_id(),
        ),
    ] {
        let labels: Option<Labels> =
            db.a.client()
                .query(include_str!("queries/identity_api/labels.surql"))
                .bind(("tenant", tenant))
                .bind(("context", context))
                .bind(("principal", principal))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        assert!(labels.is_none());
    }
}
