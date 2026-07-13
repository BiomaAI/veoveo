use std::{collections::HashMap, sync::Arc};

use rmcp::model::ProgressToken;
use tokio::sync::RwLock;
use veoveo_mcp_contract::{GatewayProfileId, PrincipalId, ServerSlug};

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
            GatewayProgressMapping { downstream_token },
        );
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
}

#[cfg(test)]
mod tests {
    use rmcp::model::NumberOrString;

    use super::*;

    fn token(value: i64) -> ProgressToken {
        ProgressToken(NumberOrString::Number(value))
    }

    #[tokio::test]
    async fn progress_tokens_translate_and_remove_by_token() {
        let registry = GatewayProgressTokens::default();
        let profile = GatewayProfileId::new("default").unwrap();
        let principal = PrincipalId::new("issuer#subject").unwrap();
        let server = ServerSlug::new("media").unwrap();

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
            .remove_token(&profile, &principal, &server, &token(1))
            .await;
        assert_eq!(
            registry
                .translate(&profile, &principal, &server, &token(1))
                .await,
            None
        );
    }
}
