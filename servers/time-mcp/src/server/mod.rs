pub(super) mod auth;
mod bootstrap;
mod config;
pub(crate) mod tasks;

use std::{collections::BTreeMap, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use axum::middleware;
use clap::Parser;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, SubscriptionHub, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_task_runtime::{DurableTasks, TaskRuntime, TaskRuntimeConfig};

use crate::{
    acquisition::{AcquisitionService, AcquisitionServiceConfig},
    admin,
    authority::{AuthorityContext, LeapSecondTable},
    catalog::TimeCatalog,
    clock::{ClockMonitor, ClockSource},
    contract::{AuthorityDatasetKind, EffectiveTimeAuthority},
    mcp::{TimeMcp, TimeSubscriptions},
    registry::AuthorityRegistry,
    state::TimeApplication,
};

use auth::{AdminAuthState, authorize_admin};
use config::Args;
use tasks::{TimeTaskExtension, recover_tasks};

const SERVER_SLUG: &str = "time";

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-time-mcp", "info,veoveo_time_mcp=debug")?;
    let args = Args::parse();
    std::sync::LazyLock::force(&crate::mcp::SERVER_SETUP);
    let public_deployment = args.public_deployment()?;
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
    let catalog = TimeCatalog::new(tasks.platform_store().clone());
    let leap_seconds = LeapSecondTable::from_path(&args.bootstrap_leap_seconds_file).await?;
    let bootstrap = AuthorityContext::from_paths(
        EffectiveTimeAuthority::new(
            bootstrap::reference(AuthorityDatasetKind::Tzdb, &args.bootstrap_tzdb_source_file)
                .await?,
            bootstrap::reference(
                AuthorityDatasetKind::LeapSeconds,
                &args.bootstrap_leap_seconds_file,
            )
            .await?,
        )?,
        &args.bootstrap_tzdb_dir,
        leap_seconds,
    )?;
    let authorities = AuthorityRegistry::new(
        bootstrap,
        args.bootstrap_tzdb_dir.clone(),
        args.bootstrap_leap_seconds_file.clone(),
    );
    let clock = ClockMonitor::new(
        args.ntpd_observation_socket
            .clone()
            .map_or(ClockSource::System, |observation_socket| {
                ClockSource::NtpdRs { observation_socket }
            }),
        Duration::from_secs(args.clock_observation_timeout_seconds),
    );
    let acquisitions = Arc::new(AcquisitionService::new(
        AcquisitionServiceConfig {
            scratch_root: args.acquisition_scratch_root.clone(),
            release_root: args.release_root.clone(),
            zic_executable: args.zic_executable.clone(),
            maximum_source_bytes: args.maximum_source_bytes,
            maximum_expanded_bytes: args.maximum_expanded_bytes,
            timeout: Duration::from_secs(args.acquisition_timeout_seconds),
        },
        catalog.clone(),
    )?);
    let state = Arc::new(TimeApplication {
        tasks,
        catalog,
        authorities,
        clock,
        acquisitions,
        subscriptions: Arc::new(SubscriptionHub::new()),
        event_watchers: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
    });
    recover_tasks(state.clone(), recovery.resumable).await?;

    let resource_state = state.clone();
    let _resource_observer = tokio::spawn(async move {
        use crate::TimeObservationTable::*;
        use futures::StreamExt;
        let mut changes = resource_state.tasks.platform_store().resource_changes(vec![
            TimeSource,
            TimeAuthorityRelease,
            TimeActiveAuthority,
            TimeAcquisition,
            TimeCalendarVersion,
            TimeMissionEpoch,
            TimeTemporalEvent,
            TimeClockPolicy,
        ]);
        while changes.next().await.is_some() {
            resource_state.authorities.invalidate().await;
            resource_state
                .subscriptions
                .notify_resource_contents_changed()
                .await;
        }
    });
    let admin = admin::router(state.clone()).layer(middleware::from_fn_with_state(
        AdminAuthState {
            required_scope: args.admin_scope.clone(),
        },
        authorize_admin,
    ));
    let clock_state = state.clone();
    let server = HostedServer::for_domain::<TimeMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || {
            Hosted::new(TimeMcp::new(state.clone())).with_tasks(DurableTasks::with_resources(
                TimeTaskExtension::new(state.clone()),
                TimeSubscriptions::new(state.clone()),
            ))
        })
        .admin_routes(admin)
        // Time serves only while it observes a clock.
        .readiness(move || {
            let state = clock_state.clone();
            async move { state.clock.quality().await.is_ok() }
        })
        .build();
    server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
