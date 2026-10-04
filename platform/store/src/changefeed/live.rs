//! Owned physical-connection cleanup for the SurrealDB3.3 LIVE profile.
use crate::{ObservationTable, PlatformClient, PlatformStore, StoreError};
use futures::{StreamExt, stream::BoxStream};
use std::{sync::Arc, time::Duration};
use surrealdb::{
    Notification, Surreal,
    types::{RecordId, SurrealValue, Uuid},
};

#[derive(SurrealValue)]
pub(crate) struct Identity {
    pub(crate) id: RecordId,
}

/// The SDK's3.3 query stream reconstructs an unauthenticated session for Drop.
/// Keep the registering session and only IDs returned by its own statements.
struct LiveCleanup {
    client: Arc<Surreal<PlatformClient>>,
    ids: Vec<Uuid>,
}
impl Drop for LiveCleanup {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        let ids = std::mem::take(&mut self.ids);
        let client = Arc::clone(&self.client);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::error!(
                queries = ids.len(),
                "LIVE cleanup requires the owning runtime to remain running"
            );
            return;
        };
        runtime.spawn(async move {
            let count = ids.len();
            let cleanup = async {
                let mut first_error = None;
                for id in ids {
                    let result = client
                        .query(include_str!("../../queries/changefeed/kill.surql"))
                        .bind(("query", id))
                        .await
                        .and_then(|response| response.check());
                    if let Err(error) = result {
                        first_error.get_or_insert(error);
                    }
                }
                first_error.map_or(Ok(()), Err)
            };
            match tokio::time::timeout(Duration::from_secs(30), cleanup).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::error!(queries = count, %error, "owned LIVE cleanup failed")
                }
                Err(_) => tracing::error!(queries = count, "owned LIVE cleanup exceeded30seconds"),
            }
        });
    }
}
impl PlatformStore {
    async fn registered_live<T: SurrealValue + Unpin + Send + 'static>(
        &self,
        tables: &[ObservationTable],
        statement: &'static str,
    ) -> Result<BoxStream<'static, Result<Notification<T>, surrealdb::Error>>, StoreError> {
        self.registered_live_with_timeout(tables, statement, Duration::from_secs(30))
            .await
    }
    async fn registered_live_with_timeout<T: SurrealValue + Unpin + Send + 'static>(
        &self,
        tables: &[ObservationTable],
        statement: &'static str,
        timeout: Duration,
    ) -> Result<BoxStream<'static, Result<Notification<T>, surrealdb::Error>>, StoreError> {
        let config = self.config().clone();
        let tables = tables.to_vec();
        let (_caller, mut abandoned) = tokio::sync::oneshot::channel::<()>();
        let registration = tokio::spawn(async move {
            tokio::time::timeout(timeout, async {
                // One authenticated physical connection owns the entire group.
                // Its lifetime covers registrations whose query UUID is not returned.
                let client = Arc::new(Self::connect_client(&config).await?);
                Self::register_live(client, tables, statement, &mut abandoned).await
            })
            .await
            .map_err(|_| StoreError::ChangefeedConnectionTimeout)?
        });
        registration
            .await
            .map_err(|_| StoreError::InvalidChangefeedEntry {
                reason: "owned LIVE registration task failed",
            })?
    }
    async fn register_live<T: SurrealValue + Unpin + Send + 'static>(
        client: Arc<Surreal<PlatformClient>>,
        tables: Vec<ObservationTable>,
        statement: &'static str,
        abandoned: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> Result<BoxStream<'static, Result<Notification<T>, surrealdb::Error>>, StoreError> {
        let mut cleanup = LiveCleanup {
            client: Arc::clone(&client),
            ids: Vec::new(),
        };
        let mut streams = Vec::new();
        for table in &tables {
            if matches!(
                abandoned.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed)
            ) {
                return Err(StoreError::InvalidChangefeedEntry {
                    reason: "LIVE registration caller abandoned",
                });
            }
            let mut response = client
                .query(statement)
                .bind(("table", table.as_str().to_owned()))
                .await?;
            let id: Option<Uuid> = response.take(0)?;
            cleanup.ids.push(id.ok_or(StoreError::MissingRecord {
                operation: "LIVE query identity",
            })?);
            let mut response = response.check()?;
            streams.push(
                response
                    .stream::<Notification<T>>(0)?
                    .chain(futures::stream::once(async {
                        Err(surrealdb::Error::internal(
                            "owned LIVE source ended".to_owned(),
                        ))
                    }))
                    .boxed(),
            );
        }
        let mut source = futures::stream::select_all(streams);
        Ok(Box::pin(async_stream::stream! {
            let _cleanup = cleanup;
            while let Some(event) = source.next().await { yield event; }
        }))
    }
    pub(crate) async fn live_identities(
        &self,
        tables: &[ObservationTable],
    ) -> Result<BoxStream<'static, Result<Notification<Identity>, surrealdb::Error>>, StoreError>
    {
        self.registered_live(tables, include_str!("../../queries/changefeed/live.surql"))
            .await
    }
    pub(crate) async fn live_rows<T: SurrealValue + Unpin + Send + 'static>(
        &self,
        table: ObservationTable,
    ) -> Result<super::LiveStream<T>, StoreError> {
        self.registered_live(
            &[table],
            include_str!("../../queries/changefeed/live_rows.surql"),
        )
        .await
    }
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
