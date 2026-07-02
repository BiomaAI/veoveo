use anyhow::Result;
use duckdb::{OptionalExt, params};
use veoveo_mcp_contract::{
    GatewayProfileId, GatewayResourceSubscription, PrincipalId, ResourceUri, ServerSlug,
};

use super::GatewayState;

impl GatewayState {
    pub fn record_resource_subscription(
        &self,
        subscription: &GatewayResourceSubscription,
    ) -> Result<()> {
        let subscription_json = serde_json::to_string(subscription)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO gateway_resource_subscriptions (
                profile, owner, upstream_server, resource_uri,
                created_at, updated_at, subscription_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(profile, owner, upstream_server, resource_uri) DO UPDATE SET
                updated_at = excluded.updated_at,
                subscription_json = excluded.subscription_json
            "#,
            params![
                subscription.profile.as_str(),
                subscription.owner.as_str(),
                subscription.upstream_server.as_str(),
                subscription.resource_uri.as_str(),
                subscription.created_at,
                subscription.updated_at,
                subscription_json,
            ],
        )?;
        Ok(())
    }

    pub fn resource_subscription(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
        resource_uri: &ResourceUri,
    ) -> Result<Option<GatewayResourceSubscription>> {
        self.query_subscription(
            r#"
            SELECT subscription_json
            FROM gateway_resource_subscriptions
            WHERE profile = ?1 AND owner = ?2 AND upstream_server = ?3 AND resource_uri = ?4
            "#,
            params![
                profile.as_str(),
                owner.as_str(),
                upstream_server.as_str(),
                resource_uri.as_str()
            ],
        )
    }

    pub fn delete_resource_subscription(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
        resource_uri: &ResourceUri,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"
            DELETE FROM gateway_resource_subscriptions
            WHERE profile = ?1 AND owner = ?2 AND upstream_server = ?3 AND resource_uri = ?4
            "#,
            params![
                profile.as_str(),
                owner.as_str(),
                upstream_server.as_str(),
                resource_uri.as_str()
            ],
        )?;
        Ok(())
    }

    fn query_subscription<P>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Option<GatewayResourceSubscription>>
    where
        P: duckdb::Params,
    {
        let conn = self.conn.lock();
        let subscription_json = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()?;
        Ok(subscription_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }
}
