//! A retained browser-family fixture; never installation credentials.
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_mcp_contract::*;
use veoveo_platform_store::{GatewayRefreshFamilyRecord, gateway_refresh_family_record_id};
pub async fn identity(db: &super::TestDb, name: &str) -> GatewayInternalIdentity {
    let owner = super::owner(name);
    db.a.ensure_identity(
        owner.tenant_key(),
        &owner.principal_key,
        &owner.issuer,
        &owner.subject,
        owner.principal_kind,
    )
    .await
    .unwrap();
    let mut identity = super::identity(&owner);
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
