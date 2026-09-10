mod support;
use support::*;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_computers::{CapacityPolicy, ComputerError, ComputersStore, Reservation};

fn request() -> Reservation {
    Reservation {
        request_id: Uuid::now_v7(),
        template_id: "development".into(),
        template_fingerprint: FINGERPRINT.into(),
    }
}
async fn store(db: veoveo_platform_store::PlatformStore, limit: u32) -> ComputersStore {
    let store = ComputersStore::new(db, Uuid::from_u128(1)).unwrap();
    store
        .install_capacity(
            None,
            CapacityPolicy {
                per_owner: limit,
                per_tenant: 10,
                provider: 10,
            },
        )
        .await
        .unwrap();
    store
}

#[tokio::test]
async fn racing_same_request_reserves_once_and_changed_input_is_rejected() {
    let db = TestDb::new().await;
    let a = store(db.a.clone(), 2).await;
    let b = store(db.b.clone(), 2).await;
    let owner = owner("alice");
    let input = request();
    let requests = (0..8).map(|i| {
        let store = if i % 2 == 0 { &a } else { &b };
        store.reserve(&owner, &input)
    });
    let results = futures::future::join_all(requests).await;
    let id = results[0].as_ref().unwrap().computer_id;
    for result in results {
        assert_eq!(result.unwrap().computer_id, id);
    }
    assert_eq!(a.list(&owner, None, 100).await.unwrap().computers.len(), 1);
    let reduced = ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
    let prior = reduced.capacity().await.unwrap();
    let closed = CapacityPolicy {
        per_owner: 0,
        ..prior
    };
    reduced.install_capacity(Some(prior), closed).await.unwrap();
    assert_eq!(a.capacity().await.unwrap(), closed);
    assert!(matches!(
        a.reserve(&owner, &request()).await,
        Err(ComputerError::CapacityFull)
    ));
    assert_eq!(
        a.install_capacity(None, prior).await,
        Err(ComputerError::PolicyConflict)
    );
    assert_eq!(
        reduced.reserve(&owner, &input).await.unwrap().computer_id,
        id
    );
    assert!(matches!(
        reduced.reserve(&owner, &request()).await,
        Err(ComputerError::CapacityFull)
    ));
    let mut changed = input.clone();
    changed.template_fingerprint = "b".repeat(64);
    assert!(matches!(
        a.reserve(&owner, &changed).await,
        Err(ComputerError::RequestConflict)
    ));
    // An exact request creates one outbox entry and consumes one retained slot.
    let mut response = db.a.client().query("SELECT * FROM outbox_event WHERE aggregate_type = 'computer'; SELECT * FROM computer_usage;").await.unwrap().check().unwrap();
    let events: Vec<surrealdb::types::Value> = response.take(0).unwrap();
    assert_eq!(events.len(), 1);
    let usage: Vec<surrealdb::types::Value> = response.take(1).unwrap();
    assert_eq!(usage.len(), 3);
    for row in usage {
        assert_eq!(row.get("retained"), &1_i64.into_value());
    }
}

#[tokio::test]
async fn collections_support_services_and_contexts_without_multiplying_owner_quota() {
    let db = TestDb::new().await;
    let a = store(db.a.clone(), 3).await;
    let alice = owner("alice");
    let bob = owner("bob");
    let mut service = owner("automation");
    service.principal_kind = veoveo_task_runtime::PrincipalKind::Service;
    service.authority.provenance = veoveo_mcp_contract::InvocationProvenance::Automated;
    let first = a.reserve(&alice, &request()).await.unwrap();
    let second = a.reserve(&alice, &request()).await.unwrap();
    let page = a.list(&alice, None, 1).await.unwrap();
    assert_eq!(page.computers.len(), 1);
    assert_eq!(page.computers[0].computer_id, first.computer_id);
    let page = a.list(&alice, page.next_cursor, 1).await.unwrap();
    assert_eq!(page.computers[0].computer_id, second.computer_id);
    assert!(page.next_cursor.is_none());
    assert!(matches!(
        a.get(&bob, first.computer_id).await,
        Err(ComputerError::NotFound)
    ));
    assert!(a.list(&bob, None, 10).await.unwrap().computers.is_empty());
    let agent = a.reserve(&service, &request()).await.unwrap();
    assert_eq!(
        agent.owner.principal_kind,
        veoveo_task_runtime::PrincipalKind::Service
    );
    let mut other_context = alice.clone();
    other_context.authority.work_context =
        veoveo_mcp_contract::WorkContextId::new("another").unwrap();
    assert!(
        a.list(&other_context, None, 10)
            .await
            .unwrap()
            .computers
            .is_empty()
    );
    assert!(matches!(
        a.get(&other_context, first.computer_id).await,
        Err(ComputerError::NotFound)
    ));
    a.reserve(&other_context, &request()).await.unwrap();
    assert!(matches!(
        a.reserve(&alice, &request()).await,
        Err(ComputerError::CapacityFull)
    ));
    let mut other_profile = alice.clone();
    other_profile.profile = "different".into();
    assert!(matches!(
        a.reserve(&other_profile, &request()).await,
        Err(ComputerError::CapacityFull)
    ));
    let mut invalid = alice.clone();
    invalid.tenant_key = Some("another-tenant".into());
    assert!(matches!(
        a.reserve(&invalid, &request()).await,
        Err(ComputerError::InvalidInput)
    ));
    let mut nil_request = request();
    nil_request.request_id = Uuid::nil();
    assert!(matches!(
        a.reserve(&alice, &nil_request).await,
        Err(ComputerError::InvalidInput)
    ));
    let mut classified = owner("classified");
    classified.data_labels.insert("private-data".into());
    let private = a.reserve(&classified, &request()).await.unwrap();
    classified.data_labels.clear();
    assert!(matches!(
        a.get(&classified, private.computer_id).await,
        Err(ComputerError::NotFound)
    ));
    assert!(matches!(
        a.list(&classified, None, 100).await,
        Err(ComputerError::NotFound)
    ));
}

#[tokio::test]
async fn concurrent_distinct_admissions_enforce_each_shared_capacity_boundary() {
    let db = TestDb::new().await;
    let capacity = CapacityPolicy {
        per_owner: 2,
        per_tenant: 3,
        provider: 4,
    };
    let a = ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
    let b = ComputersStore::new(db.b.clone(), Uuid::from_u128(1)).unwrap();
    a.install_capacity(None, capacity).await.unwrap();
    let alice = owner("alice");
    let inputs: Vec<_> = (0..6).map(|_| request()).collect();
    let results = futures::future::join_all(inputs.iter().enumerate().map(|(i, r)| {
        if i % 2 == 0 {
            a.reserve(&alice, r)
        } else {
            b.reserve(&alice, r)
        }
    }))
    .await;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 2);
    for error in results.into_iter().filter_map(Result::err) {
        assert_eq!(error, ComputerError::CapacityFull);
    }
    a.reserve(&owner("bob"), &request()).await.unwrap();
    assert!(matches!(
        a.reserve(&owner("charlie"), &request()).await,
        Err(ComputerError::CapacityFull)
    ));
    let mut other_tenant = owner("dana");
    other_tenant.tenant_key = Some("tenant-two".into());
    other_tenant.authority.tenant = veoveo_mcp_contract::TenantId::new("tenant-two").unwrap();
    b.reserve(&other_tenant, &request()).await.unwrap();
    assert!(matches!(
        a.reserve(&other_tenant, &request()).await,
        Err(ComputerError::CapacityFull)
    ));
    assert_eq!(a.list(&alice, None, 100).await.unwrap().computers.len(), 2);
}
