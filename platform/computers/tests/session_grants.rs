//! Real ledger, two independent clients and synthetic Ready Computer rows. These
//! cases establish access authority, not provider attachment or terminal behavior.
mod support;
use chrono::{TimeDelta, Utc};
use std::time::Duration;
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputerError, ComputersStore, Reservation,
    session_grants::SessionGrantPolicy,
};
use veoveo_mcp_contract::*;
use veoveo_platform_store::{
    GatewayRefreshFamilyRecord, RecordId, gateway_refresh_family_record_id,
};

const PROVIDER: Uuid = Uuid::from_u128(72);
const LIMITS: SessionGrantPolicy = SessionGrantPolicy {
    max_grants: 2,
    absolute_seconds: 120,
    idle_seconds: 30,
};

#[path = "support/grant_limits.rs"]
mod limits;

async fn browser(db: &support::TestDb, name: &str) -> GatewayInternalIdentity {
    let owner = support::owner(name);
    db.a.ensure_identity(
        owner.tenant_key(),
        &owner.principal_key,
        &owner.issuer,
        &owner.subject,
        owner.principal_kind,
    )
    .await
    .unwrap();
    let mut identity = support::identity(&owner);
    let id = Uuid::now_v7();
    let context = identity.request_context.as_mut().unwrap();
    context.access_token.session_family =
        Some(GatewayRefreshFamilyId::new(id.to_string()).unwrap());
    let principal = &context.principal;
    let now = Utc::now();
    let record = gateway_refresh_family_record_id(id);
    let family = GatewayRefreshFamilyRecord {
        id: record.clone(),
        authorization_server: "veoveo".into(),
        profile: identity.profile.to_string(),
        oauth_client_id: context.access_token.oauth_client_id.to_string(),
        work_context: identity.authority.work_context.to_string(),
        principal_id: principal.id.to_string(),
        tenant: principal.tenant.as_ref().map(ToString::to_string),
        scopes: principal.scopes.iter().map(ToString::to_string).collect(),
        principal: serde_json::from_value(
            serde_json::json!({"principal":principal,"principal_display_name":"Browser fixture"}),
        )
        .unwrap(),
        current_generation: 0,
        issued_at: now,
        expires_at: now + TimeDelta::hours(1),
        revoked_at: None,
        revocation_reason: None,
    };
    let _: Option<GatewayRefreshFamilyRecord> =
        db.a.client().create(record).content(family).await.unwrap();
    identity
}
fn control() -> GatewayControlPlane {
    let mut control = support::policy::control();
    let mut read = control.policies[0].rules[0].clone();
    read.id = PolicyRuleId::new("computer-read").unwrap();
    read.actions = [GatewayAction::ResourcesRead].into_iter().collect();
    read.tools.clear();
    control.policies[0].rules.push(read.clone());
    read.id = PolicyRuleId::new("computer-attach").unwrap();
    read.actions = [GatewayAction::ComputerAttach].into_iter().collect();
    control.policies[0].rules.push(read);
    control
}
async fn ready(
    db: &support::TestDb,
    actor: &ComputerActor,
) -> (ComputersStore, ComputersStore, Uuid) {
    support::policy::install(&db.a, control()).await;
    let a = ComputersStore::new(db.a.clone(), PROVIDER).unwrap();
    let b = ComputersStore::new(db.b.clone(), PROVIDER).unwrap();
    a.install_capacity(
        None,
        CapacityPolicy {
            per_owner: 4,
            per_tenant: 8,
            provider: 8,
        },
    )
    .await
    .unwrap();
    a.install_session_grant_policy(None, LIMITS).await.unwrap();
    let computer = a
        .reserve(
            actor.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    db.a.client().query("UPDATE ONLY $computer SET phase = 'ready', provider_resource_id = 'fixture-resource', process_id = 'fixture-process';")
        .bind(("computer", RecordId::new("computer", surrealdb::types::Uuid::from(computer.computer_id))))
        .await.unwrap().check().unwrap();
    (a, b, computer.computer_id)
}
fn grant_record(id: Uuid) -> RecordId {
    RecordId::new("computer_session_grant", surrealdb::types::Uuid::from(id))
}

#[tokio::test]
async fn ticket_redemption_is_private_one_use_and_cross_replica_revocation_ends_access() {
    let db = support::TestDb::new().await;
    let identity = browser(&db, "alice").await;
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let other_session = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let other_user = ComputerActor::from_verified(&browser(&db, "bob").await).unwrap();
    assert!(
        a.issue_browser_grant(&support::authenticated(&support::owner("alice")), computer)
            .await
            .is_err()
    );
    let ticket = a.issue_browser_grant(&actor, computer).await.unwrap();
    assert!(
        b.redeem_browser_grant(&other_session, &ticket.token)
            .await
            .is_err()
    );
    assert!(
        b.redeem_browser_grant(&other_user, &ticket.token)
            .await
            .is_err()
    );
    let (first, second) = tokio::join!(
        a.redeem_browser_grant(&actor, &ticket.token),
        b.redeem_browser_grant(&actor, &ticket.token)
    );
    let handle = match (first, second) {
        (Ok(handle), Err(_)) | (Err(_), Ok(handle)) => handle,
        _ => panic!("ticket must create exactly one attachment"),
    };
    assert!(b.renew_browser_grant(&handle, false).await.is_ok());
    let mut stored =
        db.b.client()
            .query("SELECT VALUE ticket_hash FROM ONLY $grant;")
            .bind(("grant", grant_record(handle.grant_id())))
            .await
            .unwrap()
            .check()
            .unwrap();
    let hash: Option<String> = stored.take(0).unwrap();
    let hash = hash.unwrap();
    assert_eq!(hash.len(), 64);
    assert!(!ticket.token.expose_secret().contains(&hash));
    assert!(
        a.revoke_browser_grant(&other_user, handle.grant_id())
            .await
            .is_err()
    );
    a.revoke_browser_grant(&other_session, handle.grant_id())
        .await
        .unwrap();
    assert!(b.renew_browser_grant(&handle, true).await.is_err());
    assert!(a.redeem_browser_grant(&actor, &ticket.token).await.is_err());
    assert_eq!(
        a.get(actor.owner(), computer).await.unwrap().phase,
        veoveo_computers::api::ComputerPhase::Ready
    );
}

#[tokio::test]
async fn grant_admission_is_bounded_and_stale_policy_cannot_reopen_it() {
    let db = support::TestDb::new().await;
    let actor = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    let results = futures::future::join_all((0..6).map(|i| {
        let store = if i % 2 == 0 { &a } else { &b };
        store.issue_browser_grant(&actor, computer)
    }))
    .await;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 2);
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| *e == ComputerError::AccessLimit)
    );
    let ticket = results.into_iter().find_map(Result::ok).unwrap();
    let handle = a.redeem_browser_grant(&actor, &ticket.token).await.unwrap();
    let closed = SessionGrantPolicy {
        max_grants: 0,
        ..LIMITS
    };
    b.install_session_grant_policy(Some(LIMITS), closed)
        .await
        .unwrap();
    assert_eq!(
        a.install_session_grant_policy(None, LIMITS).await.err(),
        Some(ComputerError::PolicyConflict)
    );
    assert!(a.renew_browser_grant(&handle, false).await.is_err());
    assert!(a.issue_browser_grant(&actor, computer).await.is_err());
    b.close_browser_grant(&handle).await.unwrap();
    a.close_browser_grant(&handle).await.unwrap();
}

#[tokio::test]
async fn accepted_grant_crosses_token_expiry_but_never_logout_or_current_policy_and_run() {
    let db = support::TestDb::new().await;
    let identity = browser(&db, "alice").await;
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let (a, b, computer) = ready(&db, &actor).await;
    // Grant authority retains the admitted context, not bearer bytes or token renewal.
    let mut short = identity.clone();
    short.expires_at = Utc::now() + TimeDelta::seconds(2);
    short
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .expires_at = short.expires_at;
    let short_actor = ComputerActor::from_verified(&short).unwrap();
    let ticket = a.issue_browser_grant(&short_actor, computer).await.unwrap();
    let handle = b
        .redeem_browser_grant(&short_actor, &ticket.token)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert!(a.control_authority(&short_actor).await.is_err());
    let lease = a.renew_browser_grant(&handle, false).await.unwrap();
    assert!(lease.valid_until().duration_since(lease.checked_at()) <= Duration::from_secs(30));
    let mut revoked = control();
    revoked.policies[0].rules[2].effect = PolicyEffect::Deny;
    support::policy::install(&db.b, revoked).await;
    assert!(b.renew_browser_grant(&handle, false).await.is_err());
    support::policy::install(&db.b, control()).await;
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
        .query("UPDATE ONLY $family SET revoked_at = time::now();")
        .bind(("family", family))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(b.renew_browser_grant(&handle, true).await.is_err());
    let fresh = ComputerActor::from_verified(&browser(&db, "alice").await).unwrap();
    let ticket = a.issue_browser_grant(&fresh, computer).await.unwrap();
    let next = b.redeem_browser_grant(&fresh, &ticket.token).await.unwrap();
    db.b.client()
        .query("UPDATE ONLY $computer SET process_id = 'replacement-process';")
        .bind((
            "computer",
            RecordId::new("computer", surrealdb::types::Uuid::from(computer)),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(a.renew_browser_grant(&next, false).await.is_err());
}
