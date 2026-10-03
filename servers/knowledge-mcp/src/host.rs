//! The hosted Knowledge server. Domain handlers recheck current authority.
use crate::{
    embed::Embeddings,
    indexing::IndexingReadiness,
    mcp::{KnowledgeListener, KnowledgeMcp},
};
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, PublicDeployment,
    hosting::{Hosted, HostedServer, ListenOnly},
};

/// Builds the hosted Knowledge server for `deployment`, trusting `trust`, and
/// starts its catalog observer, which stops with the server.
pub fn server<E: Embeddings + 'static>(
    server: KnowledgeMcp<E>,
    deployment: &PublicDeployment,
    allow_loopback_hosts: bool,
    allowed_hosts: Vec<String>,
    trust: GatewayInternalTrustBundle,
    readiness: IndexingReadiness,
) -> anyhow::Result<HostedServer> {
    std::sync::LazyLock::force(&crate::mcp::SETUP);
    let store = server.store.clone();
    let observer = server.clone();
    let hosted = HostedServer::for_domain::<KnowledgeMcp<E>>()
        .deployment(deployment, allow_loopback_hosts)?
        .allowed_hosts(allowed_hosts)
        .internal_trust(trust)?
        .handler(move || {
            Hosted::new(server.clone()).with_tasks(ListenOnly::new(KnowledgeListener {
                server: server.clone(),
            }))
        })
        // Ready once indexing reports ready and the gateway control plane is active.
        .readiness(move || {
            let (store, readiness) = (store.clone(), readiness.clone());
            async move {
                readiness.is_ready()
                    && tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        store.active_gateway_control_revision(),
                    )
                    .await
                    .is_ok_and(|result| result.is_ok_and(|revision| revision.is_some()))
            }
        })
        .build();
    observer.observe_catalog(hosted.cancellation_token());
    Ok(hosted)
}
