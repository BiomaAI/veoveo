//! Privileged fixture setup through the Store's production authoring APIs.
use super::configuration::Configuration;
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_agent_runtime::contract::authoring::runtime_template_revision;
use veoveo_agent_runtime::persistence::AgentRepository;
use veoveo_agent_runtime::persistence::{instances::*, *};
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipLevel, deterministic_work_context_id,
};

pub(super) struct Provisioned {
    pub authority: AgentCatalogAuthority,
    pub instance: ManagedAgentInstance,
}
pub(super) async fn provision(
    store: &PlatformStore,
    config: &Configuration,
) -> Result<Provisioned> {
    let actor = store
        .ensure_identity(
            "fixture",
            "fixture-author",
            "https://idp.invalid",
            "fixture-author",
            veoveo_platform_store::PrincipalKind::User,
        )
        .await?;
    let expected = config
        .plane
        .work_contexts
        .iter()
        .find(|context| context.tenant.as_str() == "fixture" && context.id.as_str() == "mission")
        .context("fixture configuration has no mission Work Context")?;
    let context = deterministic_work_context_id(expected.tenant.as_str(), expected.id.as_str())?;
    let published = store
        .work_context_by_key(actor.tenant_id, expected.id.as_str())
        .await?
        .context("fixture Work Context was not published")?;
    ensure!(
        published.id == context.record_id()
            && published.tenant == actor.tenant_id.record_id()
            && published.context_key == expected.id.as_str()
            && published.title == expected.title
            && published.policy_revision == expected.policy_revision.as_str(),
        "published fixture Work Context differs from installation configuration"
    );
    let version = store
        .artifact_read_context_version(expected.tenant.as_str(), expected.id.as_str())
        .await?
        .context("published fixture Work Context version missing")?;
    ensure!(
        version.policy_revision == published.policy_revision,
        "published fixture Work Context version changed during setup"
    );
    let digest = version.digest;
    let authority = AgentCatalogAuthority::new(
        actor.tenant_id,
        context,
        actor.principal_id,
        digest,
        WorkContextMembershipLevel::Owner,
        true,
        10,
    );
    let content = AgentContent {
        model: AgentModelReference {
            id: config.model.id.to_string(),
            revision: config
                .model
                .revision()
                .as_str()
                .trim_start_matches("sha256:")
                .into(),
        },
        instructions: "Credential recovery fixture; no episode is requested".into(),
        tools: vec![],
        budgets: AgentBudgets {
            max_output_tokens: 64,
            max_completion_calls: 1,
            max_tool_calls: 1,
            deadline_seconds: 30,
        },
        execution: AgentExecution::Managed {
            template: config.template.id.to_string(),
            template_revision: runtime_template_revision(&config.template)
                .as_str()
                .trim_start_matches("sha256:")
                .into(),
            parameters: BTreeMap::new(),
            resource_subscriptions: vec![],
        },
    };
    let draft = AgentRepository::new(store.clone())
        .mutate_agent_definition(
            &authority,
            "recovery",
            Uuid::now_v7(),
            None,
            AgentDefinitionMutation::Create {
                name: "Recovery".into(),
                description: "Owned credential recovery fixture".into(),
                content,
            },
        )
        .await?;
    let definition = AgentRepository::new(store.clone())
        .mutate_agent_definition(
            &authority,
            "recovery",
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
        .await?;
    let workload = format!("agent-{}", Uuid::now_v7().simple());
    AgentRepository::new(store.clone())
        .mutate_managed_agent(
            &authority,
            "recovery",
            Uuid::now_v7(),
            None,
            ManagedAgentMutation::Provision {
                plan: Box::new(ManagedAgentProvision {
                    name: "Recovery".into(),
                    definition_key: "recovery".into(),
                    revision: definition.draft_digest,
                    identity: ManagedAgentIdentity {
                        client_id: "fixture-managed-client".into(),
                        issuer: "https://gateway.invalid/oauth".into(),
                        authorization_server: "fixture-as".into(),
                        profile: "operator".into(),
                        resource: "https://gateway.invalid/mcp/operator".into(),
                        scopes: vec!["operator:use".into()],
                        roles: vec!["fixture-managed".into()],
                        membership: WorkContextMembershipLevel::Contributor,
                    },
                    resources: ManagedAgentResources {
                        namespace: config.namespace.clone(),
                        workload: workload.clone(),
                        credential_secret: format!("{workload}-key"),
                        volume_claim: format!("{workload}-memory"),
                        template_config_map: config.template.workload.config_map.clone(),
                        image: config.template.workload.image.clone(),
                        storage_gib: config.template.workload.storage_gib,
                    },
                }),
            },
            ManagedAgentLimits {
                instances: 1,
                storage_gib: 1,
            },
        )
        .await?;
    let instance = AgentRepository::new(store.clone())
        .managed_agent(&authority, "recovery")
        .await?;
    Ok(Provisioned {
        authority,
        instance,
    })
}
