//! Installation-bound managed configuration. Authored instructions stay literal.
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use uuid::Uuid;
use veoveo_agent_runtime::ManagedRuntimeBinding;
use veoveo_mcp_contract::{WorkContextId, agent_management as wire};
use veoveo_platform_store::{
    PlatformStore,
    agent_management::{AgentExecution, instances::ManagedAgentRegistration},
};

use crate::manifest::{AgentManifest, ResourceSubscription};

pub struct ManagedKernel {
    pub binding: ManagedRuntimeBinding,
    pub pod_uid: Uuid,
    pub deadline: Duration,
}

impl ManagedKernel {
    /// The manager injects the generation and approved connection. The authoring
    /// API accepts neither environment variables nor provider destinations.
    pub async fn load(store: &PlatformStore, manifest: &mut AgentManifest) -> Result<Option<Self>> {
        let registration = store
            .managed_agent_registration(&manifest.gateway.client_id)
            .await?;
        let generation = std::env::var("VEOVEO_MANAGED_GENERATION").ok();
        let Some(registration) = registration else {
            ensure!(generation.is_none(), "managed registration is missing");
            return Ok(None);
        };
        let generation: i64 = generation
            .context("managed generation is required")?
            .parse()?;
        let pod_uid = std::env::var("VEOVEO_AGENT_POD_UID")?.parse()?;
        let model: wire::ModelConnection =
            serde_json::from_str(&std::env::var("VEOVEO_MANAGED_MODEL")?)?;
        Self::apply(manifest, registration, generation, pod_uid, &model).map(Some)
    }

    fn apply(
        manifest: &mut AgentManifest,
        registration: ManagedAgentRegistration,
        generation: i64,
        pod_uid: Uuid,
        model: &wire::ModelConnection,
    ) -> Result<Self> {
        let instance = &registration.instance;
        let content = &registration.revision.content;
        ensure!(
            registration.enabled && generation > 0 && instance.active_generation == generation,
            "managed generation is unavailable"
        );
        ensure!(
            registration.tenant_key == manifest.agent.tenant
                && instance.key == manifest.agent.id
                && registration.context_key == manifest.gateway.work_context
                && instance.identity.profile == manifest.gateway.profile
                && instance.identity.client_id == manifest.gateway.client_id
                && instance.identity.scopes == manifest.gateway.scopes
                && instance.identity.resource == manifest.gateway.resource,
            "managed manifest identity mismatch"
        );
        ensure!(
            model.id.as_str() == content.model.id
                && model.revision().as_str() == format!("sha256:{}", content.model.revision)
                && model.tenant.as_str() == registration.tenant_key
                && model
                    .work_contexts
                    .contains(&WorkContextId::new(registration.context_key.clone())?)
                && model.required_scopes.iter().all(|scope| instance
                    .identity
                    .scopes
                    .iter()
                    .any(|allowed| allowed == scope.as_str())),
            "managed model connection is unavailable"
        );
        let budgets = wire::Budgets {
            max_output_tokens: content.budgets.max_output_tokens,
            max_completion_calls: content.budgets.max_completion_calls,
            max_tool_calls: content.budgets.max_tool_calls,
            deadline_seconds: content.budgets.deadline_seconds,
        };
        ensure!(model.admits(&budgets), "managed model limits changed");
        let AgentExecution::Managed {
            resource_subscriptions,
            ..
        } = &content.execution
        else {
            anyhow::bail!("managed revision has another execution kind");
        };
        // Expansion happened only in the reviewed installation manifest. Never
        // pass authored instructions or subscription values through env expansion.
        manifest.preamble = content.instructions.clone();
        manifest.agent.display_name = instance.name.clone();
        manifest.model.base_url = model.base_url.clone();
        manifest.model.model = model.model.clone();
        manifest.model.api_key_env = "VEOVEO_MANAGED_MODEL_KEY".into();
        manifest.model.max_output_tokens = Some(u64::from(budgets.max_output_tokens));
        manifest.budgets.per_episode.max_completion_calls =
            Some(u64::from(budgets.max_completion_calls));
        manifest.budgets.per_episode.max_tool_calls = Some(u64::from(budgets.max_tool_calls));
        manifest.episode.request_timeout_s = manifest
            .episode
            .request_timeout_s
            .min(u64::from(budgets.deadline_seconds));
        manifest.resource_subscriptions = resource_subscriptions
            .iter()
            .map(|uri| ResourceSubscription { uri: uri.clone() })
            .collect();
        manifest.validate()?;
        Ok(Self {
            binding: ManagedRuntimeBinding {
                instance: instance.id.clone(),
                generation,
            },
            pod_uid,
            deadline: Duration::from_secs(u64::from(budgets.deadline_seconds)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use veoveo_platform_store::agent_management::{AgentContent, AgentRevision};

    #[test]
    fn managed_overlay_preserves_literal_instructions_and_requires_exact_model_generation() {
        // SAFETY: these test-only values are never read by a model client, and
        // this is the sole test of the managed key environment variable.
        unsafe {
            std::env::set_var("VEOVEO_MANAGED_MODEL_KEY", "fixture-only");
            std::env::set_var("TEST_MANAGED_PRIVATE_KEY", "fixture-only");
        }
        let manifest: AgentManifest = serde_json::from_value(json!({
            "agent":{"tenant":"test","id":"worker","display_name":"Worker"},
            "model":{"base_url":"https://model.test/v1","api_key_env":"VEOVEO_MANAGED_MODEL_KEY","model":"approved"},
            "gateway":{"url":"https://gateway.test","transport_url":"https://gateway.test","profile":"operator","client_id":"worker-client","work_context":"operations",
                "audience":"https://gateway.test/oauth/token","resource":"https://gateway.test/mcp/operator","scopes":["operator:use"],"private_key_env":"TEST_MANAGED_PRIVATE_KEY","private_key_kid":"key"},
            "episode":{},"preamble":"Installation default"
        })).unwrap();
        let model: wire::ModelConnection = serde_json::from_value(json!({
            "id":"approved","name":"Approved","provider":"fixture","tenant":"test","work_contexts":["operations"],"required_scopes":["operator:use"],
            "base_url":"https://model.test/v1","model":"approved","api_key":"fixture-key",
            "limits":{"maxOutputTokens":128,"maxCompletionCalls":2,"maxToolCalls":3,"deadlineSeconds":60}
        })).unwrap();
        let content: AgentContent = serde_json::from_value(json!({
            "model":{"id":"approved","revision":model.revision().as_str().trim_start_matches("sha256:")},
            "instructions":"Keep ${VEOVEO_MANAGED_MODEL_KEY} and ${MISSING_VARIABLE} literal.","tools":[],
            "budgets":{"max_output_tokens":64,"max_completion_calls":1,"max_tool_calls":2,"deadline_seconds":30},
            "execution":{"kind":"managed","template":"approved","template_revision":"a".repeat(64),"parameters":{},"resource_subscriptions":[]}
        })).unwrap();
        let tenant = veoveo_platform_store::deterministic_tenant_id("test")
            .unwrap()
            .record_id();
        let instance_id = veoveo_platform_store::agent_management::instances::managed_agent_record(
            &tenant, "worker",
        )
        .unwrap();
        let definition =
            veoveo_platform_store::agent_management::agent_definition_record(&tenant, "worker")
                .unwrap();
        let principal = veoveo_platform_store::deterministic_principal_id("test", "worker")
            .unwrap()
            .record_id();
        let instance = serde_json::from_value(json!({
            "id":instance_id,"tenant":tenant,"work_context":veoveo_platform_store::deterministic_work_context_id("test","operations").unwrap().record_id(),
            "owner":principal,"deployed_by":principal,"key":"worker","name":"Authored Worker","definition":definition,
            "requested_revision":definition,"active_revision":definition,"generation":1,"active_generation":1,"dispatch_epoch":1,"desired":"running","observed":"workload","principal":principal,
            "identity":{"client_id":"worker-client","issuer":"https://gateway.test/oauth","authorization_server":"gateway","profile":"operator","resource":"https://gateway.test/mcp/operator","scopes":["operator:use"],"roles":[],"membership":"contributor"},
            "resources":{"namespace":"agents","workload":"worker","credential_secret":"worker-key","volume_claim":"worker-memory","template_config_map":"approved","image":"registry.test/kernel@sha256:fixture","storage_gib":1},
            "public_key":null,"operation":instance_id,"created_at":chrono::Utc::now(),"updated_at":chrono::Utc::now()
        })).unwrap();
        let registration = ManagedAgentRegistration {
            instance,
            revision: AgentRevision {
                id: definition.clone(),
                definition,
                digest: "a".repeat(64),
                content,
                created_by: principal,
                created_at: chrono::Utc::now(),
            },
            tenant_key: "test".into(),
            context_key: "operations".into(),
            enabled: true,
        };
        let mut applied = manifest.clone();
        let managed = ManagedKernel::apply(
            &mut applied,
            registration.clone(),
            1,
            Uuid::now_v7(),
            &model,
        )
        .unwrap();
        assert_eq!(
            applied.preamble,
            "Keep ${VEOVEO_MANAGED_MODEL_KEY} and ${MISSING_VARIABLE} literal."
        );
        assert_eq!(applied.model.max_output_tokens, Some(64));
        assert_eq!(managed.deadline, Duration::from_secs(30));
        assert!(
            ManagedKernel::apply(
                &mut manifest.clone(),
                registration.clone(),
                2,
                Uuid::now_v7(),
                &model
            )
            .is_err()
        );
        let mut changed_model = model;
        changed_model.base_url = "https://other.test/v1".into();
        assert!(
            ManagedKernel::apply(
                &mut manifest.clone(),
                registration,
                1,
                Uuid::now_v7(),
                &changed_model
            )
            .is_err()
        );
    }
}
