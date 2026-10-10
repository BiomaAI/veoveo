//! Existing hosted Time authority fixture shared by owner HTTP controls.
use super::*;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};
pub(super) struct Fixture {
    pub gateway: TestGateway,
    pub state: Arc<crate::state::TimeApplication>,
    pub files: AuthorityFiles,
    pub db: crate::test_store::TestDb,
}
impl Fixture {
    pub async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.clone());
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: files.root.path().join("scratch"),
                release_root: files.root.path().join("releases"),
                zic_executable: files.root.path().join("unavailable-zic"),
                maximum_source_bytes: 1024,
                maximum_expanded_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
            catalog.clone(),
        )
        .unwrap();
        let state = Arc::new(crate::state::TimeApplication {
            tasks: veoveo_task_runtime::TaskRuntime::new(db.a.clone(), "time", "strict-input"),
            catalog,
            authorities: files.registry(),
            clock: crate::clock::ClockMonitor::new(
                crate::clock::ClockSource::System,
                Duration::from_secs(1),
            ),
            acquisitions: Arc::new(acquisitions),
            subscriptions: Arc::new(veoveo_mcp_contract::SubscriptionHub::new()),
            event_watchers: Arc::default(),
        });
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .handler(move || Hosted::new(crate::mcp::TimeMcp::new(handler.clone())))
                .admin_routes(crate::admin::router(state.clone()))
                .build(),
        );
        Self {
            gateway,
            state,
            files,
            db,
        }
    }
}
pub(super) fn token(tenant: &str) -> String {
    let mut principal = testing::principal();
    principal.tenant = Some(tenant.parse().unwrap());
    for scope in TimeScope::ALL {
        principal.scopes.insert((*scope).into());
    }
    let mut authority = testing::authority();
    authority.tenant = tenant.parse().unwrap();
    veoveo_mcp_contract::GatewayInternalTokenIssuer::new(
        veoveo_mcp_contract::TokenIssuer::parse(veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER)
            .unwrap(),
        testing::signing_key("test-key"),
    )
    .issue(
        "operations".parse().unwrap(),
        "time".parse().unwrap(),
        principal,
        authority,
        None,
        Utc::now() + chrono::TimeDelta::minutes(5),
    )
    .unwrap()
    .bearer_token
}
