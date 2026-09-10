//! Additional grant checks share the actual ledger fixture and its authority tests.
// Kept as an include in session_grants.rs, not an independent duplicate launcher.
use super::*;

#[tokio::test]
async fn attachment_requires_its_own_resource_policy_and_current_contributor_membership() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, _, computer) = ready(&db, &actor).await;
    for change in ["attach", "read", "membership", "labels"] {
        let mut policy = control();
        match change {
            "attach" => {
                policy.policies[0].rules.remove(2);
            }
            "read" => {
                policy.policies[0].rules.remove(1);
            }
            "membership" => {
                policy.work_contexts[0].memberships[0].level = WorkContextMembershipLevel::Viewer
            }
            _ => {
                policy.policies[0].rules[2]
                    .required_data_labels
                    .insert(DataLabelId::new("cui").unwrap());
            }
        }
        support::policy::install(&db.b, policy).await;
        assert!(
            a.issue_browser_grant(&actor, computer).await.is_err(),
            "attachment admitted after {change} removal"
        );
    }
    let mut policy = control();
    policy.policies[0].rules[0].effect = PolicyEffect::Deny;
    support::policy::install(&db.b, policy).await;
    // Lifecycle permission is independently controlled.
    let ticket = a.issue_browser_grant(&actor, computer).await.unwrap();
    let handle = a.redeem_browser_grant(&actor, &ticket.token).await.unwrap();
    assert!(a.renew_browser_grant(&handle, false).await.is_ok());
    let mut invalid = control();
    invalid.servers[0].slug = ServerSlug::new("foreign").unwrap();
    invalid.servers[0].uri_scheme = ResourceScheme::new("foreign").unwrap();
    // The existing rules still name computers, so use the exact new scope too.
    invalid.profiles[0].servers[0].server = ServerSlug::new("foreign").unwrap();
    invalid.profiles[0].servers[0].resources = Exposure::All;
    for rule in &mut invalid.policies[0].rules {
        rule.servers = [ServerSlug::new("foreign").unwrap()].into_iter().collect();
    }
    assert!(matches!(
        invalid.validate(),
        Err(
            GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
                action: GatewayAction::ComputerAttach,
                ..
            }
        )
    ));
}

#[tokio::test]
async fn expired_ticket_grant_and_family_cannot_be_revived_by_activity() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    for field in ["ticket", "absolute", "idle", "family"] {
        let identity = browser(&db, "alice").await;
        let actor = ComputerActor::from_verified(&identity).unwrap();
        let ticket = a.issue_browser_grant(&actor, computer).await.unwrap();
        if field == "ticket" {
            let id: Uuid = ticket
                .token
                .expose_secret()
                .split_once('.')
                .unwrap()
                .0
                .parse()
                .unwrap();
            db.b.client()
                .query("UPDATE ONLY $grant SET ticket_expires_at = time::now() - 1s;")
                .bind(("grant", grant_record(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(b.redeem_browser_grant(&actor, &ticket.token).await.is_err());
            continue;
        }
        let handle = a.redeem_browser_grant(&actor, &ticket.token).await.unwrap();
        let statement = match field {
            "absolute" => "UPDATE ONLY $grant SET expires_at = time::now() - 1s;",
            "idle" => "UPDATE ONLY $grant SET idle_expires_at = time::now() - 1s;",
            _ => "UPDATE ONLY $family SET expires_at = time::now() - 1s;",
        };
        let family = gateway_refresh_family_record_id(
            identity
                .request_context
                .as_ref()
                .unwrap()
                .access_token
                .session_family
                .as_ref()
                .unwrap()
                .as_str()
                .parse()
                .unwrap(),
        );
        db.b.client()
            .query(statement)
            .bind(("grant", grant_record(handle.grant_id())))
            .bind(("family", family))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            b.renew_browser_grant(&handle, true).await.is_err(),
            "revived {field}"
        );
        a.close_browser_grant(&handle).await.unwrap();
    }
}

#[tokio::test]
async fn passive_renewal_preserves_idle_expiry_and_activity_cannot_slide_absolute_expiry() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let short = SessionGrantPolicy {
        max_grants: 2,
        absolute_seconds: 3,
        idle_seconds: 2,
    };
    a.install_session_grant_policy(Some(LIMITS), short)
        .await
        .unwrap();
    let ticket = a.issue_browser_grant(&actor, computer).await.unwrap();
    let handle = a.redeem_browser_grant(&actor, &ticket.token).await.unwrap();
    let first = a.renew_browser_grant(&handle, false).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let passive = b.renew_browser_grant(&handle, false).await.unwrap();
    assert!(close_deadlines(passive.valid_until(), first.valid_until()));
    let active = b.renew_browser_grant(&handle, true).await.unwrap();
    assert!(active.valid_until() > passive.valid_until() + Duration::from_millis(500));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let again = a.renew_browser_grant(&handle, true).await.unwrap();
    assert!(close_deadlines(again.valid_until(), active.valid_until()));
    tokio::time::sleep_until(
        tokio::time::Instant::from_std(again.valid_until()) + Duration::from_millis(30),
    )
    .await;
    assert!(a.renew_browser_grant(&handle, true).await.is_err());
}

fn close_deadlines(left: std::time::Instant, right: std::time::Instant) -> bool {
    left.saturating_duration_since(right)
        .max(right.saturating_duration_since(left))
        < Duration::from_millis(150)
}
