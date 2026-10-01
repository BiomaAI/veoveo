use super::IndexingConfig;
use crate::{
    ServiceError,
    source::{ApprovedCollection, DiscoveryScope},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use veoveo_mcp_contract::{GatewayControlPlane, OAuthEndpointUrl, ProtectedResourceId};
use veoveo_platform_store::PlatformStore;
use veoveo_policy::PolicyCatalog;
use veoveo_types::{ScopeName, Sha256Digest, WorkContextId};

pub(super) struct Selection {
    pub discovery: DiscoveryScope,
    pub token_endpoint: OAuthEndpointUrl,
    pub resource: ProtectedResourceId,
    pub scopes: BTreeSet<ScopeName>,
    pub work_context: WorkContextId,
}

pub(super) async fn select(
    store: &PlatformStore,
    config: &IndexingConfig,
) -> Result<Selection, ServiceError> {
    let revision = store
        .active_gateway_control_revision()
        .await?
        .ok_or(ServiceError::MachineConfiguration)?;
    let plane: GatewayControlPlane = serde_json::from_value(
        serde_json::to_value(revision.control_plane)
            .map_err(|_| ServiceError::MachineConfiguration)?,
    )
    .map_err(|_| ServiceError::MachineConfiguration)?;
    let digest = Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&plane).map_err(|_| ServiceError::MachineConfiguration)?)
            .into(),
    );
    if digest.hex() != revision.sha256 {
        return Err(ServiceError::MachineConfiguration);
    }
    let catalog = PolicyCatalog::new(plane).map_err(|_| ServiceError::MachineConfiguration)?;
    let plane = catalog.control_plane();
    // One tenant has one active index generation and one approved source set.
    let clients = plane
        .oauth_clients
        .iter()
        .filter(|client| {
            client.tenant.as_ref() == Some(&config.tenant) && client.knowledge_indexing.is_some()
        })
        .collect::<Vec<_>>();
    let [client] = clients.as_slice() else {
        return Err(ServiceError::MachineConfiguration);
    };
    if client.id != config.client_id {
        return Err(ServiceError::MachineConfiguration);
    }
    let indexing = client
        .knowledge_indexing
        .as_ref()
        .ok_or(ServiceError::MachineConfiguration)?;
    let profile = plane
        .profiles
        .iter()
        .find(|profile| {
            client
                .allowed_resources
                .contains(&profile.protected_resource)
        })
        .ok_or(ServiceError::MachineConfiguration)?;
    let issuer = plane
        .authorization_servers
        .iter()
        .find(|issuer| issuer.id == profile.authorization_server)
        .ok_or(ServiceError::MachineConfiguration)?;
    let mut collections = std::collections::BTreeMap::new();
    for id in &indexing.collections {
        let source = plane
            .servers
            .iter()
            .find(|source| &source.slug == id.server())
            .ok_or(ServiceError::MachineConfiguration)?;
        let approval = source
            .knowledge
            .iter()
            .find(|approval| &approval.collection == id)
            .ok_or(ServiceError::MachineConfiguration)?;
        collections.insert(
            id.clone(),
            ApprovedCollection {
                scheme: source.uri_scheme.clone(),
                approval: approval.clone(),
            },
        );
    }
    Ok(Selection {
        discovery: DiscoveryScope {
            tenant: config.tenant.clone(),
            control_revision: digest,
            collections,
        },
        token_endpoint: issuer.token_endpoint.clone(),
        resource: profile.protected_resource.clone(),
        scopes: client.allowed_scopes.clone(),
        work_context: client.default_work_context.clone(),
    })
}
