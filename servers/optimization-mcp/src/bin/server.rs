//! NVIDIA cuOpt-backed Optimization MCP server.

use std::{net::SocketAddr, sync::Arc};

use clap::Parser;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, ResourceListObservers, SubscriptionHub, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_optimization_mcp::{
    artifacts::ArtifactRepository,
    contract::CUOPT_STABLE_VERSION,
    executor::{ExecutorClient, ExecutorResult},
    problem_store::ProblemStore,
};
use veoveo_task_runtime::{DurableTasks, TaskRuntime, TaskRuntimeConfig};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/problems.rs"]
mod problems;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/service.rs"]
mod service;
#[path = "server/setup.rs"]
mod setup;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::AppState;
use config::Args;
use service::{OptimizationMcp, OptimizationSubscriptions};
use task_extension::{OptimizationTaskExtension, recover_tasks};

const SERVER_SLUG: &str = "optimization";

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard = init_server_telemetry(
        "veoveo-optimization-mcp",
        "info,veoveo_optimization_mcp=debug",
    )?;
    let args = Args::parse();
    std::sync::LazyLock::force(&setup::SERVER_SETUP);
    let public_deployment = args.public_deployment()?;

    let executor = ExecutorClient::new(args.executor_socket.clone(), args.max_executor_frame_bytes);
    let executor_health = match executor.health().await?.result {
        ExecutorResult::Health { health }
            if health.ready && health.cuopt_version.starts_with(CUOPT_STABLE_VERSION) =>
        {
            health
        }
        ExecutorResult::Health { health } => anyhow::bail!(
            "cuOpt executor is not ready or has unsupported version {}",
            health.cuopt_version
        ),
        ExecutorResult::Error { error } => {
            anyhow::bail!("cuOpt executor health failed: {}", error.message)
        }
        other => anyhow::bail!("cuOpt executor returned unexpected health result {other:?}"),
    };
    tracing::info!(
        gpu_name = executor_health.gpu_name,
        gpu_uuid = executor_health.gpu_uuid,
        compute_capability = executor_health.compute_capability,
        cuopt_version = executor_health.cuopt_version,
        "connected to mandatory cuOpt GPU executor"
    );

    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let recovery = tasks.recover().await?;
    let state = Arc::new(AppState {
        tasks,
        artifacts: ArtifactRepository::new(args.artifact_service_url.clone()),
        executor,
        executor_health,
        executor_slot: Arc::new(tokio::sync::Semaphore::new(1)),
        problem_store: ProblemStore::open(
            args.optimization_workspace.clone(),
            args.max_prepared_problem_bytes,
        )?,
        subscriptions: Arc::new(SubscriptionHub::new()),
        resource_observers: Arc::new(ResourceListObservers::new()),
        max_artifact_bytes: args.max_artifact_bytes,
        max_executor_frame_bytes: args.max_executor_frame_bytes,
    });
    recover_tasks(state.clone(), recovery.resumable).await?;

    let observer_state = state.clone();
    let readiness_state = state.clone();
    let server = HostedServer::for_domain::<OptimizationMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || {
            Hosted::new(OptimizationMcp::new(state.clone())).with_tasks(
                DurableTasks::with_resources(
                    OptimizationTaskExtension::new(state.clone()),
                    OptimizationSubscriptions::new(state.clone()),
                ),
            )
        })
        // Optimization serves only while the cuOpt GPU executor reports ready.
        .readiness(move || {
            let state = readiness_state.clone();
            async move {
                state.executor.health().await.is_ok_and(|response| {
                    matches!(
                        response.result,
                        ExecutorResult::Health { health }
                            if health.ready
                                && health.cuopt_version.starts_with(CUOPT_STABLE_VERSION)
                    )
                })
            }
        })
        .build();
    let _resource_observer =
        app_state::spawn_resource_observer(observer_state, server.cancellation_token());
    server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn all_five_tools_use_canonical_schemas() {
        let tools = OptimizationMcp::tool_definitions();
        assert_eq!(tools.len(), 5);
        assert!(tools.iter().all(|tool| !tool.name.is_empty()));
    }
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::setup::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "optimization");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
    }

    #[test]
    fn contract_declaration_matches_the_cuopt_surface() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "optimization");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
