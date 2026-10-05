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
                .query(include_str!("queries/identity_api/enabled_user.surql"))
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

async fn current_facts(
    db: &TestDb,
    enterprise: RecordId,
    tenant: RecordId,
    principal: RecordId,
    slug: &str,
) -> (
    Option<veoveo_platform_store::CurrentTenant>,
    Option<veoveo_platform_store::CurrentPrincipal>,
    bool,
) {
    let mut rows =
        db.a.client()
            .query(include_str!("queries/identity_api/current_facts.surql"))
            .bind(("enterprise", enterprise))
            .bind(("tenant", tenant))
            .bind(("principal", principal))
            .bind(("slug", slug.to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
    (
        rows.take(0).unwrap(),
        rows.take(1).unwrap(),
        bool::from_value(rows.take::<surrealdb::types::Value>(2).unwrap()).unwrap(),
    )
}

#[tokio::test]
async fn current_directory_facts_preserve_exact_parent_identity_and_retained_revocation_behavior() {
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let db = TestDb::new().await;
        let alice = person(&db.a, "first", "alice", "Alice").await;
        let bob = person(&db.a, "second", "bob", "Bob").await;
        let enterprise = veoveo_platform_store::deterministic_enterprise_id().record_id();
        let tenant = alice.tenant_id.record_id();
        let principal = alice.principal_id.record_id();
        let (current_tenant, current_principal, retained) = current_facts(
            &db,
            enterprise.clone(),
            tenant.clone(),
            principal.clone(),
            "first",
        )
        .await;
        assert_eq!(current_tenant.unwrap().slug.as_str(), "first");
        let facts = current_principal.unwrap();
        assert_eq!(facts.kind, PrincipalKind::User);
        assert_eq!(facts.issuer.as_str(), "https://identity.test");
        assert_eq!(facts.subject.as_str(), "alice");
        assert!(retained);
        assert!(
            current_facts(
                &db,
                enterprise.clone(),
                tenant.clone(),
                bob.principal_id.record_id(),
                "first"
            )
            .await
            .1
            .is_none()
        );
        assert!(
            !current_facts(
                &db,
                enterprise.clone(),
                tenant.clone(),
                principal.clone(),
                "second"
            )
            .await
            .2
        );
        for record in [enterprise.clone(), tenant.clone()] {
            db.a.client()
                .query(include_str!("queries/identity_api/enabled.surql"))
                .bind(("record", record.clone()))
                .bind(("enabled", false))
                .await
                .unwrap()
                .check()
                .unwrap();
            let (admitted, _, retained) = current_facts(
                &db,
                enterprise.clone(),
                tenant.clone(),
                principal.clone(),
                "first",
            )
            .await;
            assert!(admitted.is_none());
            assert!(
                retained,
                "directory revocation erased retained identity association"
            );
            db.a.client()
                .query(include_str!("queries/identity_api/enabled.surql"))
                .bind(("record", record))
                .bind(("enabled", true))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let absent_parent = RecordId::new(
            "enterprise",
            surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
        );
        db.a.client()
            .query(include_str!("queries/identity_api/set_parent.surql"))
            .bind(("record", tenant.clone()))
            .bind(("parent", absent_parent))
            .await
            .unwrap()
            .check()
            .unwrap();
        let (admitted, _, retained) =
            current_facts(&db, enterprise, tenant, principal, "first").await;
        assert!(admitted.is_none());
        assert!(!retained);
    })
    .await
    .expect("directory fact qualification exceeded three minutes");
}
