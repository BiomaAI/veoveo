//! Real-store admission/projection evidence. Capacity health and image references
//! are fixture inputs; these cases execute no provider work.
#[path = "../../../platform/computers/tests/support/mod.rs"]
#[allow(dead_code)] // This projection fixture does not use the domain's raw fingerprint.
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use uuid::Uuid;
use veoveo_computers::{ComputerError, ComputersStore, api::*};
use veoveo_computers_mcp::{Application, ApplicationError, CapacityHealth, Templates};
use veoveo_mcp_contract::{PolicyEffect, WorkContextMembershipLevel};
use veoveo_task_runtime::TaskRuntime;

#[path = "support/application.rs"]
mod app_support;
use app_support::{application, control, identities};
#[tokio::test]
async fn concurrent_create_and_default_rotation_retain_one_original_operation_and_template() {
    let db = support::TestDb::new().await;
    identities(&db).await;
    let (app, _) = application(&db, false).await;
    let owner = support::owner("alice");
    let request = CreateInput {
        computer_id: None,
        request_id: Uuid::now_v7(),
    };
    let (a, b) = tokio::join!(
        app.create(support::authenticated(&owner), request.clone()),
        app.create(support::authenticated(&owner), request.clone())
    );
    let a = a.unwrap();
    assert_eq!(a.operation_id, b.unwrap().operation_id);
    let snapshot = app
        .snapshot(&support::authenticated(&owner), None)
        .await
        .unwrap();
    assert_eq!(snapshot.computers.len(), 1);
    assert!(snapshot.can_create);
    assert!(snapshot.computers[0].busy);
    assert_eq!(snapshot.computers[0].active_task_id, Some(a.operation_id));
    let (updated, _) = application(&db, true).await;
    let retry = updated
        .create(support::authenticated(&owner), request)
        .await
        .unwrap();
    assert_eq!(retry.operation_id, a.operation_id);
    assert_eq!(retry.template_fingerprint, a.template_fingerprint);
    let fresh = updated
        .create(
            support::authenticated(&owner),
            CreateInput {
                computer_id: None,
                request_id: Uuid::now_v7(),
            },
        )
        .await
        .unwrap();
    assert_ne!(fresh.template_fingerprint, a.template_fingerprint);
    let snapshot = updated
        .snapshot(&support::authenticated(&owner), None)
        .await
        .unwrap();
    assert_eq!(snapshot.availability, CapacityAvailability::Exhausted);
    assert!(!snapshot.can_create);
    assert!(matches!(
        updated
            .create(
                support::authenticated(&owner),
                CreateInput {
                    computer_id: None,
                    request_id: Uuid::now_v7()
                }
            )
            .await,
        Err(ApplicationError::Domain(ComputerError::CapacityFull))
    ));
    assert!(matches!(
        app.computer(
            &support::authenticated(&support::owner("bob")),
            a.computer_id
        )
        .await,
        Err(ApplicationError::Domain(ComputerError::NotFound))
    ));
    assert!(
        app.snapshot(&support::authenticated(&support::owner("bob")), None)
            .await
            .unwrap()
            .computers
            .is_empty()
    );
    let bytes = serde_json::to_string(&snapshot).unwrap();
    assert!(!bytes.contains("provider_resource"));
    assert!(!bytes.contains(&a.template_fingerprint));
}

#[tokio::test]
async fn current_policy_and_membership_control_flags_and_admission_without_reserving() {
    let db = support::TestDb::new().await;
    identities(&db).await;
    let (app, _) = application(&db, false).await;
    let owner = support::owner("alice");
    for viewer in [false, true] {
        let mut changed = control();
        if viewer {
            changed.work_contexts[0].memberships[0].level = WorkContextMembershipLevel::Viewer;
        } else {
            changed.policies[0].rules[0].effect = PolicyEffect::Deny;
        }
        support::policy::install(&db.b, changed).await;
        let snapshot = app
            .snapshot(&support::authenticated(&owner), None)
            .await
            .unwrap();
        assert!(!snapshot.can_create);
        assert!(snapshot.computers.is_empty());
        assert!(matches!(
            app.create(
                support::authenticated(&owner),
                CreateInput {
                    computer_id: None,
                    request_id: Uuid::now_v7()
                }
            )
            .await,
            Err(ApplicationError::Domain(ComputerError::Forbidden))
        ));
    }
    support::policy::install(&db.b, control()).await;
    assert!(
        app.snapshot(&support::authenticated(&owner), None)
            .await
            .unwrap()
            .can_create
    );
    let source = veoveo_platform_store::deterministic_principal_id("test", &owner.principal_key)
        .unwrap()
        .record_id();
    db.b.client()
        .query("UPDATE ONLY $source SET enabled = false;")
        .bind(("source", source))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        app.snapshot(&support::authenticated(&owner), None).await,
        Err(ApplicationError::Domain(ComputerError::Forbidden))
    ));
}

#[tokio::test]
async fn unconfigured_and_stale_capacity_are_visible_without_claiming_admission() {
    let db = support::TestDb::new().await;
    identities(&db).await;
    let store = ComputersStore::new(db.a.clone(), Uuid::from_u128(100)).unwrap();
    let (_, receiver) = watch::channel(CapacityHealth {
        availability: CapacityAvailability::SetupRequired,
        observed_at: Instant::now(),
    });
    let app = Application::new(
        store,
        TaskRuntime::new(db.a.clone(), "computers", "unconfigured"),
        Templates::new(vec![], None).unwrap(),
        receiver,
        veoveo_computers_mcp::RuntimeAccess::unavailable(),
    )
    .unwrap();
    let owner = support::owner("alice");
    let snapshot = app
        .snapshot(&support::authenticated(&owner), None)
        .await
        .unwrap();
    assert_eq!(snapshot.availability, CapacityAvailability::SetupRequired);
    assert!(snapshot.template.is_none() && snapshot.limits.is_none() && !snapshot.can_create);
    assert!(matches!(
        app.create(
            support::authenticated(&owner),
            CreateInput {
                computer_id: None,
                request_id: Uuid::now_v7()
            }
        )
        .await,
        Err(ApplicationError::SetupRequired)
    ));
    let (configured, health) = application(&db, false).await;
    health
        .send(CapacityHealth {
            availability: CapacityAvailability::Available,
            observed_at: Instant::now() - Duration::from_secs(16),
        })
        .unwrap();
    let snapshot = configured
        .snapshot(&support::authenticated(&owner), None)
        .await
        .unwrap();
    assert_eq!(
        snapshot.availability,
        CapacityAvailability::ComputeUnavailable
    );
    assert!(!snapshot.can_create);
    assert!(matches!(
        configured
            .create(
                support::authenticated(&owner),
                CreateInput {
                    computer_id: None,
                    request_id: Uuid::now_v7()
                }
            )
            .await,
        Err(ApplicationError::Unavailable)
    ));
}

#[tokio::test]
async fn an_interrupted_reservation_can_be_provisioned_from_the_visible_collection() {
    let db = support::TestDb::new().await;
    identities(&db).await;
    let owner = support::owner("alice");
    let store = ComputersStore::new(db.a.clone(), Uuid::from_u128(100)).unwrap();
    let (_initial, _health) = application(&db, false).await;
    let original = template::retained_template(format!(
        "fixture.invalid/computer@sha256:{}",
        "f".repeat(64)
    ));
    let reserved = store
        .reserve(
            &owner,
            &veoveo_computers::Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development-retained".into(),
                template_fingerprint: original.fingerprint(),
            },
        )
        .await
        .unwrap();
    // A different replica and default observe the original admitted reservation.
    let (resumed, health) = app_support::application_on(db.b.clone(), true).await;
    let view = resumed
        .computer(&support::authenticated(&owner), reserved.computer_id)
        .await
        .unwrap();
    assert!(view.can_create);
    let input = CreateInput {
        computer_id: Some(view.computer_id),
        request_id: Uuid::now_v7(),
    };
    assert!(matches!(
        resumed
            .create(
                support::authenticated(&support::owner("bob")),
                input.clone()
            )
            .await,
        Err(ApplicationError::Domain(ComputerError::NotFound))
    ));
    let first = resumed
        .create(support::authenticated(&owner), input.clone())
        .await
        .unwrap();
    health
        .send(CapacityHealth {
            availability: CapacityAvailability::ComputeUnavailable,
            observed_at: Instant::now(),
        })
        .unwrap();
    let retry = resumed
        .create(support::authenticated(&owner), input)
        .await
        .unwrap();
    assert_eq!(first.operation_id, retry.operation_id);
    assert_eq!(first.computer_id, reserved.computer_id);
    assert_eq!(first.template_fingerprint, original.fingerprint());
    assert_eq!(
        resumed
            .snapshot(&support::authenticated(&owner), None)
            .await
            .unwrap()
            .computers
            .len(),
        1
    );
}
