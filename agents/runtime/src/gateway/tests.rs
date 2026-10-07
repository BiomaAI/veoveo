use crate::persistence::AgentRepository;
mod policy_actions;

use std::collections::{BTreeMap, BTreeSet};
use veoveo_gateway_contract::GatewayAction;
use veoveo_types::OAuthClientId;

use crate::contract::authoring as wire;
use crate::persistence::{instances::*, *};
use chrono::{TimeDelta, Utc};
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use uuid::Uuid;
use veoveo_mcp_contract::{
    AccessTokenSubject, GatewayControlPlane, LocalToolName, PolicyTarget, Principal, PrincipalKind,
    ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipLevel, deterministic_work_context_id,
};
use veoveo_types::{PrincipalId, ScopeName, TenantId, WorkContextId};

use super::*;
use crate::test_store::TestDb;
use std::sync::Arc;
use veoveo_mcp_gateway::{GatewayCatalog, GatewayState, VerifiedAccessToken};

fn catalog() -> GatewayCatalog {
    GatewayCatalog::from_control_plane(
        serde_json::from_str::<GatewayControlPlane>(include_str!(
            "../../../../configs/gateway.smoke.json"
        ))
        .unwrap(),
        crate::catalog_fixture::binding(),
    )
    .unwrap()
}

fn template() -> wire::RuntimeTemplate {
    serde_json::from_value(serde_json::json!({
        "id":"pilot", "name":"Reviewed pilot", "tenant":"tenant-a", "workContexts":["operations"],
        "requiredDeployerScopes":["operator:use"], "profile":"operator", "scopes":["operator:use"], "roles":["managed-pilot"], "membership":"contributor",
        "models":["approved"], "tools":["media__describe_model"], "resourceSubscriptions":[],
        "parameters":{"vehicle":{"label":"Vehicle", "shape":{"kind":"identifier", "maxLength":40}, "environmentVariable":"VEOVEO_PARAM_VEHICLE"}},
        "workload":{"namespace":"agents", "configMap":"pilot-template", "configDigest":format!("sha256:{}", "b".repeat(64)), "image":format!("registry.test/kernel@sha256:{}", "a".repeat(64)),
            "databaseSecret":"agent-store", "storageClass":"local-path", "storageGib":2, "cpuMillis":500, "memoryMib":1024,
            "modelSecrets":[{"reference":"media_provider_api_key", "secret":"agent-model", "key":"api-key"}]}
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
    changed.workload.config_digest = veoveo_types::Sha256Digest::from_hex("c".repeat(64)).unwrap();
    assert_ne!(runtime_template_revision(&changed), original_revision);
    assert!(template.accepts_parameters(&BTreeMap::from([(
        "vehicle".into(),
        wire::TemplateParameter::Text("uav-5".into())
    )])));
    for value in ["${PRIVATE_KEY}", "../memory", "a;cmd", ""] {
        assert!(!template.accepts_parameters(&BTreeMap::from([(
            "vehicle".into(),
            wire::TemplateParameter::Text(value.into())
        )])));
    }
    assert!(!template.accepts_parameters(&BTreeMap::new()));
    let mut principal = principal("managed-one");
    let context = WorkContextId::parse("operations").unwrap();
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
        id: PrincipalId::parse(format!("https://veoveo.example/oauth#{client}")).unwrap(),
        kind: PrincipalKind::Service,
        issuer: TokenIssuer::parse("https://veoveo.example/oauth").unwrap(),
        subject: TokenSubject::parse(client).unwrap(),
        tenant: Some(TenantId::parse("tenant-a").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
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
    store
        .client()
        .query(include_str!(
            "../queries/gateway/tests/provision/statement_1.surql"
        ))
        .bind(("context", context.record_id()))
        .bind(("tenant", actor.tenant_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
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
    let draft = AgentRepository::new(store.clone())
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
    let definition = AgentRepository::new(store.clone())
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
    let operation = AgentRepository::new(store.clone())
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
    let claim = AgentRepository::new(store.clone())
        .claim_managed_agent_operation(operation.id, owner)
        .await
        .unwrap()
        .unwrap()
        .claim(owner)
        .unwrap();
    AgentRepository::new(store.clone())
        .observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    let keys: JwkSet =
        serde_json::from_str(include_str!("../../../../configs/jwks.smoke.json")).unwrap();
    let AlgorithmParameters::RSA(key) = &keys.keys[0].algorithm else {
        panic!("RSA fixture")
    };
    AgentRepository::new(store.clone())
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
        AgentRepository::new(store.clone())
            .observe_managed_agent(&claim, phase, None)
            .await
            .unwrap();
    }
    let instance = AgentRepository::new(store.clone())
        .managed_agent(&authority, "one")
        .await
        .unwrap();
    (authority, definition, instance)
}

fn token(extensions: veoveo_types::AdmittedExtensions) -> VerifiedAccessToken {
    let principal = principal("managed-one");
    VerifiedAccessToken {
        access_token: AccessTokenSubject {
            managed_execution: None,
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::parse("managed-one").unwrap(),
            session_family: None,
            audience: veoveo_gateway_contract::ProtectedResourceId::parse(
                "https://veoveo.example/mcp/operator",
            )
            .unwrap(),
            work_context: WorkContextId::parse("operations").unwrap(),
            invocation_mode: veoveo_types::InvocationMode::Automated,
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
        extensions,
    }
}

#[tokio::test]
async fn managed_identity_rechecks_binding_tools_revocation_and_source_collisions() {
    tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let db = TestDb::with_modules(vec![
            crate::schema::module_setup(
                crate::test_store::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let catalog = catalog();
        let template = template();
        let (authority, definition, instance) = provision(&db.a, &template).await;
        let state =
            crate::gateway_test_state(db.a.clone(), Arc::new(templates(&template, &catalog)))
                .unwrap();
        let client_id = OAuthClientId::parse("managed-one").unwrap();
        assert!(
            GatewayState::new(db.a.clone())
                .effective_oauth_client(&catalog, &client_id)
                .await
                .is_err(),
            "unbound resolver admitted OAuth registration"
        );
        assert!(
            state
                .clone()
                .bind_oauth_client_resolver(Arc::new(ManagedOAuthClientResolver::new(
                    db.a.clone(),
                    Arc::new(templates(&template, &catalog)),
                    crate::fixture_claims().0,
                    crate::fixture_claims().1,
                )))
                .is_err(),
            "duplicate resolver replaced current authority"
        );

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
        let verified = token(effective.token_extensions().unwrap());
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
            server: ServerSlug::parse("media").unwrap(),
            tool: LocalToolName::parse("describe_model").unwrap(),
        };
        let other = PolicyTarget::Tool {
            server: ServerSlug::parse("media").unwrap(),
            tool: LocalToolName::parse("run").unwrap(),
        };
        assert!(
            state
                .oauth_action_admitted(&catalog, &subject, GatewayAction::ToolsCall, &allowed)
                .await
                .unwrap()
        );
        assert!(
            !state
                .oauth_action_admitted(&catalog, &subject, GatewayAction::ToolsCall, &other)
                .await
                .unwrap()
        );
        let mut missing = verified.clone();
        missing.extensions = Default::default();
        assert!(
            state
                .resolve_authenticated_subject(&catalog, missing)
                .await
                .is_err()
        );
        let mut old = verified.clone();
        let (registry, key) = crate::fixture_claims();
        let mut binding = old.extensions.get(&key).unwrap().unwrap().clone();
        binding.generation += 1;
        old.extensions = registry.contribute(&key, &binding).unwrap();
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
            .insert(ScopeName::parse("admin:use").unwrap());
        assert!(
            state
                .resolve_authenticated_subject(&catalog, excessive)
                .await
                .is_err()
        );
        AgentRepository::new(db.a.clone())
            .mutate_managed_agent(
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
                .oauth_action_admitted(&catalog, &subject, GatewayAction::ToolsCall, &allowed)
                .await
                .unwrap()
        );
        assert!(
            state
                .oauth_action_admitted(
                    &catalog,
                    &subject,
                    GatewayAction::TasksGet,
                    &PolicyTarget::Gateway
                )
                .await
                .unwrap(),
            "stop preserves observation"
        );
        AgentRepository::new(db.a.clone())
            .mutate_agent_definition(
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
        let collision =
            GatewayCatalog::from_control_plane(plane, crate::catalog_fixture::binding()).unwrap();
        assert!(
            state
                .effective_oauth_client(&collision, &client_id)
                .await
                .is_err(),
            "disabled registration cannot fall back to a colliding static source"
        );
        let static_state = GatewayState::new(db.a.clone())
            .bind_token_extensions(crate::fixture_claims().0)
            .unwrap()
            .bind_oauth_client_resolver(Arc::new(
                veoveo_mcp_gateway::oauth_clients::CatalogOAuthClientResolver,
            ))
            .unwrap();
        let error = static_state
            .resolve_authenticated_subject(&collision, token(effective.token_extensions().unwrap()))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("registration source mismatch"),
            "static-only resolver must refuse a managed token: {error}"
        );
        assert_eq!(
            AgentRepository::new(db.a.clone())
                .managed_agent(&authority, "one")
                .await
                .unwrap()
                .principal,
            instance.principal
        );
    })
    .await
    .expect("managed identity qualification exceeded 180 seconds");
}
