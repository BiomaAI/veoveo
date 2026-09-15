//! Router and store acceptance. AuthenticatedSubject is injected as the boundary
//! fixture; JWT signature/session-family admission has its own gateway tests.
use crate::test_store as fixture;

use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{TimeDelta, Utc};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tower::ServiceExt;
use veoveo_mcp_contract::*;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipRuleRecord, deterministic_tenant_id,
    deterministic_work_context_id,
};

fn subject(name: &str) -> AuthenticatedSubject {
    let principal = Principal {
        id: PrincipalId::new(format!("https://workspace.test#{name}")).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://workspace.test").unwrap(),
        subject: TokenSubject::new(name).unwrap(),
        tenant: Some(TenantId::new("test").unwrap()),
        groups: [GroupId::new("collaborators").unwrap()]
            .into_iter()
            .collect(),
        roles: Default::default(),
        group_roles: Default::default(),
        scopes: Default::default(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let authority: InvocationAuthority = serde_json::from_value(json!({
        "work_context":"shared", "tenant":"test", "membership":"contributor", "policy_revision":"v1",
        "output_policy":{"owner":{"kind":"principal","id":principal.id}},
        "provenance":{"mode":"direct","initiator":principal.id}
    })).unwrap();
    let now = Utc::now();
    let access_token = AccessTokenSubject {
        issuer: principal.issuer.clone(),
        subject: principal.subject.clone(),
        oauth_client_id: OAuthClientId::new("workspace").unwrap(),
        session_family: Some(
            GatewayRefreshFamilyId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        ),
        audience: ProtectedResourceId::new("https://workspace.test/mcp/operator").unwrap(),
        work_context: authority.work_context.clone(),
        invocation_mode: InvocationMode::Direct,
        initiator: Some(principal.id.clone()),
        delegation_id: None,
        scopes: Default::default(),
        jwt_id: Some(JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap()),
        issued_at: now,
        not_before: None,
        expires_at: now + TimeDelta::minutes(5),
    };
    AuthenticatedSubject {
        access_token,
        principal: principal.clone(),
        actor: principal,
        principal_display_name: None,
        authority,
    }
}

async fn setup(store: &PlatformStore) {
    for name in ["Alice", "Bob", "Eve"] {
        let actor = subject(name);
        store
            .ensure_named_identity(
                "test",
                actor.principal.id.as_str(),
                actor.principal.issuer.as_str(),
                name,
                veoveo_platform_store::PrincipalKind::User,
                name,
            )
            .await
            .unwrap();
    }
    let rule = WorkContextMembershipRuleRecord {
        level: veoveo_platform_store::WorkContextMembershipLevel::Contributor,
        principals: vec![],
        groups: vec!["collaborators".into()],
        roles: vec![],
        oauth_clients: vec![],
    };
    store.client().query("CREATE ONLY $context SET tenant = $tenant, context_key = 'shared', title = 'Shared work',
        policy_revision = 'v1', memberships = $rules,
        output_policy = {owner_kind: 'principal', owner_key: 'Alice', initial_grants: [], data_labels: []};")
        .bind(("context", deterministic_work_context_id("test", "shared").unwrap().record_id()))
        .bind(("tenant", deterministic_tenant_id("test").unwrap().record_id()))
        .bind(("rules", vec![rule])).await.unwrap().check().unwrap();
}

fn app(store: &PlatformStore, subject: AuthenticatedSubject) -> Router {
    super::router(super::WorkspaceState {
        store: store.clone(),
    })
    .layer(Extension(subject))
}

async fn request(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/workspace-api/operator{path}"))
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    if status.is_success() {
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    (
        status,
        to_bytes(response.into_body(), 128 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}

async fn ok<T: DeserializeOwned>(app: &Router, method: &str, path: &str, body: Value) -> T {
    let (status, bytes) = request(app, method, path, body).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn ordinary_humans_collaborate_and_cannot_forge_authors_or_read_another_chat() {
    let db = fixture::TestDb::new().await;
    setup(&db.a).await;
    let alice = app(&db.a, subject("Alice"));
    let bob = app(&db.b, subject("Bob"));
    let eve = app(&db.b, subject("Eve"));
    let identity: workspace::WorkspaceBootstrap = ok(&alice, "GET", "/session", Value::Null).await;
    assert_eq!(identity.person.display_name, "Alice");
    assert!(identity.can_contribute);
    assert_eq!(identity.work_context_title, "Shared work");
    let chat = uuid::Uuid::now_v7();
    let _: workspace::Chat = ok(
        &alice,
        "POST",
        "/chats",
        json!({"id":chat,"title":"Project"}),
    )
    .await;
    let people: Vec<workspace::Person> = ok(&alice, "GET", "/people?q=Bob", Value::Null).await;
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].display_name, "Bob");
    let invitation = uuid::Uuid::now_v7();
    let _: workspace::Invitation = ok(
        &alice,
        "POST",
        &format!("/chats/{chat}/invitations"),
        json!({"id":invitation,"invitee":people[0].id}),
    )
    .await;
    assert_eq!(
        request(&bob, "GET", &format!("/chats/{chat}"), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &eve,
            "POST",
            &format!("/invitations/{invitation}"),
            json!({"chatId":chat,"state":"accepted"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let _: workspace::Invitation = ok(
        &bob,
        "POST",
        &format!("/invitations/{invitation}"),
        json!({"chatId":chat,"state":"accepted"}),
    )
    .await;
    let path = format!("/chats/{chat}/messages");
    assert_eq!(
        request(
            &bob,
            "POST",
            &path,
            json!({"id":uuid::Uuid::now_v7(),"text":"Forged","author":uuid::Uuid::now_v7()})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let message: workspace::Message = ok(
        &bob,
        "POST",
        &path,
        json!({"id":uuid::Uuid::now_v7(),"text":"Hello","replyTo":null}),
    )
    .await;
    let snapshot: workspace::ChatSnapshot =
        ok(&alice, "GET", &format!("/chats/{chat}"), Value::Null).await;
    let author = snapshot
        .members
        .iter()
        .find(|member| member.id == message.author)
        .unwrap();
    assert_eq!(author.person.display_name, "Bob");
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(
        request(&eve, "GET", &format!("/chats/{chat}"), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &alice,
            "DELETE",
            &format!("/chats/{chat}/members/{}", people[0].id.0),
            Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(&bob, "GET", &format!("/chats/{chat}"), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn stored_policy_revocation_and_service_identity_cannot_be_bypassed_by_token_claims() {
    let db = fixture::TestDb::new().await;
    setup(&db.a).await;
    let mut service = subject("Alice");
    service.principal.kind = PrincipalKind::Service;
    assert_eq!(
        request(&app(&db.a, service), "GET", "/chats", Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let alice = app(&db.a, subject("Alice"));
    let _: Vec<workspace::Chat> = ok(&alice, "GET", "/chats", Value::Null).await;
    db.a.client()
        .query("UPDATE ONLY $context SET memberships = [], policy_revision = 'v2';")
        .bind((
            "context",
            deterministic_work_context_id("test", "shared")
                .unwrap()
                .record_id(),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    // The injected token still says contributor and still has the old group.
    assert_eq!(
        request(&alice, "GET", "/chats", Value::Null).await.0,
        StatusCode::FORBIDDEN
    );
}
