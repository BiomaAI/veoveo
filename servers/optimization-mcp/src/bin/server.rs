//! NVIDIA cuOpt-backed Optimization MCP server.

use std::{net::SocketAddr, sync::Arc};

use axum::{Router, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use serde_json::json;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ResourceListObservers, ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer,
    init_server_telemetry, public_allowed_hosts,
};
use veoveo_optimization_mcp::{
    artifacts::ArtifactRepository,
    domain::CUOPT_STABLE_VERSION,
    executor::{ExecutorClient, ExecutorResult},
    problem_store::ProblemStore,
};
use veoveo_task_runtime::{TaskRuntime, TaskRuntimeConfig};

#[path = "server/admin.rs"]
mod admin;
#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/host.rs"]
mod host;
#[path = "server/internal_auth.rs"]
mod internal_auth;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/problems.rs"]
mod problems;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/records.rs"]
mod records;
#[path = "server/service.rs"]
mod service;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::AppState;
use config::Args;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use service::OptimizationMcp;
use task_extension::recover_tasks;

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
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );

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

    let cancellation = tokio_util::sync::CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let auth_state = InternalMcpAuthState {
        verifier: internal_token_verifier,
    };
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(OptimizationMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_internal_mcp,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state,
        authenticate_internal_mcp,
    ));
    let health_state = state.clone();
    let server_router = Router::new()
        .route(
            "/healthz",
            get(move || {
                let state = health_state.clone();
                async move {
                    let response = state.executor.health().await;
                    let ready = response.is_ok_and(|response| {
                        matches!(
                            response.result,
                            ExecutorResult::Health { health }
                                if health.ready
                                    && health.cuopt_version.starts_with(CUOPT_STABLE_VERSION)
                        )
                    });
                    let status = if ready {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::SERVICE_UNAVAILABLE
                    };
                    (
                        status,
                        axum::Json(json!({
                                        "ready": ready,
                                        "gpu_required": true,
                        "cuopt_version": state.executor_health.cuopt_version.clone(),
                        "gpu_uuid": state.executor_health.gpu_uuid.clone(),
                                    })),
                    )
                }
            }),
        )
        .nest("/mcp", mcp_router)
        .nest("/admin", admin_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), server_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-optimization-mcp",
        %address,
        mcp_path = public_endpoint.path("mcp"),
        admin_path = public_endpoint.path("admin"),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
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

    use super::service::SERVER_DOCS;

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
