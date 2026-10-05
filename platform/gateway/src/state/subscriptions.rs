use anyhow::{Context, Result};
use veoveo_mcp_contract::{GatewayProfileId, GatewayResourceSubscription, ServerSlug};
use veoveo_platform_store::{
    GatewayResourceSubscriptionRecord, gateway_resource_subscription_record_id,
};
use veoveo_types::{PrincipalId, ResourceUri};

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
                profile: subscription.profile.clone(),
                owner: subscription.owner.clone(),
                upstream_server: subscription.upstream_server.clone(),
                resource_uri: subscription.resource_uri.clone(),
                created_at: subscription.created_at,
                updated_at: subscription.updated_at,
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
                Ok(GatewayResourceSubscription {
                    profile: record.profile,
                    owner: record.owner,
                    upstream_server: record.upstream_server,
                    resource_uri: record.resource_uri,
                    created_at: record.created_at,
                    updated_at: record.updated_at,
                })
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
