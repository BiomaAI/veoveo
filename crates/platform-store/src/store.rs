use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws, Wss};
use surrealdb::opt::{
    Config, WebsocketConfig,
    auth::{Database, Namespace, Root},
};

use crate::{StoreAuthLevel, StoreConfig, StoreCredentials, StoreError};

pub type PlatformClient = Client;

pub(crate) fn primary_transaction_error(
    errors: std::collections::HashMap<usize, surrealdb::Error>,
) -> Option<surrealdb::Error> {
    let mut errors = errors.into_iter().collect::<Vec<_>>();
    errors.sort_by_key(|(statement, _)| *statement);
    let primary = errors.iter().position(|(_, error)| {
        let message = error.to_string();
        !message.contains("not executed due to a failed transaction")
            && !message.contains("Cannot COMMIT")
    });
    primary
        .map(|index| errors.swap_remove(index).1)
        .or_else(|| errors.into_iter().next().map(|(_, error)| error))
}

/// A connected, namespace-scoped handle to the installation platform store.
#[derive(Clone, Debug)]
pub struct PlatformStore {
    pub(crate) db: Surreal<Client>,
    config: StoreConfig,
}

impl PlatformStore {
    pub async fn connect(config: StoreConfig) -> Result<Self, StoreError> {
        let websocket = WebsocketConfig::new()
            .read_buffer_size(config.websocket_read_buffer())
            .write_buffer_size(config.websocket_write_buffer())
            .max_write_buffer_size(config.websocket_max_write_buffer())
            .max_message_size(config.websocket_max_message());
        let sdk_config = Config::new()
            .query_timeout(config.query_timeout())
            .transaction_timeout(config.transaction_timeout())
            .websocket(websocket)?;
        let address = config
            .endpoint()
            .as_str()
            .strip_prefix(&format!("{}://", config.endpoint().scheme()))
            .expect("validated URL starts with its scheme")
            .to_owned();

        let db = match config.endpoint().scheme() {
            "ws" => {
                Surreal::new::<Ws>((address, sdk_config))
                    .with_capacity(config.connection_capacity())
                    .await?
            }
            "wss" => {
                Surreal::new::<Wss>((address, sdk_config))
                    .with_capacity(config.connection_capacity())
                    .await?
            }
            _ => unreachable!("StoreConfig accepts only ws and wss"),
        };

        match config.credentials() {
            StoreCredentials::Root { .. } => {
                db.signin(Root {
                    username: config.username().to_owned(),
                    password: config.credentials().password().to_owned(),
                })
                .await?;
            }
            StoreCredentials::Namespace { .. } => {
                db.signin(Namespace {
                    namespace: config.namespace().to_owned(),
                    username: config.username().to_owned(),
                    password: config.credentials().password().to_owned(),
                })
                .await?;
            }
            StoreCredentials::Database { .. } => {
                db.signin(Database {
                    namespace: config.namespace().to_owned(),
                    database: config.database().to_owned(),
                    username: config.username().to_owned(),
                    password: config.credentials().password().to_owned(),
                })
                .await?;
            }
        }
        db.use_ns(config.namespace())
            .use_db(config.database())
            .await?;

        let store = Self { db, config };
        if store.config.migrate_on_connect() {
            store.migrate().await?;
        }
        Ok(store)
    }

    pub fn client(&self) -> &Surreal<Client> {
        &self.db
    }

    pub fn config(&self) -> &StoreConfig {
        &self.config
    }

    pub(crate) fn require_root(&self, operation: &'static str) -> Result<(), StoreError> {
        if self.config.auth_level() != StoreAuthLevel::Root {
            return Err(StoreError::RootCredentialsRequired { operation });
        }
        Ok(())
    }
}
