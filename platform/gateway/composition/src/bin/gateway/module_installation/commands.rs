//! Installation commands share preflight, generation proof and authoritative waits.
use super::{PlanArgs, composition, preparation_key};
use crate::{RedactedSecret, SurrealStoreArgs};
use anyhow::{Context, ensure};
use secrecy::ExposeSecret;
use std::time::{Duration, Instant};
use veoveo_modules::{
    ModuleName,
    runner::{self, DatabaseEditorCredentials},
};
use veoveo_platform_store::{PlatformStore, StoreAuthLevel};

pub(crate) async fn prepare(
    args: PlanArgs,
    store: SurrealStoreArgs,
    password: RedactedSecret,
    wait_seconds: u64,
) -> anyhow::Result<()> {
    let plan = args.load()?;
    ensure!(
        store.auth_level == StoreAuthLevel::Root,
        "installation-prepare requires root database authentication"
    );
    let key = preparation_key(&args, &plan)?;
    let credentials = DatabaseEditorCredentials::new(
        args.runtime_username.clone(),
        password.0.expose_secret().to_owned(),
    )?;
    let root_password = store.password.clone();
    let config = store.into_config()?;
    // StoreConfig validates identifiers; provision a genuinely absent namespace/database
    // only after the entire compiled selection has passed admission.
    provision_database(
        &config,
        root_password.0.expose_secret(),
        deadline(wait_seconds)?,
    )
    .await?;
    let store = PlatformStore::connect(config).await?;
    let registry = composition::registry()?;
    let prepared = runner::prepare(registry.select(plan.enabled().to_vec())?)?;
    prepared.claim_preparation(store.client(), &key).await?;
    store.migrate().await?;
    prepared
        .complete_preparation(store.client(), &key, credentials)
        .await?;
    println!(
        "{}",
        serde_json::json!({"status":"prepared","generation":plan.generation().to_string(),"identity":key.identity()})
    );
    Ok(())
}
async fn provision_database(
    config: &veoveo_platform_store::StoreConfig,
    root_password: &str,
    deadline: Instant,
) -> anyhow::Result<()> {
    use surrealdb::{
        Surreal,
        engine::remote::ws::{Ws, Wss},
        opt::{Config, auth::Root},
    };
    let address = config
        .endpoint()
        .as_str()
        .strip_prefix(&format!("{}://", config.endpoint().scheme()))
        .context("validated database endpoint")?;
    let db = loop {
        let attempt = tokio::time::timeout(Duration::from_secs(30), async {
            let db = match config.endpoint().scheme() {
                "ws" => {
                    Surreal::new::<Ws>((
                        address,
                        Config::new()
                            .query_timeout(config.query_timeout())
                            .transaction_timeout(config.transaction_timeout()),
                    ))
                    .await?
                }
                "wss" => {
                    Surreal::new::<Wss>((
                        address,
                        Config::new()
                            .query_timeout(config.query_timeout())
                            .transaction_timeout(config.transaction_timeout()),
                    ))
                    .await?
                }
                _ => unreachable!("validated WebSocket endpoint"),
            };
            db.signin(Root {
                username: config.username().into(),
                password: root_password.into(),
            })
            .await?;
            Ok::<_, surrealdb::Error>(db)
        })
        .await;
        if let Ok(Ok(db)) = attempt {
            break db;
        }
        ensure!(
            Instant::now() < deadline,
            "root database authentication/transport did not become ready within the configured wait"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    db.query(format!(
        "DEFINE NAMESPACE IF NOT EXISTS `{}`;",
        config.namespace()
    ))
    .await?
    .check()?;
    db.use_ns(config.namespace()).await?;
    db.query(format!(
        "DEFINE DATABASE IF NOT EXISTS `{}`;",
        config.database()
    ))
    .await?
    .check()?;
    Ok(())
}
pub(crate) async fn migrate(
    args: PlanArgs,
    store: SurrealStoreArgs,
    module: ModuleName,
    wait_seconds: u64,
) -> anyhow::Result<()> {
    let plan = args.load()?;
    let registry = composition::registry()?;
    let selection = registry.select(plan.enabled().to_vec())?;
    ensure!(
        selection.contains(&module),
        "module lane is disabled in the installation selection"
    );
    ensure!(
        store.auth_level == StoreAuthLevel::Root,
        "module-migrate requires root database authentication"
    );
    let deadline = deadline(wait_seconds)?;
    let store = connect_ready(store.into_config()?, deadline).await?;
    let prepared = runner::prepare(selection)?;
    let key = preparation_key(&args, &plan)?;
    wait_preparation(&prepared, &store, &key, deadline).await?;
    loop {
        let status = prepared.status(store.client()).await?;
        let setup = registry.module(&module).context("selected module")?;
        let ready = setup.requires().iter().all(|required| {
            status
                .lane(required.module())
                .is_some_and(|lane| match required.minimum() {
                    Some(minimum) => {
                        lane.initialized && lane.current.is_some_and(|version| version >= minimum)
                    }
                    None => lane.is_current(),
                })
        });
        if ready {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "module prerequisites are incomplete after the configured wait"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        prepared.require_preparation(store.client(), &key).await?;
    }
    prepared
        .apply_installation_lane(store.client(), &module, &key)
        .await?;
    println!(
        "{}",
        serde_json::json!({"module":module.as_str(),"status":"current"})
    );
    Ok(())
}
fn deadline(seconds: u64) -> anyhow::Result<Instant> {
    ensure!(
        seconds <= 300,
        "installation wait must be at most 300 seconds"
    );
    Ok(Instant::now() + Duration::from_secs(seconds))
}
pub(crate) async fn status(
    args: PlanArgs,
    store: SurrealStoreArgs,
    wait_seconds: u64,
) -> anyhow::Result<()> {
    let plan = args.load()?;
    let deadline = deadline(wait_seconds)?;
    let store = connect_ready(store.into_config()?, deadline).await?;
    let registry = composition::registry()?;
    let prepared = runner::prepare(registry.select(plan.enabled().to_vec())?)?;
    let key = preparation_key(&args, &plan)?;
    wait_preparation(&prepared, &store, &key, deadline).await?;
    loop {
        prepared.require_preparation(store.client(), &key).await?;
        let status = prepared.status(store.client()).await?;
        if status.is_current() {
            println!(
                "{}",
                serde_json::json!({"status":"current","generation":plan.generation().to_string(),"lanes":status.lanes.iter().map(|lane| serde_json::json!({"module":lane.module.as_str(),"initialized":lane.initialized,"current":lane.current.map(|v|v.get()),"latest":lane.latest.map(|v|v.get()),"selected":lane.selected})).collect::<Vec<_>>()})
            );
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "selected module lanes are incomplete after the configured wait"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
pub(crate) async fn require_current(args: &PlanArgs, store: &PlatformStore) -> anyhow::Result<()> {
    let plan = args.load()?;
    ensure!(
        store.config().auth_level() == StoreAuthLevel::Database,
        "runtime publication and serving require database-scoped authentication"
    );
    ensure!(
        store.config().username() == args.runtime_username,
        "database-authenticated account differs from the declared runtime account"
    );
    let registry = composition::registry()?;
    let prepared = runner::prepare(registry.select(plan.enabled().to_vec())?)?;
    prepared
        .require_preparation(store.client(), &preparation_key(args, &plan)?)
        .await?;
    ensure!(
        prepared.status(store.client()).await?.is_current(),
        "selected module lanes are incomplete"
    );
    ensure!(
        store.schema_status().await?.is_current(),
        "mixed legacy schema catalog is incomplete"
    );
    store.healthcheck().await?;
    Ok(())
}

async fn connect_ready(
    config: veoveo_platform_store::StoreConfig,
    deadline: Instant,
) -> anyhow::Result<PlatformStore> {
    loop {
        if let Ok(Ok(store)) = tokio::time::timeout(
            Duration::from_secs(30),
            PlatformStore::connect(config.clone()),
        )
        .await
        {
            return Ok(store);
        }
        ensure!(
            Instant::now() < deadline,
            "database authentication/selected database did not become ready within the configured wait"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
async fn wait_preparation(
    prepared: &runner::PreparedInstallation<'_>,
    store: &PlatformStore,
    key: &veoveo_modules::PreparationKey,
    deadline: Instant,
) -> anyhow::Result<()> {
    loop {
        if prepared.preparation_ready(store.client(), key).await? {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "expected preparation generation did not complete within the configured wait"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
pub(crate) async fn wait_current(
    args: &PlanArgs,
    config: veoveo_platform_store::StoreConfig,
    seconds: u64,
) -> anyhow::Result<PlatformStore> {
    let plan = args.load()?;
    ensure!(
        config.auth_level() == StoreAuthLevel::Database
            && config.username() == args.runtime_username,
        "publication requires the declared database-scoped runtime account"
    );
    let deadline = deadline(seconds)?;
    let store = connect_ready(config, deadline).await?;
    let registry = composition::registry()?;
    let prepared = runner::prepare(registry.select(plan.enabled().to_vec())?)?;
    let key = preparation_key(args, &plan)?;
    wait_preparation(&prepared, &store, &key, deadline).await?;
    loop {
        prepared.require_preparation(store.client(), &key).await?;
        if prepared.status(store.client()).await?.is_current() {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "selected module lanes did not complete within the configured wait"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    require_current(args, &store).await?;
    Ok(store)
}
