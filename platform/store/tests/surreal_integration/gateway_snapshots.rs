//! Native closed refresh actors and top-level authorization-code consumption.
use super::fixture;
use chrono::{TimeDelta, Utc};
use surrealdb::types::{RecordId, SurrealValue, Value};
use veoveo_platform_store::{
    GatewayAuthorizationCodeStateRecord, GatewayRefreshFamilyRecord, GatewayRefreshTokenRecord,
};

#[derive(SurrealValue)]
struct FamilyWindow {
    expires_at: chrono::DateTime<Utc>,
}

async fn row(db: &fixture::TestDb, family: &RecordId) -> Value {
    db.a.client()
        .query(include_str!(
            "../queries/surreal_integration/gateway_snapshots/read.surql"
        ))
        .bind(("family", family.clone()))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

#[tokio::test]
async fn refresh_principal_controls_and_family_facts_survive_reconnect_without_unknown_fields() {
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let now = Utc::now();
        let family = GatewayRefreshFamilyRecord {
            id: veoveo_platform_store::gateway_refresh_family_record_id(uuid::Uuid::now_v7()),
            authorization_server:"veoveo".into(), profile:"operator".into(), oauth_client_id:"qualification".into(),
            work_context:"operations".into(), principal_id:"alice".into(), tenant:Some("tenant-refresh".into()), scopes:vec![],
            principal: serde_json::from_value(serde_json::json!({"principal":{
                "id":"alice", "kind":"user", "issuer":"https://identity.test", "subject":"alice", "tenant":"tenant-refresh",
                "groups":["engineering"], "group_roles":[{"group":"engineering","role":"write"}], "roles":[], "scopes":[], "data_labels":[], "assurances":[]
            },"principal_display_name":"Alice"})).unwrap(),
            current_generation:0, issued_at:now, expires_at:now+TimeDelta::hours(1), revoked_at:None, revocation_reason:None,
        };
        let token = GatewayRefreshTokenRecord {
            id:veoveo_platform_store::gateway_refresh_token_record_id(uuid::Uuid::now_v7()), family:family.id.clone(), token_hash:"a".repeat(64), generation:0,
            issued_at:now, expires_at:family.expires_at, consumed_at:None, replacement:None, replay_detected_at:None, delivery_envelope:None, delivery_expires_at:None,
        };
        db.a.create_gateway_refresh_family(family.clone(), token).await.unwrap();
        let reconnect = db.connect_at(db.a.config().endpoint().as_str()).await;
        let (_, decoded) = reconnect.gateway_refresh_grant_by_hash(&"a".repeat(64)).await.unwrap().unwrap();
        assert_eq!(decoded.principal, family.principal);
        let original = row(&db, &family.id).await;
        let Value::Object(record) = &original else { panic!("family missing") };
        let snapshot = record.get("principal").unwrap().clone();
        for which in 0..4 {
            let mut invalid = snapshot.clone();
            let Value::Object(wrapper) = &mut invalid else { panic!("principal wrapper missing") };
            if which == 0 { wrapper.insert("unknown", Value::Bool(true)); }
            else {
                let Value::Object(actor) = wrapper.get_mut("principal").unwrap() else { panic!("actor missing") };
                match which {
                    1 => { actor.insert("unknown", Value::Bool(true)); }
                    2 => { actor.remove("issuer"); }
                    3 => {
                        let Value::Array(roles) = actor.get_mut("group_roles").unwrap() else { panic!("roles missing") };
                        let Value::Object(role) = &mut roles[0] else { panic!("role missing") };
                        role.insert("unknown", Value::Bool(true));
                    }
                    _ => unreachable!(),
                }
            }
            assert!(db.a.client().query(include_str!("../queries/surreal_integration/gateway_snapshots/principal.surql"))
                .bind(("family", family.id.clone())).bind(("principal", invalid)).await.unwrap().check().is_err(), "unknown/missing refresh actor control {which} was accepted");
            assert_eq!(row(&db, &family.id).await, original);
        }
        for at in [now, family.expires_at] {
            let window:Option<FamilyWindow> = db.a.client().query(include_str!("../queries/surreal_integration/gateway_snapshots/window.surql"))
                .bind(("family", family.id.clone())).bind(("now", at)).await.unwrap().check().unwrap().take(0).unwrap();
            if at == now { assert_eq!(window.unwrap().expires_at, family.expires_at); } else { assert!(window.is_none()); }
        }
    }).await.expect("refresh snapshot qualification exceeded three minutes");
}

#[tokio::test]
async fn authorization_code_consumption_uses_only_the_declared_top_level_marker() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let now = Utc::now();
        let id = veoveo_platform_store::gateway_authorization_code_record_id("qualification-code");
        db.a.create_gateway_authorization_code(GatewayAuthorizationCodeStateRecord {
            id: id.clone(),
            code: "qualification-code".into(),
            profile: "operator".into(),
            oauth_client_id: "qualification".into(),
            work_context: "operations".into(),
            oidc_client: "provider".into(),
            principal: "alice".into(),
            redirect_uri: "https://client.test/callback".into(),
            issued_at: now,
            expires_at: now + TimeDelta::minutes(5),
            consumed_at: None,
            payload: serde_json::from_value(
                serde_json::json!({"consumed_at":"already-set-in-opaque-payload"}),
            )
            .unwrap(),
        })
        .await
        .unwrap();
        let consumed =
            db.a.consume_gateway_authorization_code(id.clone(), now)
                .await
                .unwrap()
                .unwrap();
        assert!(consumed.consumed_at.is_none());
        assert!(
            db.b.consume_gateway_authorization_code(id, now)
                .await
                .unwrap()
                .is_none()
        );
    })
    .await
    .expect("authorization code qualification exceeded two minutes");
}

#[tokio::test]
async fn independent_control_object_kinds_follow_shared_extension_name_syntax() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = fixture::TestDb::new().await;
        let revision = RecordId::new("gateway_control_revision", "kind-qualification");
        let content = veoveo_platform_store::GatewayControlRevisionContent {
            revision_id: "kind-qualification".into(),
            sha256: "a".repeat(64),
            source: veoveo_platform_store::GatewayControlRevisionSource::SeedFile,
            applied_at: Utc::now(),
            applied_by: "qualification".into(),
            tenant: None,
            control_plane: Default::default(),
        };
        db.a.client()
            .query(include_str!(
                "../queries/surreal_integration/gateway_snapshots/create_revision.surql"
            ))
            .bind(("revision", revision.clone()))
            .bind(("content", content))
            .await
            .unwrap()
            .check()
            .unwrap();
        for kind in [
            "vendor/fleet-v2".to_owned(),
            "vendor//fleet".to_owned(),
            "".to_owned(),
            "bad name".to_owned(),
            "nonascii-é".to_owned(),
            "a".repeat(129),
        ] {
            let valid = veoveo_types::ExtensionName::parse(kind.clone()).is_ok();
            let object = RecordId::new(
                "gateway_control_object",
                surrealdb::types::Uuid::from(uuid::Uuid::now_v7()),
            );
            let content = veoveo_platform_store::GatewayControlObjectContent {
                revision: revision.clone(),
                tenant: None,
                object_kind: kind.clone(),
                profile_policy_version: None,
                object_id: "qualification".into(),
                document: Default::default(),
            };
            let result =
                db.a.client()
                    .query(include_str!(
                        "../queries/surreal_integration/gateway_snapshots/create_object.surql"
                    ))
                    .bind(("object", object.clone()))
                    .bind(("content", content))
                    .await
                    .unwrap()
                    .check();
            assert_eq!(
                result.is_ok(),
                valid,
                "native and shared control-kind admission disagree for {kind:?}: {:?}",
                result.as_ref().err()
            );
            let stored: Option<Value> =
                db.a.client()
                    .query(include_str!(
                        "../queries/surreal_integration/gateway_snapshots/read_object.surql"
                    ))
                    .bind(("object", object))
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            assert_eq!(
                stored.is_some(),
                valid,
                "rejected control-kind create left a record"
            );
        }
    })
    .await
    .expect("control-kind qualification exceeded two minutes");
}
