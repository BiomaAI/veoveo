use anyhow::{Context, Result};
use veoveo_mcp_contract::{
    GatewayProfileId, GatewayResourceSubscription, PrincipalId, ResourceUri, ServerSlug,
};
use veoveo_platform_store::{
    GatewayResourceSubscriptionRecord, OpenObject, gateway_resource_subscription_record_id,
};

use super::GatewayState;

impl GatewayState {
    pub async fn record_resource_subscription(
        &self,
        subscription: &GatewayResourceSubscription,
    ) -> Result<()> {
        let id = subscription_record_id(
            &subscription.profile,
            &subscription.owner,
            &subscription.upstream_server,
            &subscription.resource_uri,
        );
        self.platform
            .upsert_gateway_resource_subscription(GatewayResourceSubscriptionRecord {
                id,
                profile: subscription.profile.to_string(),
                owner: subscription.owner.to_string(),
                upstream_server: subscription.upstream_server.to_string(),
                resource_uri: subscription.resource_uri.to_string(),
                created_at: subscription.created_at,
                updated_at: subscription.updated_at,
                payload: serialize_subscription(subscription)?,
            })
            .await
            .context("failed to persist gateway resource subscription")
    }

    pub async fn resource_subscription(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
        resource_uri: &ResourceUri,
    ) -> Result<Option<GatewayResourceSubscription>> {
        self.platform
            .gateway_resource_subscription(subscription_record_id(
                profile,
                owner,
                upstream_server,
                resource_uri,
            ))
            .await
            .context("failed to read gateway resource subscription")?
            .map(|record| {
                serde_json::from_value(serde_json::to_value(record.payload)?).map_err(Into::into)
            })
            .transpose()
    }

    pub async fn delete_resource_subscription(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
        resource_uri: &ResourceUri,
    ) -> Result<()> {
        self.platform
            .delete_gateway_resource_subscription(subscription_record_id(
                profile,
                owner,
                upstream_server,
                resource_uri,
            ))
            .await
            .context("failed to delete gateway resource subscription")
    }
}

fn subscription_record_id(
    profile: &GatewayProfileId,
    owner: &PrincipalId,
    upstream_server: &ServerSlug,
    resource_uri: &ResourceUri,
) -> veoveo_platform_store::RecordId {
    gateway_resource_subscription_record_id(
        profile.as_str(),
        owner.as_str(),
        upstream_server.as_str(),
        resource_uri.as_str(),
    )
}

fn serialize_subscription(subscription: &GatewayResourceSubscription) -> Result<OpenObject> {
    let value = serde_json::to_value(subscription)?;
    let serde_json::Value::Object(object) = value else {
        anyhow::bail!("gateway resource subscription did not serialize as an object");
    };
    Ok(OpenObject::new(object.into_iter().collect()))
}
