use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeDelta, Utc};
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use uuid::Uuid;
use veoveo_mcp_contract::{
    AccessTokenSubject, GatewayAction, GatewayControlPlane, LocalToolName, OAuthClientId,
    PolicyTarget, Principal, PrincipalId, PrincipalKind, ScopeName, ServerSlug, TenantId,
    TokenIssuer, TokenSubject, WorkContextId, agent_management as wire,
};
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipLevel, agent_management::instances::*, agent_management::*,
    deterministic_work_context_id,
};

use super::*;
use crate::{GatewayCatalog, GatewayState, VerifiedAccessToken, test_store::TestDb};

fn catalog() -> GatewayCatalog {
    GatewayCatalog::from_control_plane(
        serde_json::from_str::<GatewayControlPlane>(include_str!(
            "../../../../configs/gateway.smoke.json"
        ))
        .unwrap(),
    )
    .unwrap()
}

fn template() -> wire::RuntimeTemplate {
    serde_json::from_value(serde_json::json!({
        "id":"pilot", "name":"Reviewed pilot", "tenant":"tenant-a", "work_contexts":["operations"],
        "required_deployer_scopes":["operator:use"], "profile":"operator", "scopes":["operator:use"], "roles":["managed-pilot"], "membership":"contributor",
        "models":["approved"], "tools":["media__describe_model"], "resource_subscriptions":[],
        "parameters":{"vehicle":{"label":"Vehicle", "shape":{"kind":"identifier", "maxLength":40}, "environment_variable":"VEOVEO_PARAM_VEHICLE"}},
        "workload":{"namespace":"agents", "config_map":"pilot-template", "config_digest":format!("sha256:{}", "b".repeat(64)), "image":format!("registry.test/kernel@sha256:{}", "a".repeat(64)),
            "database_secret":"agent-store", "storage_class":"local-path", "storage_gib":2, "cpu_millis":500, "memory_mib":1024,
            "model_secrets":[{"reference":"media_provider_api_key", "secret":"agent-model", "key":"api-key"}]}
    })).unwrap()
}

fn templates(template: &wire::RuntimeTemplate, catalog: &GatewayCatalog) -> ManagedTemplateCatalog {
    ManagedTemplateCatalog::from_json(&serde_json::to_string(&[template]).unwrap(), catalog)
        .unwrap()
}

#[test]
fn template_parameters_and_public_choices_cannot_select_credentials_or_code() {
    let catalog = catalog();
    let mut template = template();
    let admitted = templates(&template, &catalog);
    let original_revision = runtime_template_revision(&template);
    let mut changed = template.clone();
    changed.workload.config_digest =
        veoveo_mcp_contract::Sha256Digest::from_hex("c".repeat(64)).unwrap();
    assert_ne!(runtime_template_revision(&changed), original_revision);
    assert!(ManagedTemplateCatalog::parameters(
        &template,
        &BTreeMap::from([(
            "vehicle".into(),
            wire::TemplateParameter::Text("uav-5".into())
        )])
    ));
    for value in ["${PRIVATE_KEY}", "../memory", "a;cmd", ""] {
        assert!(!ManagedTemplateCatalog::parameters(
            &template,
            &BTreeMap::from([(
                "vehicle".into(),
                wire::TemplateParameter::Text(value.into())
            )])
        ));
    }
    assert!(!ManagedTemplateCatalog::parameters(
        &template,
        &BTreeMap::new()
    ));
    let mut principal = principal("managed-one");
    let context = WorkContextId::new("operations").unwrap();
    let public = serde_json::to_string(&admitted.choices(&principal, &context)).unwrap();
    assert!(public.contains("Vehicle"));
    for private in [
        "agent-store",
        "agent-model",
        "registry.test",
        "VEOVEO_PARAM_",
    ] {
        assert!(!public.contains(private));
    }
    principal.scopes.clear();
    assert!(admitted.choices(&principal, &context).is_empty());
    template
        .parameters
        .get_mut("vehicle")
        .unwrap()
        .environment_variable = "VEOVEO_AGENT_PRIVATE_KEY".into();
    assert!(
        ManagedTemplateCatalog::from_json(&serde_json::to_string(&[template]).unwrap(), &catalog)
            .is_err()
    );
}

fn principal(client: &str) -> Principal {
    Principal {
        id: PrincipalId::new(format!("https://veoveo.example/oauth#{client}")).unwrap(),
        kind: PrincipalKind::Service,
        issuer: TokenIssuer::new("https://veoveo.example/oauth").unwrap(),
        subject: TokenSubject::new(client).unwrap(),
        tenant: Some(TenantId::new("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    }
}

async fn provision(
    store: &PlatformStore,
    template: &wire::RuntimeTemplate,
) -> (AgentCatalogAuthority, AgentDefinition, ManagedAgentInstance) {
    let actor = store
        .ensure_identity(
            "tenant-a",
            "alice",
            "https://idp.example",
            "alice",
            veoveo_platform_store::PrincipalKind::User,
        )
        .await
        .unwrap();
    let context = deterministic_work_context_id("tenant-a", "operations").unwrap();
    store.client().query("CREATE ONLY $context SET tenant = $tenant, context_key = 'operations', title = 'Operations', policy_revision = 'v1', memberships = [], output_policy = {owner_kind:'principal', owner_key:'alice', initial_grants:[], data_labels:[]};")
        .bind(("context", context.record_id())).bind(("tenant", actor.tenant_id.record_id())).await.unwrap().check().unwrap();
    let digest = store
        .artifact_read_context_version("tenant-a", "operations")
        .await
        .unwrap()
        .unwrap()
        .digest;
    let authority = AgentCatalogAuthority::new(
        actor.tenant_id,
        context,
        actor.principal_id,
        digest,
        WorkContextMembershipLevel::Owner,
        true,
        100,
    );
    let revision = runtime_template_revision(template);
    let content = AgentContent {
        model: AgentModelReference {
            id: "approved".into(),
            revision: "b".repeat(64),
        },
        instructions: "Published pilot instructions".into(),
        tools: vec!["media__describe_model".into()],
        budgets: AgentBudgets {
            max_output_tokens: 100,
            max_completion_calls: 2,
            max_tool_calls: 2,
            deadline_seconds: 30,
        },
        execution: AgentExecution::Managed {
            template: "pilot".into(),
            template_revision: revision.as_str().strip_prefix("sha256:").unwrap().into(),
            parameters: BTreeMap::from([(
                "vehicle".into(),
                AgentTemplateParameter::Text("uav-5".into()),
            )]),
            resource_subscriptions: vec![],
        },
    };
    let draft = store
        .mutate_agent_definition(
            &authority,
            "pilot",
            Uuid::now_v7(),
            None,
            AgentDefinitionMutation::Create {
                name: "Pilot".into(),
                description: "Test".into(),
                content,
            },
        )
        .await
        .unwrap();
    let definition = store
        .mutate_agent_definition(
            &authority,
            "pilot",
            Uuid::now_v7(),
            Some(draft.revision),
            AgentDefinitionMutation::Publish {
                digest: draft.draft_digest,
                audience: vec![AgentPublicationContext {
                    work_context: authority.work_context.clone(),
                    context_digest: authority.context_digest.clone(),
                }],
            },
        )
        .await
        .unwrap();
    let operation = store
        .mutate_managed_agent(
            &authority,
            "one",
            Uuid::now_v7(),
            None,
            ManagedAgentMutation::Provision {
                plan: Box::new(ManagedAgentProvision {
                    name: "One".into(),
                    definition_key: "pilot".into(),
                    revision: definition.draft_digest.clone(),
                    identity: ManagedAgentIdentity {
                        client_id: "managed-one".into(),
                        issuer: "https://veoveo.example/oauth".into(),
                        authorization_server: "veoveo".into(),
                        profile: "operator".into(),
                        resource: "https://veoveo.example/mcp/operator".into(),
                        scopes: vec!["operator:use".into()],
                        roles: vec!["managed-pilot".into()],
                        membership: WorkContextMembershipLevel::Contributor,
                    },
                    resources: ManagedAgentResources {
                        namespace: "agents".into(),
                        workload: "one".into(),
                        credential_secret: "one-key".into(),
                        volume_claim: "one-memory".into(),
                        template_config_map: "pilot-template".into(),
                        image: template.workload.image.clone(),
                        storage_gib: 2,
                    },
                }),
            },
            ManagedAgentLimits {
                instances: 20,
                storage_gib: 100,
            },
        )
        .await
        .unwrap();
    let owner = Uuid::now_v7();
    let claim = store
        .claim_managed_agent_operation(operation.id, owner)
        .await
        .unwrap()
        .unwrap()
        .claim(owner)
        .unwrap();
    store
        .observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    let keys: JwkSet =
        serde_json::from_str(include_str!("../../../../configs/jwks.smoke.json")).unwrap();
    let AlgorithmParameters::RSA(key) = &keys.keys[0].algorithm else {
        panic!("RSA fixture")
    };
    store
        .register_managed_agent_key(
            &claim,
            ManagedAgentPublicKey {
                kid: "test-key".into(),
                n: key.n.clone(),
                e: key.e.clone(),
            },
        )
        .await
        .unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        ManagedAgentPhase::Workload,
        ManagedAgentPhase::Ready,
    ] {
        store
            .observe_managed_agent(&claim, phase, None)
            .await
            .unwrap();
    }
    let instance = store.managed_agent(&authority, "one").await.unwrap();
    (authority, definition, instance)
}

fn token(binding: wire::ManagedAgentToken) -> VerifiedAccessToken {
    let principal = principal("managed-one");
    VerifiedAccessToken {
        access_token: AccessTokenSubject {
            managed_agent: Some(binding),
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::new("managed-one").unwrap(),
            session_family: None,
            audience: veoveo_mcp_contract::ProtectedResourceId::new(
                "https://veoveo.example/mcp/operator",
            )
            .unwrap(),
            work_context: WorkContextId::new("operations").unwrap(),
            invocation_mode: veoveo_mcp_contract::InvocationMode::Automated,
            initiator: None,
            delegation_id: None,
            scopes: principal.scopes.clone(),
            jwt_id: None,
            issued_at: Utc::now(),
            not_before: None,
            expires_at: Utc::now() + TimeDelta::minutes(15),
        },
        principal,
        principal_display_name: None,
    }
}

#[tokio::test]
async fn managed_identity_rechecks_binding_tools_revocation_and_source_collisions() {
    let db = TestDb::new().await;
    let catalog = catalog();
    let template = template();
    let (authority, definition, instance) = provision(&db.a, &template).await;
    let state =
        GatewayState::new(db.a.clone()).with_managed_templates(templates(&template, &catalog));
    let client_id = OAuthClientId::new("managed-one").unwrap();
    let effective = state
        .effective_oauth_client(&catalog, &client_id)
        .await
        .unwrap()
        .unwrap();
    assert!(effective.registration.jwks.is_none());
    assert_eq!(
        effective.public_keys.as_ref().unwrap().keys[0]
            .common
            .key_id
            .as_deref(),
        Some("test-key")
    );
    let verified = token(effective.token_binding().unwrap().unwrap());
    let subject = state
        .resolve_authenticated_subject(&catalog, verified.clone())
        .await
        .unwrap();
    assert!(
        subject
            .principal
            .roles
            .iter()
            .any(|role| role.as_str() == "managed-pilot")
    );
    let allowed = PolicyTarget::Tool {
        server: ServerSlug::new("media").unwrap(),
        tool: LocalToolName::new("describe_model").unwrap(),
    };
    let other = PolicyTarget::Tool {
        server: ServerSlug::new("media").unwrap(),
        tool: LocalToolName::new("run").unwrap(),
    };
    assert!(
        state
            .managed_action_admitted(&catalog, &subject, GatewayAction::ToolsCall, &allowed)
            .await
            .unwrap()
    );
    assert!(
        !state
            .managed_action_admitted(&catalog, &subject, GatewayAction::ToolsCall, &other)
            .await
            .unwrap()
    );
    let mut missing = verified.clone();
    missing.access_token.managed_agent = None;
    assert!(
        state
            .resolve_authenticated_subject(&catalog, missing)
            .await
            .is_err()
    );
    let mut old = verified.clone();
    old.access_token.managed_agent.as_mut().unwrap().generation += 1;
    assert!(
        state
            .resolve_authenticated_subject(&catalog, old)
            .await
            .is_err()
    );
    let mut excessive = verified.clone();
    excessive
        .access_token
        .scopes
        .insert(ScopeName::new("admin:use").unwrap());
    assert!(
        state
            .resolve_authenticated_subject(&catalog, excessive)
            .await
            .is_err()
    );
    db.a.mutate_managed_agent(
        &authority,
        "one",
        Uuid::now_v7(),
        Some(1),
        ManagedAgentMutation::Stop,
        ManagedAgentLimits {
            instances: 20,
            storage_gib: 100,
        },
    )
    .await
    .unwrap();
    assert!(
        !state
            .managed_action_admitted(&catalog, &subject, GatewayAction::ToolsCall, &allowed)
            .await
            .unwrap()
    );
    assert!(
        state
            .managed_action_admitted(
                &catalog,
                &subject,
                GatewayAction::TasksGet,
                &PolicyTarget::Gateway
            )
            .await
            .unwrap(),
        "stop preserves observation"
    );
    db.a.mutate_agent_definition(
        &authority,
        "pilot",
        Uuid::now_v7(),
        Some(definition.revision),
        AgentDefinitionMutation::Status {
            status: AgentDefinitionStatus::Disabled,
        },
    )
    .await
    .unwrap();
    assert!(
        state
            .effective_oauth_client(&catalog, &client_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .resolve_authenticated_subject(&catalog, verified)
            .await
            .is_err()
    );
    let mut plane = catalog.control_plane().clone();
    let mut static_client = plane
        .oauth_clients
        .iter()
        .find(|c| c.id.as_str() == "operator-service")
        .unwrap()
        .clone();
    static_client.id = client_id.clone();
    plane.oauth_clients.push(static_client);
    let collision = GatewayCatalog::from_control_plane(plane).unwrap();
    assert!(
        state
            .effective_oauth_client(&collision, &client_id)
            .await
            .is_err(),
        "disabled registration cannot fall back to a colliding static source"
    );
    assert_eq!(
        db.a.managed_agent(&authority, "one")
            .await
            .unwrap()
            .principal,
        instance.principal
    );
}
