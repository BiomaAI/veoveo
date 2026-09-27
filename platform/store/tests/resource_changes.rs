//! Opt-in native SurrealDB qualification. Requires root credentials at a ws://
//! VEOVEO_SURREAL_URL endpoint; all writes use one disposable test database.
use std::time::Duration;

use futures::StreamExt;
use secrecy::SecretString;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};
use veoveo_platform_store::{
    PlatformStore, PlatformTable, ResourceInvalidation, StoreConfig, StoreCredentials,
};

struct ConnectionSwitch {
    endpoint: String,
    commands: mpsc::Sender<(bool, oneshot::Sender<()>)>,
    task: JoinHandle<()>,
}

impl ConnectionSwitch {
    async fn start(endpoint: &str) -> Self {
        let remote = url::Url::parse(endpoint).unwrap();
        assert_eq!(
            remote.scheme(),
            "ws",
            "qualification requires a local ws endpoint"
        );
        let host = remote.host_str().unwrap().to_owned();
        let port = remote.port_or_known_default().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let (commands, mut receive) = mpsc::channel::<(bool, oneshot::Sender<()>)>(1);
        let task = tokio::spawn(async move {
            let mut enabled = true;
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    command = receive.recv() => {
                        let Some((next, acknowledged)) = command else { break; };
                        enabled = next;
                        if !enabled {
                            connections.abort_all();
                            while connections.join_next().await.is_some() {}
                        }
                        let _ = acknowledged.send(());
                    }
                    accepted = listener.accept() => {
                        let (mut client, _) = accepted.unwrap();
                        if !enabled { continue; }
                        let host = host.clone();
                        connections.spawn(async move {
                            let mut upstream = TcpStream::connect((host.as_str(), port)).await.unwrap();
                            let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Self {
            endpoint,
            commands,
            task,
        }
    }

    async fn set_enabled(&self, enabled: bool) {
        let (acknowledged, receiver) = oneshot::channel();
        self.commands.send((enabled, acknowledged)).await.unwrap();
        receiver.await.unwrap();
    }
}

impl Drop for ConnectionSwitch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn config(endpoint: String, database: &str) -> StoreConfig {
    StoreConfig::builder(
        endpoint,
        "veoveo_resource_integration",
        database,
        StoreCredentials::root(
            std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".into()),
            SecretString::from(
                std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".into()),
            ),
        ),
    )
    .build()
    .unwrap()
}

#[tokio::test]
async fn replica_changes_and_reconnect_invalidate_without_outbox_or_idle_polling() {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return;
    }
    let endpoint = std::env::var("VEOVEO_SURREAL_URL").expect("explicit test endpoint");
    let database = format!("resources_{}", uuid::Uuid::now_v7().simple());
    let writer = tokio::time::timeout(
        Duration::from_secs(15),
        PlatformStore::connect(config(endpoint.clone(), &database)),
    )
    .await
    .expect("database connection timed out")
    .unwrap();
    let test_writer = writer.clone();
    let mut test = tokio::spawn(async move {
        // Deliberately define no outbox. The same table identities used by the
        // Frames, Media and Recording hubs must deliver directly from the store.
        test_writer
            .client()
            .query(
                "DEFINE TABLE frame_world SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;
             DEFINE TABLE provider_job SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;
             DEFINE TABLE media_usage SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;
             DEFINE TABLE recording SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;
             DEFINE TABLE task SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;
             DEFINE TABLE domain_usage SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;
             DEFINE TABLE audit_event SCHEMALESS CHANGEFEED 1h INCLUDE ORIGINAL;",
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        let switch = ConnectionSwitch::start(&endpoint).await;
        let reader = PlatformStore::connect(config(switch.endpoint.clone(), &database))
            .await
            .unwrap();
        let mut changes = reader.resource_changes(vec![
            PlatformTable::FrameWorld,
            PlatformTable::ProviderJob,
            PlatformTable::MediaUsage,
            PlatformTable::Recording,
            PlatformTable::Task,
            PlatformTable::DomainUsage,
        ]);
        assert_eq!(
            changes.next().await,
            Some(ResourceInvalidation::Reconcile),
            "initial reconciliation"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(250), changes.next())
                .await
                .is_err(),
            "an idle source must not wake"
        );
        for statement in [
            "CREATE frame_world:fixture SET revision = 1;",
            "CREATE provider_job:fixture SET state = 'completed';",
            "CREATE media_usage:fixture SET cost = 1;",
            "CREATE recording:fixture SET state = 'writing';",
            "CREATE task:fixture SET state = 'succeeded';",
            "CREATE domain_usage:fixture SET quantity = 1;",
        ] {
            test_writer
                .client()
                .query(statement)
                .await
                .unwrap()
                .check()
                .unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(5), changes.next())
                    .await
                    .expect("replica update missing"),
                Some(ResourceInvalidation::Live)
            );
        }
        switch.set_enabled(false).await;
        // These commits occur while the observer cannot receive LIVE events.
        test_writer
            .client()
            .query(
                "CREATE audit_event:unrelated SET ordinal = 1;
             UPDATE frame_world:fixture SET revision = 2;",
            )
            .await
            .unwrap()
            .check()
            .unwrap();
        switch.set_enabled(true).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(15), changes.next())
                .await
                .expect("reconnect recovery missing"),
            Some(ResourceInvalidation::Changefeed)
        );
        test_writer
            .client()
            .query("DELETE recording:fixture;")
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), changes.next())
                .await
                .expect("LIVE did not resume"),
            Some(ResourceInvalidation::Live)
        );
        drop(changes);
    });
    let result = tokio::time::timeout(Duration::from_secs(60), &mut test).await;
    if result.is_err() {
        test.abort();
        let _ = test.await;
    }
    // Cleanup runs after failures and timeouts as well as after a passing test.
    tokio::time::timeout(
        Duration::from_secs(15),
        writer
            .client()
            .query(format!("REMOVE DATABASE {};", writer.config().database())),
    )
    .await
    .expect("test database cleanup timed out")
    .unwrap()
    .check()
    .unwrap();
    result
        .expect("resource qualification exceeded 60 seconds")
        .expect("resource qualification failed");
}
