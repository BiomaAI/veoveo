use std::{collections::HashMap, sync::Arc};

use rmcp::model::{ProgressToken, TaskStatus};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{GatewayProfileId, PrincipalId, ServerSlug, UpstreamTaskId};

#[derive(Debug, Clone, Default)]
pub(super) struct GatewayProgressTokens {
    inner: Arc<RwLock<HashMap<GatewayProgressTokenKey, GatewayProgressMapping>>>,
}

impl GatewayProgressTokens {
    pub(super) async fn register(
        &self,
        profile: &GatewayProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: ProgressToken,
        downstream_token: ProgressToken,
    ) {
        self.inner.write().await.insert(
            GatewayProgressTokenKey::new(profile, principal, upstream_server, upstream_token),
            GatewayProgressMapping {
                downstream_token,
                upstream_task_id: None,
            },
        );
    }

    pub(super) async fn attach_task(
        &self,
        profile: &GatewayProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: &ProgressToken,
        upstream_task_id: UpstreamTaskId,
    ) {
        let mut mappings = self.inner.write().await;
        if let Some(mapping) = mappings.get_mut(&GatewayProgressTokenKey::new(
            profile,
            principal,
            upstream_server,
            upstream_token.clone(),
        )) {
            mapping.upstream_task_id = Some(upstream_task_id);
        }
    }

    pub(super) async fn translate(
        &self,
        profile: &GatewayProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: &ProgressToken,
    ) -> Option<ProgressToken> {
        self.inner
            .read()
            .await
            .get(&GatewayProgressTokenKey::new(
                profile,
                principal,
                upstream_server,
                upstream_token.clone(),
            ))
            .map(|mapping| mapping.downstream_token.clone())
    }

    pub(super) async fn remove_task(
        &self,
        profile: &GatewayProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_task_id: &UpstreamTaskId,
    ) {
        self.inner.write().await.retain(|key, mapping| {
            key.profile != *profile
                || key.principal != *principal
                || key.upstream_server != *upstream_server
                || mapping.upstream_task_id.as_ref() != Some(upstream_task_id)
        });
    }

    pub(super) async fn remove_token(
        &self,
        profile: &GatewayProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: &ProgressToken,
    ) {
        self.inner
            .write()
            .await
            .remove(&GatewayProgressTokenKey::new(
                profile,
                principal,
                upstream_server,
                upstream_token.clone(),
            ));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GatewayProgressTokenKey {
    profile: GatewayProfileId,
    principal: PrincipalId,
    upstream_server: ServerSlug,
    upstream_token: ProgressToken,
}

impl GatewayProgressTokenKey {
    fn new(
        profile: &GatewayProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: ProgressToken,
    ) -> Self {
        Self {
            profile: profile.clone(),
            principal: principal.clone(),
            upstream_server: upstream_server.clone(),
            upstream_token,
        }
    }
}

#[derive(Debug, Clone)]
struct GatewayProgressMapping {
    downstream_token: ProgressToken,
    upstream_task_id: Option<UpstreamTaskId>,
}

pub(super) fn is_terminal(status: &TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    )
}

#[cfg(test)]
mod tests {
    use rmcp::model::NumberOrString;

    use super::*;

    fn token(value: i64) -> ProgressToken {
        ProgressToken(NumberOrString::Number(value))
    }

    #[tokio::test]
    async fn progress_tokens_translate_and_remove_by_task() {
        let registry = GatewayProgressTokens::default();
        let profile = GatewayProfileId::new("default").unwrap();
        let principal = PrincipalId::new("issuer#subject").unwrap();
        let server = ServerSlug::new("media").unwrap();
        let upstream_task_id = UpstreamTaskId::new("upstream-task-1").unwrap();

        registry
            .register(&profile, &principal, &server, token(1), token(99))
            .await;
        assert_eq!(
            registry
                .translate(&profile, &principal, &server, &token(1))
                .await,
            Some(token(99))
        );

        registry
            .attach_task(
                &profile,
                &principal,
                &server,
                &token(1),
                upstream_task_id.clone(),
            )
            .await;
        registry
            .remove_task(&profile, &principal, &server, &upstream_task_id)
            .await;
        assert_eq!(
            registry
                .translate(&profile, &principal, &server, &token(1))
                .await,
            None
        );
    }
}
