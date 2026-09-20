//! Resolve immutable publications under current use, model and context authority.
use axum::http::StatusCode;
use veoveo_mcp_contract::{
    GatewayAction, GatewayProfileId, GatewayToolName, agent_management as wire, workspace,
};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{agent_management as domain, workspace::WorkspaceAgentAdmission};

use super::{AgentManagementState, Fault, authority, models::ModelConnection, projection};

#[derive(Clone)]
pub(crate) struct ResolvedAgent {
    pub id: String,
    pub name: String,
    pub revision: String,
    pub model: ModelConnection,
    pub instructions: String,
    pub tools: Vec<GatewayToolName>,
    pub budgets: wire::Budgets,
}
impl ResolvedAgent {
    pub fn admission(&self) -> WorkspaceAgentAdmission {
        WorkspaceAgentAdmission {
            definition: self.id.clone(),
            definition_digest: self.revision.clone(),
            display_name: self.name.clone(),
            provider: self.model.provider.clone(),
            model: self.model.model.clone(),
        }
    }
}

impl AgentManagementState {
    pub(crate) async fn execution_authority(
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
            GatewayAction::AgentDefinitionsUse,
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

    pub(crate) async fn resolve(
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
                    && model.permits(&subject.principal, &subject.authority.work_context)
                    && model.admits(&content.budgets)
            })
            .ok_or(StatusCode::FORBIDDEN)?
            .clone();
        Ok(ResolvedAgent {
            id: executable.key,
            name: executable.name,
            revision: executable.revision.digest,
            model,
            instructions: content.instructions,
            tools: content.tools,
            budgets: content.budgets,
        })
    }

    pub(crate) async fn revision_view(
        &self,
        profile: &GatewayProfileId,
        subject: &AuthenticatedSubject,
        key: &str,
        revision: &str,
    ) -> Result<workspace::AgentRevisionView, StatusCode> {
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
            GatewayAction::AgentDefinitionsReadContent,
        ) && self
            .store()
            .agent_authored_revision(&actor, key, revision)
            .await
            .is_ok();
        Ok(workspace::AgentRevisionView {
            revision: projection::digest(revision).map_err(|e| e.code())?,
            model: content.model,
            tools: content.tools,
            budgets: content.budgets,
            instructions_digest: veoveo_mcp_contract::Sha256Digest::from_hex(hex::encode(
                Sha256::digest(content.instructions.as_bytes()),
            ))
            .expect("SHA256"),
            instructions: can_read.then_some(content.instructions),
            published_by: workspace::PersonId(
                projection::uuid(&executable.revision.created_by).map_err(|e| e.code())?,
            ),
            published_at: executable.revision.created_at,
            published_by_name: executable.published_by_name,
        })
    }

    pub(crate) async fn executable_catalog(
        &self,
        profile: &GatewayProfileId,
        subject: &AuthenticatedSubject,
        after: Option<&str>,
    ) -> Result<workspace::AgentCatalogPage, StatusCode> {
        let actor = self.execution_authority(profile, subject).await?;
        let entries = self
            .store()
            .agent_catalog(&actor, after, 100)
            .await
            .map_err(|e| Fault::from(e).code())?;
        let next = if entries.len() == 100 {
            entries.last().map(|v| v.key.clone())
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
                    && m.permits(&subject.principal, &subject.authority.work_context)
            }) else {
                continue;
            };
            items.push(workspace::AgentDefinition {
                id: entry.key,
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
        Ok(workspace::AgentCatalogPage { items, next })
    }
}
