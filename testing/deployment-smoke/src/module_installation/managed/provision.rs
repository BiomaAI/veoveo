//! Privileged fixture setup through the Store's production authoring APIs.
use super::configuration::Configuration;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_agent_runtime::contract::authoring::runtime_template_revision;
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipLevel, agent_management::instances::*, agent_management::*,
    deterministic_work_context_id,
};

pub(super) struct Provisioned {
    pub authority: AgentCatalogAuthority,
    pub instance: ManagedAgentInstance,
}
pub(super) async fn provision(
    store: &PlatformStore,
    config: &Configuration,
) -> Result<Provisioned> {
    use veoveo_platform_store::{
        ArtifactGrantSubjectKind, WorkContextMembershipRuleRecord, WorkContextOutputPolicyRecord,
        WorkContextRecord,
    };
    let actor = store
        .ensure_identity(
            "fixture",
            "fixture-author",
            "https://idp.invalid",
            "fixture-author",
            veoveo_platform_store::PrincipalKind::User,
        )
        .await?;
    let context = deterministic_work_context_id("fixture", "mission")?;
    let now = chrono::Utc::now();
    let record = WorkContextRecord {
        id: context.record_id(),
        tenant: actor.tenant_id.record_id(),
        context_key: "mission".into(),
        title: "Mission".into(),
        policy_revision: "policy-fixture".into(),
        output_policy: WorkContextOutputPolicyRecord {
            owner_kind: ArtifactGrantSubjectKind::Group,
            owner_key: "operations".into(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![WorkContextMembershipRuleRecord {
            level: WorkContextMembershipLevel::Contributor,
            principals: vec![],
            groups: vec!["operations".into()],
            roles: vec!["fixture-managed".into()],
            oauth_clients: vec![],
        }],
        created_at: now,
        updated_at: now,
    };
    store
        .client()
        .query("CREATE ONLY $context CONTENT $record;")
        .bind(("context", context.record_id()))
        .bind(("record", record))
        .await?
        .check()?;
    let digest = store
        .artifact_read_context_version("fixture", "mission")
        .await?
        .context("fixture Work Context version")?
        .digest;
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
    let draft = store
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
    let definition = store
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
    store
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
    let instance = store.managed_agent(&authority, "recovery").await?;
    Ok(Provisioned {
        authority,
        instance,
    })
}
