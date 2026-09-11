mod support;
use uuid::Uuid;
use veoveo_computers::{ComputerActor, ComputerError, ComputersStore, Reservation, api::*};

async fn grant(
    store: &ComputersStore,
    owner: &ComputerActor,
    computer: Uuid,
    permissions: &[AutomationPermission],
) -> AutomationGrantView {
    let mut input = support::automation::input(computer);
    input.permissions = permissions.iter().copied().collect();
    if !input.permissions.contains(&AutomationPermission::Execute) {
        input.execution_limits = None;
    }
    store.issue_automation_grant(owner, &input).await.unwrap()
}

#[tokio::test]
async fn read_and_action_grants_compose_without_exposing_owner_management() {
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    let read = grant(
        &store,
        &owner,
        computer,
        &[AutomationPermission::Read, AutomationPermission::Stop],
    )
    .await;
    let execute = grant(&store, &owner, computer, &[AutomationPermission::Execute]).await;
    let control = store.control_authority(&agent).await.unwrap();
    let access = store
        .read_computer_access(&agent, &control, computer)
        .await
        .unwrap();
    assert_eq!(access.mode(), ComputerAccessMode::Granted);
    assert_eq!(access.computer().unwrap().owner, *owner.owner());
    let scopes = access.grants().unwrap();
    assert_eq!(scopes.len(), 2);
    assert_eq!(scopes[0].grant_id, read.grant_id);
    assert_eq!(scopes[1].grant_id, execute.grant_id);
    assert!(
        !access.allows_file_transfer().unwrap(),
        "command policy does not imply file-tool policy"
    );
    assert!(
        store
            .list_automation_grants(&agent, computer)
            .await
            .is_err()
    );
    assert!(store.get(agent.owner(), computer).await.is_err());
    assert_eq!(
        store
            .complete_accessible_ids(&agent, &control, &computer.to_string())
            .await
            .unwrap(),
        (vec![computer.to_string()], false)
    );
    assert!(
        store
            .complete_accessible_ids(&agent, &control, "zz")
            .await
            .is_err()
    );
    assert!(
        store
            .automation_change_recipient(&agent, &control, computer, read.grant_id)
            .await
            .unwrap()
    );
    assert!(
        !store
            .automation_change_recipient(&agent, &control, computer, execute.grant_id)
            .await
            .unwrap()
    );
    let owner_control = store.control_authority(&owner).await.unwrap();
    let own = store
        .read_computer_access(&owner, &owner_control, computer)
        .await
        .unwrap();
    assert_eq!(own.mode(), ComputerAccessMode::Owner);
    assert!(own.grants().unwrap().is_empty());
    assert!(
        store
            .read_computer_access(&agent, &owner_control, computer)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_mutation_grant_without_read_cannot_discover_private_computer_state() {
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    grant(&store, &owner, computer, &[AutomationPermission::Stop]).await;
    let control = store.control_authority(&agent).await.unwrap();
    assert!(matches!(
        store.read_computer_access(&agent, &control, computer).await,
        Err(ComputerError::NotFound)
    ));
    assert!(
        store
            .read_accessible_computers(&agent, &control, None, 100)
            .await
            .unwrap()
            .computers
            .is_empty()
    );
    let read = grant(&store, &owner, computer, &[AutomationPermission::Read]).await;
    assert!(
        store
            .read_computer_access(&agent, &control, computer)
            .await
            .is_ok()
    );
    store
        .revoke_automation_grant(
            &owner,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: read.grant_id,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        store.read_computer_access(&agent, &control, computer).await,
        Err(ComputerError::NotFound)
    ));
    assert!(
        store
            .read_accessible_computers(&agent, &control, None, 100)
            .await
            .unwrap()
            .computers
            .is_empty()
    );
}

#[tokio::test]
async fn owned_and_granted_computers_share_bounded_ordered_pages_without_duplicates() {
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, first) = support::automation::setup(&db).await;
    grant(&store, &owner, first, &[AutomationPermission::Read]).await;
    grant(&store, &owner, first, &[AutomationPermission::Read]).await;
    let second = store
        .reserve(
            owner.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    grant(
        &store,
        &owner,
        second.computer_id,
        &[AutomationPermission::Read],
    )
    .await;
    let third = store
        .reserve(
            agent.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    let control = store.control_authority(&agent).await.unwrap();
    let mut after = None;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let page = store
            .read_accessible_computers(&agent, &control, after, 1)
            .await
            .unwrap();
        assert_eq!(page.computers.len(), 1);
        ids.push(page.computers[0].computer().unwrap().computer_id);
        after = page.next_cursor;
    }
    assert_eq!(ids, [first, second.computer_id, third.computer_id]);
    assert_eq!(after, None);
    assert!(
        store
            .read_accessible_computers(&agent, &control, None, 0)
            .await
            .is_err()
    );
    assert!(
        store
            .read_accessible_computers(&agent, &control, None, 101)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn disabled_owner_and_insufficient_retained_clearance_remove_granted_reads() {
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    grant(&store, &owner, computer, &[AutomationPermission::Read]).await;
    let control = store.control_authority(&agent).await.unwrap();
    let principal = veoveo_platform_store::deterministic_principal_id(
        owner.owner().tenant_key(),
        &owner.owner().principal_key,
    )
    .unwrap()
    .record_id();
    db.a.client()
        .query("UPDATE ONLY $principal SET enabled = false;")
        .bind(("principal", principal.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        store
            .read_computer_access(&agent, &control, computer)
            .await
            .is_err()
    );
    assert!(
        store
            .read_accessible_computers(&agent, &control, None, 100)
            .await
            .unwrap()
            .computers
            .is_empty()
    );
    db.a.client().query("UPDATE ONLY $principal SET enabled = true; UPDATE computer SET owner_context.data_labels = ['pii'];").bind(("principal", principal)).await.unwrap().check().unwrap();
    assert!(
        store
            .read_computer_access(&agent, &control, computer)
            .await
            .is_err()
    );
}
