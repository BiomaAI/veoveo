//! SQL visibility precedes protected decoding and the public page limit.
mod support;
use std::time::Duration;
use veoveo_computers::{ComputerActor, ComputerError, ComputersStore, Reservation, api::*};
use veoveo_platform_store::{OpenObject, RecordId};
use veoveo_task_runtime::TaskOwner;

fn record(id: ComputerId) -> RecordId {
    RecordId::new("computer", surrealdb::types::Uuid::from(id.into_uuid()))
}
async fn reserve(store: &ComputersStore, owner: &TaskOwner) -> ComputerId {
    store
        .reserve(
            &crate::support::authenticated(owner),
            &Reservation {
                request_id: veoveo_computers::api::RequestId::new(),
                template_id: "development".parse().unwrap(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap()
        .computer_id
}
async fn replace_owner(db: &support::TestDb, id: ComputerId, value: serde_json::Value) {
    db.a.client()
        .query("UPDATE ONLY $computer SET owner_context = $owner;")
        .bind(("computer", record(id)))
        .bind((
            "owner",
            serde_json::from_value::<OpenObject>(value).unwrap(),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
}
async fn read_grant(
    store: &ComputersStore,
    owner: &ComputerActor,
    id: ComputerId,
) -> AutomationGrantView {
    let mut input = support::automation::input(id);
    input.permissions = [AutomationPermission::Read].into();
    input.execution_limits = None;
    store.issue_automation_grant(owner, &input).await.unwrap()
}

#[tokio::test]
async fn owned_reads_filter_complete_identity_and_clearance_before_decoding_and_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let actor =
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
        let (store, _, first) = support::interactive::ready(&db, &actor).await;
        let second = reserve(&store, actor.owner()).await;
        let mut ids = [first, second];
        ids.sort();
        let [hidden, visible] = ids;
        let base = serde_json::to_value(actor.owner()).unwrap();
        let changes = [
            (vec!["principal_key"], serde_json::json!("foreign")),
            (vec!["principal_kind"], serde_json::json!("service")),
            (vec!["issuer"], serde_json::json!("https://foreign.invalid")),
            (vec!["subject"], serde_json::json!("foreign")),
            (vec!["tenant_key"], serde_json::json!("foreign")),
            (vec!["authority", "tenant"], serde_json::json!("foreign")),
            (
                vec!["authority", "work_context"],
                serde_json::json!("foreign"),
            ),
            (vec!["data_labels"], serde_json::json!(["private"])),
            (
                vec!["authority", "output_policy", "data_labels"],
                serde_json::json!(["private"]),
            ),
            (
                vec!["authority", "output_policy", "classification"],
                serde_json::json!("private"),
            ),
        ];
        for (path, value) in changes {
            let mut denied = base.clone();
            let mut field = &mut denied;
            for key in &path {
                field = &mut field[*key];
            }
            *field = value;
            // This field cannot decode as TaskOwner. SQL must exclude the row first.
            denied["profile"] = serde_json::json!(42);
            replace_owner(&db, hidden, denied).await;
            assert!(
                matches!(
                    store.get(actor.owner(), hidden).await,
                    Err(ComputerError::NotFound)
                ),
                "{path:?}"
            );
            let page = store.list(actor.owner(), None, 1).await.unwrap();
            assert_eq!(
                page.computers
                    .iter()
                    .map(|c| c.computer_id)
                    .collect::<Vec<_>>(),
                [visible],
                "{path:?}"
            );
            assert!(page.next_cursor.is_none(), "{path:?}");
            assert_eq!(
                store.complete_ids(actor.owner(), "").await.unwrap(),
                (vec![visible.to_string()], false),
                "{path:?}"
            );
        }
        let mut corrupt = base;
        corrupt["profile"] = serde_json::json!(42);
        replace_owner(&db, hidden, corrupt).await;
        assert!(matches!(
            store.get(actor.owner(), hidden).await,
            Err(ComputerError::Unavailable)
        ));
        assert!(
            store.list(actor.owner(), None, 1).await.is_err(),
            "an admitted malformed row must fail"
        );
    })
    .await
    .expect("owned SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn granted_pages_skip_denied_keys_without_decoding_private_rows_or_losing_the_next_computer()
{
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, first) = support::automation::setup(&db).await;
        let second = reserve(&store, owner.owner()).await;
        let third = reserve(&store, owner.owner()).await;
        let mut ids = [first, second, third]; ids.sort();
        let [classified, foreign, visible] = ids;
        read_grant(&store, &owner, classified).await;
        let wrong_grant = read_grant(&store, &owner, foreign).await;
        read_grant(&store, &owner, visible).await;
        let mut retained = serde_json::to_value(owner.owner()).unwrap();
        retained["data_labels"] = serde_json::json!(["private"]);
        retained["profile"] = serde_json::json!(42);
        replace_owner(&db, classified, retained).await;
        db.a.client().query("UPDATE ONLY $grant SET grantee_issuer = 'https://foreign.invalid', authority.request_context = 42;")
            .bind(("grant", RecordId::new("computer_automation_grant", surrealdb::types::Uuid::from(wrong_grant.grant_id.into_uuid()))))
            .await.unwrap().check().unwrap();
        let control = store.control_authority(&agent).await.unwrap();
        for hidden in [classified, foreign] {
            assert!(matches!(store.read_computer_access(&agent, &control, hidden).await, Err(ComputerError::NotFound)));
        }
        let page = store.read_accessible_computers(&agent, &control, None, 1).await.unwrap();
        assert_eq!(page.computers.len(), 1);
        assert_eq!(page.computers[0].computer().unwrap().computer_id, visible);
        assert!(page.next_cursor.is_none());
        assert_eq!(store.complete_accessible_ids(&agent, &control, "").await.unwrap(), (vec![visible.to_string()], false));
        assert!(matches!(store.get_automation_grant(&owner, visible, wrong_grant.grant_id).await, Err(ComputerError::NotFound)));
    }).await.expect("granted SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn current_owner_policy_is_resolved_before_decoding_granted_computer_state() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, hidden) = support::automation::setup(&db).await;
        read_grant(&store, &owner, hidden).await;
        let bob =
            ComputerActor::from_verified(&support::browser::identity(&db, "bob").await).unwrap();
        let visible = reserve(&store, bob.owner()).await;
        read_grant(&store, &bob, visible).await;
        let mut policy = support::automation::control();
        let mut deny = policy.policies[0]
            .rules
            .iter()
            .find(|rule| {
                rule.actions
                    .contains(&veoveo_gateway_contract::GatewayAction::ResourcesRead.into())
            })
            .unwrap()
            .clone();
        deny.id = veoveo_mcp_contract::PolicyRuleId::new("deny-alice-read").unwrap();
        deny.effect = veoveo_mcp_contract::PolicyEffect::Deny;
        deny.principal_ids =
            [veoveo_types::PrincipalId::new(owner.owner().principal_key.clone()).unwrap()].into();
        policy.policies[0].rules.push(deny);
        support::policy::install(&db.a, policy).await;
        let mut corrupt = serde_json::to_value(owner.owner()).unwrap();
        corrupt["profile"] = serde_json::json!(42);
        replace_owner(&db, hidden, corrupt).await;
        let control = store.control_authority(&agent).await.unwrap();
        assert!(matches!(
            store.read_computer_access(&agent, &control, hidden).await,
            Err(ComputerError::NotFound)
        ));
        let page = store
            .read_accessible_computers(&agent, &control, None, 1)
            .await
            .unwrap();
        assert_eq!(page.computers.len(), 1);
        assert_eq!(page.computers[0].computer().unwrap().computer_id, visible);
        assert!(page.next_cursor.is_none());
    })
    .await
    .expect("grant policy admission exceeded 90 seconds");
}

#[tokio::test]
async fn policy_replacement_and_logout_invalidate_an_already_resolved_owner_read() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let identity = support::browser::identity(&db, "alice").await;
        let family = identity
            .request_context
            .as_ref()
            .unwrap()
            .access_token
            .session_family
            .as_ref()
            .unwrap();
        let family = veoveo_platform_store::gateway_refresh_family_record_id(
            family.as_str().parse().unwrap(),
        );
        let owner = ComputerActor::from_verified(&identity).unwrap();
        let (store, _, computer) = support::interactive::ready(&db, &owner).await;
        let old = store.control_authority(&owner).await.unwrap();
        let mut policy = support::interactive::control();
        policy.policies[0].rules[0].id =
            veoveo_mcp_contract::PolicyRuleId::new("replacement-action-policy").unwrap();
        support::policy::install(&db.a, policy).await;
        assert!(matches!(
            store.read_computer_access(&owner, &old, computer).await,
            Err(ComputerError::Forbidden)
        ));
        assert!(matches!(
            store.read_accessible_computers(&owner, &old, None, 1).await,
            Err(ComputerError::Forbidden)
        ));
        let fresh = store.control_authority(&owner).await.unwrap();
        assert!(
            store
                .read_computer_access(&owner, &fresh, computer)
                .await
                .is_ok()
        );
        db.a.client()
            .query("UPDATE ONLY $family SET revoked_at = time::now();")
            .bind(("family", family))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store.read_computer_access(&owner, &fresh, computer).await,
            Err(ComputerError::Forbidden)
        ));
        assert!(matches!(
            store
                .read_accessible_computers(&owner, &fresh, None, 1)
                .await,
            Err(ComputerError::Forbidden)
        ));
    })
    .await
    .expect("read authority invalidation exceeded 90 seconds");
}

#[tokio::test]
async fn reduced_grant_lifetime_denies_before_decoding_its_computer() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
        read_grant(&store, &owner, computer).await;
        let mut corrupt = serde_json::to_value(owner.owner()).unwrap();
        corrupt["profile"] = serde_json::json!(42);
        replace_owner(&db, computer, corrupt).await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        store
            .install_automation_grant_policy(
                Some(support::automation::POLICY),
                veoveo_computers::automation_grants::AutomationGrantPolicy {
                    maximum_lifetime_seconds: 1,
                    ..support::automation::POLICY
                },
            )
            .await
            .unwrap();
        let control = store.control_authority(&agent).await.unwrap();
        assert!(matches!(
            store.read_computer_access(&agent, &control, computer).await,
            Err(ComputerError::NotFound)
        ));
        let page = store
            .read_accessible_computers(&agent, &control, None, 1)
            .await
            .unwrap();
        assert!(page.computers.is_empty());
        assert!(page.next_cursor.is_none());
    })
    .await
    .expect("grant lifetime admission exceeded 90 seconds");
}
