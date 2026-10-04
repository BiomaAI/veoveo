//! Resolve immutable publications under current use, model and context authority.
use crate::contract::AgentAction;
use crate::contract::authoring as wire;
use axum::http::StatusCode;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::GatewayProfileId;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::agent_management as domain;

use super::{AgentManagementState, Fault, authority, models::ModelConnection, projection};

#[derive(Clone)]
pub struct ResolvedAgent {
    pub id: wire::AgentDefinitionId,
    pub name: String,
    pub revision: veoveo_types::Sha256Digest,
    pub model: ModelConnection,
    pub instructions: String,
    pub tools: Vec<GatewayToolName>,
    pub budgets: wire::Budgets,
}

/// Agent-owned publication facts; consumers choose their own presentation.
pub struct PublishedAgentRevision {
    pub revision: veoveo_types::Sha256Digest,
    pub model: wire::ModelReference,
    pub tools: Vec<GatewayToolName>,
    pub budgets: wire::Budgets,
    pub instructions_digest: veoveo_types::Sha256Digest,
    pub instructions: Option<String>,
    pub published_by: veoveo_platform_store::PrincipalId,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub published_by_name: String,
}
pub struct ExecutableAgent {
    pub id: wire::AgentDefinitionId,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub model: String,
    pub revision: veoveo_types::Sha256Digest,
    pub tools: Vec<GatewayToolName>,
}
pub struct ExecutableAgentCatalog {
    pub items: Vec<ExecutableAgent>,
    pub next: Option<wire::AgentDefinitionId>,
}

impl AgentManagementState {
    pub async fn execution_authority(
        &self,
        profile: &GatewayProfileId,
        subject: &AuthenticatedSubject,
    ) -> Result<domain::AgentCatalogAuthority, StatusCode> {
        let catalog = self.catalog.current();
        let profile = catalog.profile(profile).ok_or(StatusCode::NOT_FOUND)?;
        if !authority::allowed(
            &catalog,
            &profile.id,
            subject,
            AgentAction::AgentDefinitionsUse,
        ) {
            return Err(StatusCode::FORBIDDEN);
        }
        authority::live_session(self, profile, subject)
            .await
            .map_err(|e| e.code())?;
        authority::context(self, subject, &subject.authority.work_context)
            .await
            .map_err(|e| e.code())
    }

    pub async fn resolve(
        &self,
        profile: &GatewayProfileId,
        subject: &AuthenticatedSubject,
        key: &str,
        revision: Option<&str>,
    ) -> Result<ResolvedAgent, StatusCode> {
        let actor = self.execution_authority(profile, subject).await?;
        let executable = self
            .store()
            .agent_executable(&actor, key, revision)
            .await
            .map_err(|e| Fault::from(e).code())?;
        let content =
            projection::public_content(executable.revision.content).map_err(|e| e.code())?;
        if !matches!(content.execution, wire::Execution::Chat) {
            return Err(StatusCode::NOT_FOUND);
        }
        let model = self
            .models
            .iter()
            .find(|model| {
                model.id == content.model.id
                    && model.revision() == content.model.revision
                    && model.permits(
                        crate::gateway::installation::caller_facts(&subject.principal),
                        &subject.authority.work_context,
                    )
                    && model.admits(&content.budgets)
            })
            .ok_or(StatusCode::FORBIDDEN)?
            .clone();
        Ok(ResolvedAgent {
            id: wire::AgentDefinitionId::new(executable.key)
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
            name: executable.name,
            revision: projection::digest(&executable.revision.digest).map_err(|e| e.code())?,
            model,
            instructions: content.instructions,
            tools: content.tools,
            budgets: content.budgets,
        })
    }

    pub async fn revision_view(
        &self,
        profile: &GatewayProfileId,
        subject: &AuthenticatedSubject,
        key: &str,
        revision: &str,
    ) -> Result<PublishedAgentRevision, StatusCode> {
        use sha2::{Digest, Sha256};
        let actor = self.execution_authority(profile, subject).await?;
        let executable = self
            .store()
            .agent_executable(&actor, key, Some(revision))
            .await
            .map_err(|e| Fault::from(e).code())?;
        let content =
            projection::public_content(executable.revision.content).map_err(|e| e.code())?;
        let can_read = authority::allowed(
            &self.catalog.current(),
            profile,
            subject,
            AgentAction::AgentDefinitionsReadContent,
        ) && self
            .store()
            .agent_authored_revision(&actor, key, revision)
            .await
            .is_ok();
        Ok(PublishedAgentRevision {
            revision: projection::digest(revision).map_err(|e| e.code())?,
            model: content.model,
            tools: content.tools,
            budgets: content.budgets,
            instructions_digest: veoveo_types::Sha256Digest::from_hex(hex::encode(Sha256::digest(
                content.instructions.as_bytes(),
            )))
            .expect("SHA256"),
            instructions: can_read.then_some(content.instructions),
            published_by: veoveo_platform_store::PrincipalId::from_uuid(
                projection::uuid(&executable.revision.created_by).map_err(|e| e.code())?,
            ),
            published_at: executable.revision.created_at,
            published_by_name: executable.published_by_name,
        })
    }

    pub async fn executable_catalog(
        &self,
        profile: &GatewayProfileId,
        subject: &AuthenticatedSubject,
        after: Option<&str>,
    ) -> Result<ExecutableAgentCatalog, StatusCode> {
        let actor = self.execution_authority(profile, subject).await?;
        let entries = self
            .store()
            .agent_catalog(&actor, after, 100)
            .await
            .map_err(|e| Fault::from(e).code())?;
        let next = if entries.len() == 100 {
            entries
                .last()
                .map(|v| {
                    wire::AgentDefinitionId::new(v.key.clone())
                        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
                })
                .transpose()?
        } else {
            None
        };
        let mut items = Vec::new();
        for entry in entries {
            if entry.execution_kind != domain::AgentExecutionKind::Chat {
                continue;
            }
            let Some(model) = self.models.iter().find(|m| {
                m.id.as_str() == entry.model.id
                    && m.revision().hex() == entry.model.revision
                    && m.permits(
                        crate::gateway::installation::caller_facts(&subject.principal),
                        &subject.authority.work_context,
                    )
            }) else {
                continue;
            };
            items.push(ExecutableAgent {
                id: wire::AgentDefinitionId::new(entry.key)
                    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
                name: entry.name,
                description: entry.description,
                provider: model.provider.clone(),
                model: model.model.clone(),
                revision: projection::digest(&entry.digest).map_err(|e| e.code())?,
                tools: entry
                    .tools
                    .into_iter()
                    .map(|t| GatewayToolName::new(t).map_err(|_| StatusCode::SERVICE_UNAVAILABLE))
                    .collect::<Result<_, _>>()?,
            });
        }
        Ok(ExecutableAgentCatalog { items, next })
    }
}
