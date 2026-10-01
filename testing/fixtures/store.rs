//! Shared Rust fixture: exact disposable store, no installation data or credentials.
use std::time::Duration;
#[path = "store/container.rs"]
mod container;
use container::{Container, Docker};
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
    runtime_credentials: StoreCredentials,
    pub a: PlatformStore,
    #[allow(dead_code, reason = "Only replica fixtures use the second connection")]
    pub b: PlatformStore,
}

impl TestDb {
    #[allow(
        dead_code,
        reason = "Fixture consumers select memory or RocksDB explicitly"
    )]
    pub async fn new() -> Self {
        Self::with_backend(StoreBackend::Memory).await
    }

    pub async fn with_backend(backend: StoreBackend) -> Self {
        Self::with_backend_and_schema(backend, "").await
    }

    pub async fn with_backend_and_schema(backend: StoreBackend, schema: &str) -> Self {
        let storage = match backend {
            StoreBackend::Memory => "memory",
            StoreBackend::RocksDb => "rocksdb:/tmp/veoveo-test.db",
        };
        let password = fixture_password();
        let (container, endpoint) = Container::start(Docker::default(), storage, &password)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        let database = format!("fixture_{}", Uuid::now_v7().simple());
        let config = StoreConfig::builder(
            &endpoint,
            "veoveo_fixture",
            &database,
            StoreCredentials::root("fixture_admin", password),
        )
        .migrate_on_connect(true)
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
                        error @ (veoveo_platform_store::StoreError::MigrationExecution { .. }
                        | veoveo_platform_store::StoreError::Migration(_)
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
        .build()
        .unwrap();
        let a = connect(config.clone(), "first runtime client").await;
        let b = connect(config, "second runtime client").await;
        Self {
            _container: container,
            runtime_credentials,
            a,
            b,
        }
    }

    #[allow(
        dead_code,
        reason = "Only network-recovery fixtures select a fault-injection endpoint"
    )]
    pub async fn connect_via(&self, endpoint: &str) -> PlatformStore {
        let config = StoreConfig::builder(
            endpoint,
            self.a.config().namespace(),
            self.a.config().database(),
            self.runtime_credentials.clone(),
        )
        .build()
        .unwrap();
        connect(config, "fault-injection client").await
    }
}

async fn connect(config: StoreConfig, stage: &str) -> PlatformStore {
    tokio::time::timeout(Duration::from_secs(10), PlatformStore::connect(config))
        .await
        .unwrap_or_else(|_| panic!("Store fixture {stage}: connection exceeded 10 seconds"))
        .unwrap_or_else(|_| panic!("Store fixture {stage}: connection failed"))
}
