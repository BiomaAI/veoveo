use std::sync::Arc;

use anyhow::Result;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller, PrincipalKind, SubscriptionHub};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

use crate::{
    acquisition::AcquisitionService,
    analytics::MapAnalytics,
    artifacts::ArtifactRepository,
    catalog::{MapCatalog, MapScope},
    geography::GeographyService,
    release_products::ReleaseProducts,
    routes::RouteService,
    routes::valhalla::ValhallaProcess,
};

#[derive(Clone)]
pub struct MapApplication {
    pub tasks: TaskRuntime,
    pub catalog: MapCatalog,
    pub analytics: MapAnalytics,
    pub routes: RouteService,
    pub geography: GeographyService,
    pub acquisitions: Arc<AcquisitionService>,
    pub artifacts: ArtifactRepository,
    pub products: ReleaseProducts,
    pub valhalla_process: ValhallaProcess,
    pub activation: Arc<tokio::sync::Mutex<()>>,
    pub subscriptions: Arc<SubscriptionHub>,
}

impl MapApplication {
    pub async fn scope(&self, identity: &GatewayInternalIdentity) -> Result<MapScope> {
        let tenant_key = identity
            .actor
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        let platform_identity = self
            .tasks
            .platform_store()
            .ensure_identity(
                &tenant_key,
                identity.actor.id.as_str(),
                identity.actor.issuer.as_str(),
                identity.actor.subject.as_str(),
                match identity.actor.kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        Ok(MapScope {
            identity: platform_identity,
        })
    }

    pub async fn scope_from_task_owner(&self, owner: &TaskOwner) -> Result<MapScope> {
        let identity = self
            .tasks
            .platform_store()
            .ensure_identity(
                owner.tenant_key(),
                &owner.principal_key,
                &owner.issuer,
                &owner.subject,
                match owner.principal_kind {
                    veoveo_task_runtime::PrincipalKind::User => StorePrincipalKind::User,
                    veoveo_task_runtime::PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        Ok(MapScope { identity })
    }

    pub fn caller(&self, identity: GatewayInternalIdentity, bearer_token: String) -> PlaneCaller {
        let memberships = identity.actor.group_memberships();
        PlaneCaller {
            bearer_token,
            identity,
            memberships,
        }
    }
}
