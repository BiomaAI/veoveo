//! Managed registration setup for Knowledge receiver authority checks.
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_agent_runtime::persistence::AgentRepository;
use veoveo_agent_runtime::persistence::{instances::*, *};
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipLevel, deterministic_work_context_id,
};
pub async fn provision(
    store: &PlatformStore,
    plane: &veoveo_mcp_contract::GatewayControlPlane,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
) -> (AgentCatalogAuthority, AgentDefinition, ManagedAgentInstance) {
    let request = identity.request_context.as_ref().unwrap();
    let profile = plane
        .profiles
        .iter()
        .find(|p| p.id == identity.profile)
        .unwrap();
    let authorization_server = plane
        .authorization_servers
        .iter()
        .find(|server| server.id == profile.authorization_server)
        .unwrap();
    let definition = plane
        .work_contexts
        .iter()
        .find(|context| {
            context.id == identity.authority.work_context
                && context.tenant == identity.authority.tenant
        })
        .unwrap();
    assert_eq!(request.access_token.issuer, authorization_server.issuer);
    assert_eq!(request.access_token.audience, profile.protected_resource);
    let actor = store
        .ensure_identity(
            definition.tenant.as_str(),
            "managed-fixture-author",
            "https://idp.example",
            "managed-fixture-author",
            veoveo_platform_store::PrincipalKind::User,
        )
        .await
        .unwrap();
    let context =
        deterministic_work_context_id(definition.tenant.as_str(), definition.id.as_str()).unwrap();
    use veoveo_platform_store::{
        ArtifactGrantSubjectKind, GrantPermission, WorkContextInitialGrantRecord,
        WorkContextMembershipRuleRecord, WorkContextOutputPolicyRecord, WorkContextRecord,
    };
    fn subject(value: &veoveo_types::AccessSubject) -> (ArtifactGrantSubjectKind, String) {
        match value {
            veoveo_types::AccessSubject::Principal(id) => {
                (ArtifactGrantSubjectKind::Principal, id.to_string())
            }
            veoveo_types::AccessSubject::Group(id) => {
                (ArtifactGrantSubjectKind::Group, id.to_string())
            }
        }
    }
    fn level(value: veoveo_types::WorkContextMembershipLevel) -> WorkContextMembershipLevel {
        match value {
            veoveo_types::WorkContextMembershipLevel::Viewer => WorkContextMembershipLevel::Viewer,
            veoveo_types::WorkContextMembershipLevel::Contributor => {
                WorkContextMembershipLevel::Contributor
            }
            veoveo_types::WorkContextMembershipLevel::Custodian => {
                WorkContextMembershipLevel::Custodian
            }
            veoveo_types::WorkContextMembershipLevel::Owner => WorkContextMembershipLevel::Owner,
        }
    }
    let (owner_kind, owner_key) = subject(&definition.output_policy.owner);
    let now = chrono::Utc::now();
    let record = WorkContextRecord {
        id: context.record_id(),
        tenant: actor.tenant_id.record_id(),
        context_key: definition.id.to_string(),
        title: definition.title.clone(),
        policy_revision: definition.policy_revision.to_string(),
        output_policy: WorkContextOutputPolicyRecord {
            owner_kind,
            owner_key,
            initial_grants: definition
                .output_policy
                .initial_grants
                .iter()
                .map(|grant| {
                    let (subject_kind, subject_key) = subject(&grant.subject);
                    WorkContextInitialGrantRecord {
                        subject_kind,
                        subject_key,
                        permission: match grant.level {
                            veoveo_types::AccessLevel::Read => GrantPermission::Read,
                            veoveo_types::AccessLevel::Write => GrantPermission::Write,
                            veoveo_types::AccessLevel::Admin => GrantPermission::Admin,
                        },
                    }
                })
                .collect(),
            classification: definition
                .output_policy
                .classification
                .as_ref()
                .map(ToString::to_string),
            data_labels: definition
                .output_policy
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect(),
        },
        memberships: definition
            .memberships
            .iter()
            .map(|rule| WorkContextMembershipRuleRecord {
                level: level(rule.level),
                principals: rule.principals.iter().map(ToString::to_string).collect(),
                groups: rule.groups.iter().map(ToString::to_string).collect(),
                roles: rule.roles.iter().map(ToString::to_string).collect(),
                oauth_clients: rule.oauth_clients.iter().map(ToString::to_string).collect(),
            })
            .collect(),
        created_at: now,
        updated_at: now,
    };
    store
        .client()
        .query(include_str!("../queries/support/managed/level.surql"))
        .bind(("context", context.record_id()))
        .bind(("record", record))
        .await
        .unwrap()
        .check()
        .unwrap();
    let digest = store
        .artifact_read_context_version(definition.tenant.as_str(), definition.id.as_str())
        .await
        .unwrap()
        .unwrap()
        .digest;
    let membership = level(
        definition
            .membership_for(&request.principal, &request.access_token.oauth_client_id)
            .unwrap(),
    );
    let authority = AgentCatalogAuthority::new(
        actor.tenant_id,
        context,
        actor.principal_id,
        digest,
        membership,
        true,
        100,
    );

    let content = AgentContent {
        model: AgentModelReference {
            id: "approved".into(),
            revision: "b".repeat(64),
        },
        instructions: "Published pilot instructions".into(),
        tools: vec!["knowledge__search".into()],
        budgets: AgentBudgets {
            max_output_tokens: 100,
            max_completion_calls: 2,
            max_tool_calls: 2,
            deadline_seconds: 30,
        },
        execution: AgentExecution::Managed {
            template: "pilot".into(),
            template_revision: "b".repeat(64),
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
                        client_id: request.access_token.oauth_client_id.to_string(),
                        issuer: authorization_server.issuer.to_string(),
                        authorization_server: profile.authorization_server.to_string(),
                        profile: profile.id.to_string(),
                        resource: profile.protected_resource.to_string(),
                        scopes: request
                            .access_token
                            .scopes
                            .iter()
                            .map(ToString::to_string)
                            .collect(),
                        roles: vec!["managed-pilot".into()],
                        membership,
                    },
                    resources: ManagedAgentResources {
                        namespace: "agents".into(),
                        workload: "one".into(),
                        credential_secret: "one-key".into(),
                        volume_claim: "one-memory".into(),
                        template_config_map: "pilot-template".into(),
                        image: format!("registry.test/kernel@sha256:{}", "a".repeat(64)),
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
