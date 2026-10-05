use super::AgentRepository;
use veoveo_platform_store::{InvocationAuthorityRecord, StoreError, deterministic_tenant_id};

impl AgentRepository {
    pub async fn automated_authority_for_oauth_client(
        &self,
        tenant_key: &str,
        context_key: &str,
        oauth_client: &str,
    ) -> Result<Option<InvocationAuthorityRecord>, StoreError> {
        let tenant_id = deterministic_tenant_id(tenant_key)?;
        let Some(context) = self
            .store
            .work_context_by_key(tenant_id, context_key)
            .await?
        else {
            return Ok(None);
        };
        if let Some(managed) = self
            .managed_agent_registration(oauth_client)
            .await
            .map_err(|_| StoreError::AdministrationFailed {
                operation: "managed authority resolution",
            })?
        {
            let mut response = self
                .client()
                .query(include_str!(
                    "queries/authority/automated_authority_for_oauth_client.surql"
                ))
                .bind(("client", oauth_client.to_owned()))
                .await?
                .check()?;
            let installed: Vec<i64> = response.take(0)?;
            if installed.first().copied().unwrap_or(0) != 0
                || !managed.enabled
                || managed.tenant_key != tenant_key
                || managed.context_key != context_key
            {
                return Ok(None);
            }
            return Ok(Some(
                context.automated_authority(managed.instance.identity.membership),
            ));
        }
        Ok(context
            .membership_for_oauth_client(oauth_client)
            .map(|membership| context.automated_authority(membership)))
    }
}
