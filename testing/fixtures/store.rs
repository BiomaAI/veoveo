//! Shared Rust fixture: exact disposable store, no installation data or credentials.
use std::{sync::Arc, time::Duration};
use veoveo_platform_store::audit::AuditTargetRegistry;
#[path = "store/container.rs"]
mod container;
#[path = "store/io.rs"]
pub mod io;
use container::{Container, Docker};
#[path = "module_lanes.rs"]
pub mod module_lanes;
use uuid::Uuid;
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};

#[allow(
    dead_code,
    reason = "Each fixture consumer selects its storage qualification"
)]
pub enum StoreBackend {
    Memory,
    RocksDb,
}
fn fixture_password() -> String {
    format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple())
}
pub struct TestDb {
    _container: Container,
    #[allow(
        dead_code,
        reason = "Only storage measurements reconnect as fixture admin"
    )]
    admin_config: StoreConfig,
    pub a: PlatformStore,
    #[allow(dead_code, reason = "Only replica fixtures use the second connection")]
    pub b: PlatformStore,
}

impl TestDb {
    /// Storage maintenance is outside the workload and uses fixture administration.
    /// Measured domain operations keep their database-editor credentials.
    #[allow(dead_code, reason = "Only RocksDB measurements request compaction")]
    pub async fn compact(&self) {
        tokio::time::timeout(Duration::from_secs(30), async {
            let admin = PlatformStore::connect(self.admin_config.clone())
                .await
                .unwrap();
            admin
                .client()
                .query(include_str!("queries/store/compact.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
        })
        .await
        .expect("measurement compaction exceeded 30 seconds");
    }

    #[allow(
        dead_code,
        reason = "Only write-cost measurements inspect retained storage"
    )]
    pub async fn storage_bytes(&self) -> u64 {
        self._container.storage_bytes().await.unwrap()
    }

    #[allow(
        dead_code,
        reason = "Only write-cost measurements collect Linux I/O counters"
    )]
    pub async fn io_probe(&self) -> io::IoProbe {
        io::IoProbe::for_process(self._container.local_process_id().await.unwrap())
    }

    /// Replay committed rows from an isolated fixture. Assertions inspect native
    /// domain state; definitions and deletions do not represent a new row state.
    #[allow(
        dead_code,
        reason = "Only transition qualification reads committed rows"
    )]
    pub async fn committed(
        &self,
        table: impl Into<veoveo_platform_store::ObservationTable>,
    ) -> Vec<serde_json::Value> {
        let table = table.into();
        use veoveo_platform_store::{ChangefeedCursor, ChangefeedEntry, decode_changefeed_entry};
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut cursor = ChangefeedCursor::initial();
            let mut rows = Vec::new();
            loop {
                let batches = self.b.replay_changes(cursor, 1_000).await.unwrap();
                let Some(last) = batches.last() else {
                    return rows;
                };
                cursor =
                    ChangefeedCursor::from_versionstamp(last.versionstamp.checked_add(1).unwrap())
                        .unwrap();
                for batch in batches {
                    for value in batch.changes {
                        let change = decode_changefeed_entry(&value).unwrap();
                        if change.table() == Some(table.as_str())
                            && let ChangefeedEntry::Upsert(row) = change
                        {
                            rows.push(row.into_json_value());
                        }
                    }
                }
            }
        })
        .await
        .expect("fixture native replay exceeded 30 seconds")
    }

    /// Isolated administrative session for native privileged grammar qualification.
    #[allow(
        dead_code,
        reason = "Only privileged fixture grammar uses administration"
    )]
    pub async fn admin(&self) -> PlatformStore {
        tokio::time::timeout(
            Duration::from_secs(60),
            PlatformStore::connect(self.admin_config.clone()),
        )
        .await
        .expect("fixture admin connection exceeded60seconds")
        .unwrap()
    }

    /// Reconnect through an owned network fault fixture with the same editor identity.
    #[allow(
        dead_code,
        reason = "Only reconnect qualification replaces its endpoint"
    )]
    pub async fn connect_at(&self, endpoint: &str) -> PlatformStore {
        PlatformStore::connect(self.a.config().clone().with_endpoint(endpoint).unwrap())
            .await
            .unwrap()
    }

    #[allow(
        dead_code,
        reason = "Fixture consumers select memory or RocksDB explicitly"
    )]
    pub async fn new() -> Self {
        Self::with_backend(StoreBackend::Memory).await
    }

    #[allow(
        dead_code,
        reason = "Fixture consumers select memory or RocksDB explicitly"
    )]
    pub async fn with_backend(backend: StoreBackend) -> Self {
        Self::with_backend_and_modules(backend, Vec::new()).await
    }

    #[allow(
        dead_code,
        reason = "Only owner-native fixtures install additional test tables"
    )]
    pub async fn with_backend_and_schema(backend: StoreBackend, schema: &str) -> Self {
        Self::with_setup(
            backend,
            Vec::new(),
            schema,
            Arc::new(AuditTargetRegistry::empty()),
        )
        .await
    }

    #[allow(dead_code, reason = "Only optional-owner fixtures select domain lanes")]
    pub async fn with_modules(modules: Vec<veoveo_modules::ModuleSetup>) -> Self {
        Self::with_backend_and_modules(StoreBackend::Memory, modules).await
    }

    pub async fn with_backend_and_modules(
        backend: StoreBackend,
        modules: Vec<veoveo_modules::ModuleSetup>,
    ) -> Self {
        Self::with_setup(backend, modules, "", Arc::new(AuditTargetRegistry::empty())).await
    }

    #[allow(
        dead_code,
        reason = "Only contextual Audit fixtures install target codecs"
    )]
    pub async fn with_audit_targets(targets: Arc<AuditTargetRegistry>) -> Self {
        Self::with_setup(StoreBackend::Memory, Vec::new(), "", targets).await
    }

    #[allow(
        dead_code,
        reason = "Owners select explicit modules and codecs for native fixtures"
    )]
    pub async fn with_composition(
        backend: StoreBackend,
        modules: Vec<veoveo_modules::ModuleSetup>,
        targets: Arc<AuditTargetRegistry>,
    ) -> Self {
        Self::with_setup(backend, modules, "", targets).await
    }

    async fn with_setup(
        backend: StoreBackend,
        modules: Vec<veoveo_modules::ModuleSetup>,
        schema: &str,
        audit_targets: Arc<AuditTargetRegistry>,
    ) -> Self {
        // Complete SQL/dependency admission precedes database or container effects.
        let selected = modules.iter().map(|module| module.name().clone()).collect();
        let registry = module_lanes::registry(modules).expect("fixture lane declarations");
        let prepared = veoveo_modules::runner::prepare(
            registry
                .select(selected)
                .expect("fixture selected prerequisites"),
        )
        .expect("fixture schema admission");
        let storage = match backend {
            StoreBackend::Memory => "memory",
            StoreBackend::RocksDb => "rocksdb:/tmp/veoveo-test.db",
        };
        let password = fixture_password();
        let (container, endpoint) = Container::start(Docker::default(), storage, &password)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let database = format!("fixture_{}", Uuid::now_v7().simple());
        let admin_credentials = StoreCredentials::root("fixture_admin", password);
        let config = StoreConfig::builder(
            &endpoint,
            "veoveo_fixture",
            &database,
            admin_credentials.clone(),
        )
        .audit_targets(audit_targets.clone())
        .build()
        .unwrap();
        let admin = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                match PlatformStore::connect(config.clone()).await {
                    Ok(store) => break store,
                    // Retrying deterministic schema failures cannot make this
                    // fresh fixture ready and previously hid the useful error
                    // behind a full minute of reconnects. Migration diagnostics
                    // contain only repository-owned SQL, never fixture secrets.
                    Err(
                        error @ (veoveo_platform_store::StoreError::FreshInstallationRequired
                        | veoveo_platform_store::StoreError::Config(_)),
                    ) => {
                        panic!("isolated store initialization failed: {error}");
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        })
        .await
        .expect("isolated migrations/readiness failed");
        tokio::time::timeout(Duration::from_secs(60), prepared.apply(admin.client()))
            .await
            .expect("fixture selected bootstrap exceeded60seconds")
            .unwrap_or_else(|error| panic!("fixture selected bootstrap failed: {error}"));
        let admin_config =
            StoreConfig::builder(&endpoint, "veoveo_fixture", &database, admin_credentials)
                .audit_targets(audit_targets.clone())
                .build()
                .unwrap();
        if !schema.is_empty() {
            tokio::time::timeout(Duration::from_secs(10), async {
                admin.client().query(schema).await?.check()
            })
            .await
            .expect("Store fixture schema setup exceeded 10 seconds")
            .unwrap_or_else(|error| panic!("Store fixture schema setup failed: {error}"));
        }
        let runtime_password = fixture_password();
        tokio::time::timeout(
            Duration::from_secs(10),
            admin.replace_database_editor("fixture_runtime", &runtime_password.clone().into()),
        )
        .await
        .expect("Store fixture runtime credential setup exceeded 10 seconds")
        .unwrap_or_else(|_| panic!("Store fixture runtime credential setup failed"));
        let runtime_credentials = StoreCredentials::database("fixture_runtime", runtime_password);
        let config = StoreConfig::builder(
            &endpoint,
            "veoveo_fixture",
            database,
            runtime_credentials.clone(),
        )
        .audit_targets(audit_targets.clone())
        .build()
        .unwrap();
        let a = connect(config.clone(), "first runtime client").await;
        let b = connect(config, "second runtime client").await;
        Self {
            _container: container,
            admin_config,
            a,
            b,
        }
    }

    #[allow(
        dead_code,
        reason = "Only network-recovery fixtures select a fault-injection endpoint"
    )]
    pub async fn connect_via(&self, endpoint: &str) -> PlatformStore {
        let config = self.a.config().clone().with_endpoint(endpoint).unwrap();
        connect(config, "fault-injection client").await
    }
}

async fn connect(config: StoreConfig, stage: &str) -> PlatformStore {
    tokio::time::timeout(Duration::from_secs(10), PlatformStore::connect(config))
        .await
        .unwrap_or_else(|_| panic!("Store fixture {stage}: connection exceeded 10 seconds"))
        .unwrap_or_else(|_| panic!("Store fixture {stage}: connection failed"))
}

/// Recovery fixtures stop the previous reader before mutating and restarting it.
#[allow(
    dead_code,
    reason = "Only stopped-reader recovery fixtures await LIVE retirement"
)]
pub async fn wait_for_no_live(
    store: &PlatformStore,
    tables: &[veoveo_platform_store::ObservationTable],
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let mut remaining = 0;
            for table in tables {
                let mut response = store
                    .client()
                    .query(include_str!("queries/store/table_info.surql"))
                    .bind(("table", table.as_str().to_owned()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
                let info: Option<serde_json::Value> = response.take(0).unwrap();
                remaining += info.unwrap()["lives"].as_object().unwrap().len();
            }
            if remaining == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("stopped-reader LIVE queries survived ten-second cleanup bound");
}
