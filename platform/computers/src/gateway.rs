//! Gateway WebSocket pool for the Computers transport; authorization stays per request.
use rmcp::model::ErrorData;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{OnceCell, RwLock};
use veoveo_mcp_contract::ServerManifest;
use veoveo_mcp_gateway::{
    GatewayCatalog, UpstreamClientKey, upstream_client_builder, upstream_client_key,
};

#[derive(Clone, Debug, Default)]
pub struct ComputersGatewayClientPool {
    clients:
        Arc<RwLock<BTreeMap<UpstreamClientKey, Arc<OnceCell<veoveo_computers_transport::Client>>>>>,
}
impl ComputersGatewayClientPool {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn client(
        &self,
        catalog: &GatewayCatalog,
        server: &ServerManifest,
    ) -> Result<veoveo_computers_transport::Client, ErrorData> {
        let key = upstream_client_key(catalog, server)?;
        let cell = {
            let mut clients = self.clients.write().await;
            clients.retain(|candidate, _| {
                candidate.catalog_revision() == catalog.configuration_sha256()
            });
            clients
                .entry(key)
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };
        cell.get_or_try_init(|| async {
            let builder = upstream_client_builder(catalog, server).await?;
            veoveo_computers_transport::Client::new(builder).map_err(|_| {
                ErrorData::internal_error("failed to build upstream WebSocket client", None)
            })
        })
        .await
        .cloned()
    }
}

#[cfg(test)]
mod tests;

pub mod routes;
