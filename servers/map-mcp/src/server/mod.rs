pub(super) mod auth;
mod bootstrap;
mod config;
pub(crate) mod tasks;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use clap::Parser;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, SubscriptionHub, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_task_runtime::{DurableTasks, TaskRecoveryObserver, TaskRuntime, TaskRuntimeConfig};

use crate::{
    acquisition::{
        AcquisitionHelper, AcquisitionHelperConfig, AcquisitionService, AcquisitionServiceConfig,
    },
    analytics::{MapAnalytics, MapAnalyticsConfig},
    artifacts::ArtifactRepository,
    authoring::AuthoringService,
    catalog::MapCatalog,
    geography::GeographyService,
    mcp::{MapMcp, MapSubscriptions},
    release_products::{ReleaseProductConfig, ReleaseProducts},
    routes::{
        RouteService,
        valhalla::{
            ValhallaClient, ValhallaClientConfig, ValhallaPlanner, ValhallaProcess,
            ValhallaProcessConfig,
        },
    },
    spatial::SpatialService,
    state::MapApplication,
};

use config::{Args, Cli};
use tasks::{MapTaskExtension, recover_tasks};

const SERVER_SLUG: &str = "map";

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    match Cli::parse() {
        Cli::BootstrapValidate { path } => bootstrap::run_validate(&path).await,
        Cli::Serve(args) => serve(*args).await,
    }
}

async fn serve(args: Args) -> Result<()> {
    std::sync::LazyLock::force(&crate::mcp::setup::SERVER_SETUP);
    let workspace_app = veoveo_mcp_apps_extension::AppHtml::load(&args.workspace_app)?;
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-map-mcp", "info,veoveo_map_mcp=debug")?;
    let public_deployment = args.public_deployment()?;
    let workspace_basemap = args.workspace_basemap()?;
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
    let tasks = crate::task_lookup::bind(tasks)?;
    let recovery = tasks.observe_startup_recovery().await?;

    let catalog = MapCatalog::new(tasks.platform_store().clone());
    if let Some(path) = &args.bootstrap_catalog {
        bootstrap::apply(path, &catalog).await?;
    }
    let analytics = MapAnalytics::open(MapAnalyticsConfig {
        database_path: args.map_database.clone(),
        authoring_task_root: args.authoring_task_root.clone(),
        spill_dir: args.duckdb_spill_dir.clone(),
        spatial_extension: args.spatial_extension.clone(),
        memory_limit: args.duckdb_memory_limit.clone(),
        threads: args.duckdb_threads,
    })?;
    analytics.verify_spatial()?;
    let authoring = AuthoringService::new(catalog.store().clone(), analytics.clone());
    authoring.reconcile_projection().await?;
    let authoring_task_root = args.authoring_task_root.canonicalize()?;
    let valhalla_client = ValhallaClient::new(ValhallaClientConfig {
        base_url: args.valhalla_url.clone(),
        timeout: Duration::from_secs(args.valhalla_timeout_seconds),
    })?;
    let valhalla_process = ValhallaProcess::start(
        ValhallaProcessConfig {
            executable: args.valhalla_executable.clone(),
            config_file: args.valhalla_config.clone(),
            concurrency: args.valhalla_concurrency,
            startup_timeout: Duration::from_secs(args.valhalla_startup_timeout_seconds),
        },
        &valhalla_client,
    )
    .await?;
    let routes = RouteService::new(
        catalog.clone(),
        analytics.clone(),
        ValhallaPlanner::new(valhalla_client.clone()),
    );
    let artifacts = ArtifactRepository::new(args.artifact_service_url.clone());
    let products = ReleaseProducts::new(
        ReleaseProductConfig {
            release_root: args.release_root.clone(),
            valhalla_active_dir: args.valhalla_active_dir.clone(),
            maximum_routing_expanded_bytes: args.max_routing_expanded_bytes,
        },
        analytics.clone(),
    )?;
    let helper = AcquisitionHelper::new(AcquisitionHelperConfig {
        python_executable: args.helper_python.clone(),
        module: args.helper_module.clone(),
        maximum_output_bytes: args.max_artifact_bytes,
    })?;
    let acquisitions = Arc::new(AcquisitionService::new(
        AcquisitionServiceConfig {
            scratch_root: args.acquisition_scratch_root.clone(),
            mount_root: args.source_mount_root.clone(),
            secret_root: args.source_secret_root.clone(),
            maximum_artifact_bytes: args.max_artifact_bytes,
        },
        catalog.clone(),
        helper,
        artifacts.clone(),
        products.clone(),
    )?);
    let raster = crate::raster::RasterService::new(crate::raster::RasterServiceConfig {
        python_executable: args.helper_python.clone(),
        module: args.raster_helper_module.clone(),
        maximum_output_bytes: args.max_artifact_bytes,
        timeout: Duration::from_secs(args.raster_operation_timeout_seconds),
    })?;
    let feature_packages = crate::feature_packages::FeaturePackageService::new(
        crate::feature_packages::FeaturePackageServiceConfig {
            python_executable: args.helper_python.clone(),
            module: args.feature_package_helper_module.clone(),
            maximum_output_bytes: args.max_artifact_bytes,
            timeout: Duration::from_secs(args.feature_package_timeout_seconds),
        },
    )?;
    let state = Arc::new(MapApplication {
        workspace_basemap,
        tasks,
        catalog: catalog.clone(),
        analytics: analytics.clone(),
        authoring,
        routes,
        geography: GeographyService::new(catalog.clone(), analytics.clone()),
        raster,
        feature_packages,
        spatial: SpatialService::new(catalog.clone()),
        acquisitions,
        artifacts,
        products,
        valhalla_process: valhalla_process.clone(),
        activation: Arc::new(tokio::sync::Mutex::new(())),
        subscriptions: Arc::new(SubscriptionHub::new()),
        authoring_task_root,
        max_artifact_bytes: args.max_artifact_bytes,
    });
    let recovery_state = state.clone();
    let recovery_observer = TaskRecoveryObserver::start(recovery, move |report| {
        let state = recovery_state.clone();
        async move { recover_tasks(state, report.resumable).await }
    })
    .await?;

    let observer_hub = state.subscriptions.clone();
    let observer_store = catalog.store().clone();
    let (health_analytics, health_valhalla, health_process) = (
        analytics.clone(),
        valhalla_client.clone(),
        valhalla_process.clone(),
    );
    let engine_health = move || {
        let (analytics, valhalla, process) = (
            health_analytics.clone(),
            health_valhalla.clone(),
            health_process.clone(),
        );
        async move {
            let spatial = tokio::task::spawn_blocking(move || analytics.verify_spatial())
                .await
                .is_ok_and(|result| result.is_ok());
            spatial
                && process.exited().await.is_ok_and(|exited| !exited)
                && valhalla.health().await.is_ok()
        }
    };
    let readiness_store = catalog.store().clone();
    let server = HostedServer::for_domain::<MapMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || {
            let server = MapMcp::new(state.clone(), workspace_app.clone());
            Hosted::new(server.clone()).with_tasks(DurableTasks::with_resources(
                MapTaskExtension::new(state.clone()),
                MapSubscriptions::new(server),
            ))
        })
        // A failed spatial engine or an exited routing process needs a restart.
        .liveness(engine_health.clone())
        .readiness(move || {
            let store = readiness_store.clone();
            let engine = engine_health();
            async move {
                matches!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        engine.await && store.healthcheck().await.is_ok()
                    })
                    .await,
                    Ok(true)
                )
            }
        })
        .build();
    let shutdown = server.cancellation_token();
    let observer = tokio::spawn(crate::resource_changes::observe(
        observer_store,
        observer_hub,
        server.cancellation_token(),
    ));
    let serve_result = recovery_observer
        .serve(server.serve(SocketAddr::from(([0, 0, 0, 0], args.port))))
        .await;
    shutdown.cancel();
    let observer_result = observer.await;
    valhalla_process.stop().await;
    observer_result?;
    serve_result
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
