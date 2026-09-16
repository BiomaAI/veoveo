//! Resource identity survives client changes; current policy and grant sessions do not merge.
mod support;
use veoveo_computers::{ComputerActor, ComputerError, api::*};
use veoveo_mcp_contract::{DataLabelId, GatewayAction, WorkContextId};

#[tokio::test]
async fn retained_collection_uses_current_profile_policy_and_indexed_owner_identity() {
    let db = support::TestDb::new().await;
    let original =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let (store, replica, computer_id) = support::interactive::ready(&db, &original).await;
    let workspace = support::clients::workspace(&db, "alice").await;
    let control = support::clients::control(support::interactive::control());
    support::policy::install(&db.a, control.clone()).await;
    let before = store.get(original.owner(), computer_id).await.unwrap();
    assert_eq!(
        replica.get(workspace.owner(), computer_id).await.unwrap(),
        before
    );
    assert_eq!(
        replica
            .list(workspace.owner(), None, 10)
            .await
            .unwrap()
            .computers,
        std::slice::from_ref(&before)
    );
    assert_eq!(
        replica
            .complete_ids(workspace.owner(), &computer_id.to_string())
            .await
            .unwrap(),
        (vec![computer_id.to_string()], false)
    );
    let current = replica.control_authority(&workspace).await.unwrap();
    let page = replica
        .read_accessible_computers(&workspace, &current, None, 10)
        .await
        .unwrap();
    assert_eq!(page.computers.len(), 1);
    assert_eq!(page.computers[0].mode(), ComputerAccessMode::Owner);
    assert_eq!(page.computers[0].computer().unwrap(), &before);
    for mut other in [support::owner("bob"), workspace.owner().clone()] {
        if other.principal_key == workspace.owner().principal_key {
            other.authority.work_context = WorkContextId::new("other-context").unwrap();
        }
        assert!(replica.get(&other, computer_id).await.is_err());
        assert!(
            replica
                .list(&other, None, 10)
                .await
                .unwrap()
                .computers
                .is_empty()
        );
    }
    let query = include_str!("../queries/list.surql")
        .trim_end()
        .trim_end_matches(';')
        .to_owned()
        + " EXPLAIN;";
    let mut plan =
        db.a.client()
            .query(query)
            .bind(("owner_tenant", workspace.owner().tenant_key().to_owned()))
            .bind(("owner_principal", workspace.owner().principal_key.clone()))
            .bind((
                "owner_context",
                workspace.owner().authority.work_context.to_string(),
            ))
            .bind(("after", Option::<uuid::Uuid>::None))
            .bind(("limit", 11_i64))
            .await
            .unwrap()
            .check()
            .unwrap();
    let plan: surrealdb::types::Value = plan.take(0).unwrap();
    let plan = format!("{plan:?}");
    assert!(plan.contains("computer_resource_owner"), "{plan}");
    // Only the current Workspace policy is removed; the Console remains authorized.
    let mut denied = control;
    for policy in &mut denied.policies {
        for rule in &mut policy.rules {
            if rule.actions.contains(&GatewayAction::ResourcesRead) {
                rule.profiles
                    .retain(|profile| profile.as_str() != "workspace");
            }
        }
    }
    support::policy::install(&db.a, denied).await;
    let current = replica.control_authority(&workspace).await.unwrap();
    assert!(
        replica
            .read_accessible_computers(&workspace, &current, None, 10)
            .await
            .is_err()
    );
    let current = replica.control_authority(&original).await.unwrap();
    assert!(
        replica
            .read_computer_access(&original, &current, computer_id)
            .await
            .is_ok()
    );
    assert_eq!(
        store.get(original.owner(), computer_id).await.unwrap(),
        before
    );
    // Retained labels remain a floor after a client change.
    let mut retained = before.owner.clone();
    retained.data_labels.insert("cui".into());
    retained
        .authority
        .output_policy
        .data_labels
        .insert(DataLabelId::new("cui").unwrap());
    db.a.client()
        .query("UPDATE ONLY $computer SET owner_context = $owner;")
        .bind((
            "computer",
            veoveo_platform_store::RecordId::new(
                "computer",
                surrealdb::types::Uuid::from(computer_id),
            ),
        ))
        .bind((
            "owner",
            serde_json::from_value::<veoveo_platform_store::OpenObject>(
                serde_json::to_value(retained).unwrap(),
            )
            .unwrap(),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(replica.get(workspace.owner(), computer_id).await.is_err());
}

#[tokio::test]
async fn browser_profiles_share_capacity_and_revocation_without_sharing_sessions() {
    let db = support::TestDb::new().await;
    let original =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let (store, replica, computer) = support::interactive::ready(&db, &original).await;
    let workspace = support::clients::workspace(&db, "alice").await;
    let stranger = support::clients::workspace(&db, "bob").await;
    support::policy::install(
        &db.a,
        support::clients::control(support::interactive::control()),
    )
    .await;
    let before = store.get(original.owner(), computer).await.unwrap();
    let console_ticket = store
        .issue_browser_grant(&original, computer)
        .await
        .unwrap();
    let workspace_ticket = replica
        .issue_browser_grant(&workspace, computer)
        .await
        .unwrap();
    assert!(
        replica
            .redeem_browser_grant(&original, &workspace_ticket.token)
            .await
            .is_err()
    );
    assert!(
        replica
            .redeem_browser_grant(&stranger, &workspace_ticket.token)
            .await
            .is_err()
    );
    let console_handle = store
        .redeem_browser_grant(&original, &console_ticket.token)
        .await
        .unwrap();
    let workspace_handle = replica
        .redeem_browser_grant(&workspace, &workspace_ticket.token)
        .await
        .unwrap();
    assert!(
        store
            .renew_browser_grant(&console_handle, false)
            .await
            .is_ok()
    );
    assert!(
        replica
            .renew_browser_grant(&workspace_handle, true)
            .await
            .is_ok()
    );
    for actor in [&original, &workspace] {
        assert!(matches!(
            store.issue_browser_grant(actor, computer).await,
            Err(ComputerError::AccessLimit)
        ));
        let inventory = replica.access_grants(actor, computer).await.unwrap();
        assert_eq!(inventory.grants.len(), 2);
        assert_eq!(
            inventory
                .grants
                .iter()
                .filter(|grant| grant.current_session)
                .count(),
            1
        );
    }
    assert!(
        replica
            .revoke_browser_grant(&stranger, computer, console_handle.grant_id())
            .await
            .is_err()
    );
    replica
        .revoke_browser_grant(&workspace, computer, console_handle.grant_id())
        .await
        .unwrap();
    assert!(
        store
            .renew_browser_grant(&console_handle, false)
            .await
            .is_err()
    );
    assert!(
        store
            .renew_browser_grant(&workspace_handle, false)
            .await
            .is_ok()
    );
    store
        .revoke_browser_grant(&original, computer, workspace_handle.grant_id())
        .await
        .unwrap();
    assert!(
        replica
            .renew_browser_grant(&workspace_handle, false)
            .await
            .is_err()
    );
    assert!(
        store
            .access_grants(&workspace, computer)
            .await
            .unwrap()
            .grants
            .is_empty()
    );
    assert_eq!(store.get(original.owner(), computer).await.unwrap(), before);
}
