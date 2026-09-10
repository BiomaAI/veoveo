//! Real shared ledger with synthetic Ready Computers. Public stock CLI transport
//! and installed identity acceptance are separate qualification boundaries.
mod support;
use chrono::{TimeDelta, Utc};
use std::time::Duration;
use support::{
    browser::identity as browser,
    interactive::{LIMITS, control, ready},
};
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, ComputersStore, cli_grants::*, session_grants::SessionGrantPolicy,
};
use veoveo_mcp_contract::{PolicyEffect, WorkContextMembershipLevel};
use veoveo_platform_store::RecordId;

fn request(code: &str) -> CliPairingRequest {
    CliPairingRequest {
        name: "Development laptop".into(),
        code: code.into(),
        callback_port: 49152,
    }
}
fn record(table: &str, id: Uuid) -> RecordId {
    RecordId::new(table, surrealdb::types::Uuid::from(id))
}
async fn pair(
    a: &ComputersStore,
    b: &ComputersStore,
    actor: &ComputerActor,
    computer: Uuid,
    code: &str,
) -> PairedCliGrant {
    let challenge = a
        .begin_cli_pairing(actor, computer, &request(code))
        .await
        .unwrap();
    b.confirm_cli_pairing(actor, computer, challenge.pairing_id)
        .await
        .unwrap()
}

#[test]
fn stock_pairing_accepts_only_the_qualified_code_and_loopback_port_profile() {
    assert!(request("ABC-2345").validate().is_ok());
    for code in [
        "ABC-1234",
        "ABC-234",
        "abc-2345",
        "ABO-2345",
        "ABC-2345?token=secret",
    ] {
        assert!(request(code).validate().is_err());
    }
    for port in [0, 22, 443, 1023] {
        let mut input = request("ABC-2345");
        input.callback_port = port;
        assert!(input.validate().is_err());
    }
    for name in ["", " laptop", "laptop\n", "x\u{1b}[0m"] {
        let mut input = request("ABC-2345");
        input.name = name.into();
        assert!(input.validate().is_err());
    }
}

#[tokio::test]
async fn pairing_is_one_use_private_and_connection_close_preserves_the_named_grant() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let other_session = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let other_owner = ComputerActor::from_verified(&browser(&db, "bob").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let challenge = a
        .begin_cli_pairing(&actor, computer, &request("ABC-2345"))
        .await
        .unwrap();
    assert!(
        b.confirm_cli_pairing(&other_session, computer, challenge.pairing_id)
            .await
            .is_err()
    );
    assert!(
        b.confirm_cli_pairing(&other_owner, computer, challenge.pairing_id)
            .await
            .is_err()
    );
    assert!(
        b.confirm_cli_pairing(&actor, Uuid::now_v7(), challenge.pairing_id)
            .await
            .is_err()
    );
    let (one, two) = tokio::join!(
        a.confirm_cli_pairing(&actor, computer, challenge.pairing_id),
        b.confirm_cli_pairing(&actor, computer, challenge.pairing_id)
    );
    let paired = match (one, two) {
        (Ok(grant), Err(_)) | (Err(_), Ok(grant)) => grant,
        _ => panic!("exactly one challenge consumer"),
    };
    assert_eq!(paired.callback_port, 49152);
    assert!(
        b.begin_cli_pairing(&actor, computer, &request("ABC-2345"))
            .await
            .is_err()
    );
    let mut stored =
        db.b.client()
            .query("SELECT VALUE credential_hash FROM ONLY $grant;")
            .bind(("grant", record("computer_cli_grant", paired.grant_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
    let hash: Option<String> = stored.take(0).unwrap();
    assert_eq!(hash.as_ref().unwrap().len(), 64);
    assert!(
        !paired
            .credential
            .expose_secret()
            .contains(hash.as_ref().unwrap())
    );
    let foreign = CliGrantCredential::new(format!("vcli1.{}.{}", paired.grant_id, "f".repeat(64)));
    assert!(a.open_cli_connection(computer, &foreign).await.is_err());
    assert!(
        a.open_cli_connection(Uuid::now_v7(), &paired.credential)
            .await
            .is_err()
    );
    let first = a
        .open_cli_connection(computer, &paired.credential)
        .await
        .unwrap();
    let second = b
        .open_cli_connection(computer, &paired.credential)
        .await
        .unwrap();
    b.close_cli_connection(&first).await.unwrap();
    assert!(a.renew_cli_grant(&first, true).await.is_err());
    assert!(a.renew_cli_grant(&second, false).await.is_ok());
    assert!(
        b.open_cli_connection(computer, &paired.credential)
            .await
            .is_ok()
    );
    let inventory = b.cli_access_grants(&other_session, computer).await.unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].name, "Development laptop");
    assert!(!inventory[0].current_session);
    assert!(a.cli_access_grants(&other_owner, computer).await.is_err());
    assert!(
        a.revoke_cli_grant(&other_owner, computer, paired.grant_id)
            .await
            .is_err()
    );
    assert!(
        a.revoke_cli_grant(&actor, Uuid::now_v7(), paired.grant_id)
            .await
            .is_err()
    );
    let mut viewer = control();
    viewer.work_contexts[0].memberships[0].level = WorkContextMembershipLevel::Viewer;
    support::policy::install(&db.b, viewer).await;
    b.revoke_cli_grant(&other_session, computer, paired.grant_id)
        .await
        .unwrap();
    a.revoke_cli_grant(&other_session, computer, paired.grant_id)
        .await
        .unwrap();
    assert!(a.renew_cli_grant(&second, true).await.is_err());
    assert!(
        b.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
    assert!(
        a.cli_access_grants(&other_session, computer)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        a.get(actor.owner(), computer).await.unwrap().phase,
        veoveo_computers::api::ComputerPhase::Ready
    );
}

#[tokio::test]
async fn shared_browser_cli_quota_and_pairing_rate_are_atomic_across_replicas() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let paired = pair(&a, &b, &actor, computer, "ABC-2345").await;
    let challenge = b
        .begin_cli_pairing(&actor, computer, &request("DEF-6789"))
        .await
        .unwrap();
    let (browser_result, cli_result) = tokio::join!(
        a.issue_browser_grant(&actor, computer),
        b.confirm_cli_pairing(&actor, computer, challenge.pairing_id)
    );
    assert_eq!(
        usize::from(browser_result.is_ok()) + usize::from(cli_result.is_ok()),
        1
    );
    assert!(a.issue_browser_grant(&actor, computer).await.is_err());
    assert!(
        b.confirm_cli_pairing(&actor, computer, challenge.pairing_id)
            .await
            .is_err()
    );
    a.revoke_cli_grant(&actor, computer, paired.grant_id)
        .await
        .unwrap();
    assert!(b.issue_browser_grant(&actor, computer).await.is_ok());
    let requests: Vec<_> = [
        "GHJ-2345", "KLM-2345", "NPQ-2345", "RST-2345", "UVW-2345", "XYZ-2345",
    ]
    .into_iter()
    .map(request)
    .collect();
    let outcomes = futures::future::join_all(requests.iter().enumerate().map(|(i, input)| {
        let store = if i % 2 == 0 { &a } else { &b };
        store.begin_cli_pairing(&actor, computer, input)
    }))
    .await;
    // Two earlier challenges, including their consumed state, count toward the window.
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 3);
    assert!(
        outcomes
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| *e == ComputerError::AccessLimit)
    );
}

#[tokio::test]
async fn connection_slots_are_shared_and_expired_connections_cannot_revive() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let paired = pair(&a, &b, &actor, computer, "ABC-2345").await;
    let results = futures::future::join_all((0..20).map(|i| {
        let store = if i % 2 == 0 { &a } else { &b };
        store.open_cli_connection(computer, &paired.credential)
    }))
    .await;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 16);
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| *e == ComputerError::AccessLimit)
    );
    let handles: Vec<_> = results.into_iter().filter_map(Result::ok).collect();
    b.close_cli_connection(&handles[0]).await.unwrap();
    assert!(
        a.open_cli_connection(computer, &paired.credential)
            .await
            .is_ok()
    );
    db.b.client().query("UPDATE computer_cli_connection SET expires_at = time::now() - 1s WHERE grant_id = $grant;").bind(("grant", paired.grant_id)).await.unwrap().check().unwrap();
    assert!(b.renew_cli_grant(&handles[1], true).await.is_err());
    assert!(
        a.open_cli_connection(computer, &paired.credential)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn paired_client_crosses_token_and_process_changes_but_old_connections_and_logout_do_not() {
    let db = support::TestDb::new().await;
    let mut identity = browser(&db, "alice").await;
    identity.expires_at = Utc::now() + TimeDelta::seconds(3);
    identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .expires_at = identity.expires_at;
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let paired = pair(&a, &b, &actor, computer, "ABC-2345").await;
    let handle = a
        .open_cli_connection(computer, &paired.credential)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(3100)).await;
    assert!(a.control_authority(&actor).await.is_err());
    assert!(b.renew_cli_grant(&handle, false).await.is_ok());
    let mut deny = control();
    deny.policies[0].rules[2].effect = PolicyEffect::Deny;
    support::policy::install(&db.b, deny).await;
    assert!(a.renew_cli_grant(&handle, true).await.is_err());
    assert!(
        b.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
    support::policy::install(&db.b, control()).await;
    db.b.client()
        .query("UPDATE ONLY $computer SET process_id = 'replacement-process';")
        .bind(("computer", record("computer", computer)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(a.renew_cli_grant(&handle, false).await.is_err());
    let next = b
        .open_cli_connection(computer, &paired.credential)
        .await
        .unwrap();
    let lease = a.renew_cli_grant(&next, false).await.unwrap();
    assert!(lease.valid_until().duration_since(lease.checked_at()) <= Duration::from_secs(30));
    let family = veoveo_platform_store::gateway_refresh_family_record_id(
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
        .query("UPDATE ONLY $family SET revoked_at = time::now();")
        .bind(("family", family))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(b.renew_cli_grant(&next, true).await.is_err());
    assert!(
        a.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn expired_pairing_and_tightened_or_expired_grants_never_gain_time_from_activity() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let challenge = a
        .begin_cli_pairing(&actor, computer, &request("ABC-2345"))
        .await
        .unwrap();
    db.b.client()
        .query("UPDATE ONLY $pairing SET expires_at = time::now() - 1s;")
        .bind((
            "pairing",
            record("computer_cli_pairing", challenge.pairing_id),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        b.confirm_cli_pairing(&actor, computer, challenge.pairing_id)
            .await
            .is_err()
    );
    let paired = pair(&a, &b, &actor, computer, "DEF-6789").await;
    let handle = a
        .open_cli_connection(computer, &paired.credential)
        .await
        .unwrap();
    let mut before =
        db.b.client()
            .query("SELECT VALUE idle_expires_at FROM ONLY $grant;")
            .bind(("grant", record("computer_cli_grant", paired.grant_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
    let before: Option<chrono::DateTime<Utc>> = before.take(0).unwrap();
    a.renew_cli_grant(&handle, false).await.unwrap();
    let mut after =
        db.b.client()
            .query("SELECT VALUE idle_expires_at FROM ONLY $grant;")
            .bind(("grant", record("computer_cli_grant", paired.grant_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
    let after: Option<chrono::DateTime<Utc>> = after.take(0).unwrap();
    assert_eq!(before, after);
    a.renew_cli_grant(&handle, true).await.unwrap();
    let current = b.cli_access_grants(&actor, computer).await.unwrap();
    assert_eq!(current[0].expires_at, paired.expires_at);
    let tightened = SessionGrantPolicy {
        absolute_seconds: 1,
        idle_seconds: 1,
        ..LIMITS
    };
    db.b.client()
        .query("UPDATE ONLY $grant SET issued_at = time::now() - 2s;")
        .bind(("grant", record("computer_cli_grant", paired.grant_id)))
        .await
        .unwrap()
        .check()
        .unwrap();
    a.install_session_grant_policy(Some(LIMITS), tightened)
        .await
        .unwrap();
    assert!(b.renew_cli_grant(&handle, true).await.is_err());
    assert!(
        a.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
    b.install_session_grant_policy(Some(tightened), LIMITS)
        .await
        .unwrap();
    let closed = SessionGrantPolicy {
        max_grants: 0,
        ..LIMITS
    };
    b.install_session_grant_policy(Some(LIMITS), closed)
        .await
        .unwrap();
    assert!(a.renew_cli_grant(&handle, true).await.is_err());
    assert!(
        b.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
    a.install_session_grant_policy(Some(closed), LIMITS)
        .await
        .unwrap();
    db.b.client()
        .query("UPDATE ONLY $grant SET idle_expires_at = time::now() - 1s;")
        .bind(("grant", record("computer_cli_grant", paired.grant_id)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(b.renew_cli_grant(&handle, true).await.is_err());
    assert!(
        a.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
    db.b.client().query("UPDATE ONLY $grant SET idle_expires_at = time::now() + 1m, expires_at = time::now() - 1s;")
        .bind(("grant", record("computer_cli_grant", paired.grant_id)))
        .await.unwrap().check().unwrap();
    assert!(b.renew_cli_grant(&handle, true).await.is_err());
    assert!(
        a.open_cli_connection(computer, &paired.credential)
            .await
            .is_err()
    );
}
