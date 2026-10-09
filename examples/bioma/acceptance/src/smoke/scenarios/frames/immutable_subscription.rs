//! Observe immutable-resource rejection after the protocol acknowledgement.
use anyhow::{Context, Result, bail, ensure};
use rmcp::{
    ServiceError,
    model::{ErrorCode, SubscriptionFilter},
    service::{Peer, RoleClient, Subscription},
};
use std::time::Duration;
use tokio::time::Instant;

pub(super) async fn require_rejection(
    peer: &Peer<RoleClient>,
    filter: SubscriptionFilter,
    deadline: Instant,
    owned: &mut Option<Subscription>,
) -> Result<()> {
    tokio::time::timeout_at(deadline, async {
        match peer.listen(filter.clone()).await {
            Err(ServiceError::McpError(error)) if error.code == ErrorCode::INVALID_PARAMS => {
                return Ok(());
            }
            Err(_) => {
                bail!("Frames immutable subscription establishment did not return invalid params")
            }
            Ok(subscription) => *owned = Some(subscription),
        }
        let subscription = owned.as_mut().expect("opened immutable subscription");
        ensure!(
            subscription.acknowledged() == &filter,
            "Frames immutable subscription changed its acknowledged filter"
        );
        match subscription.next().await {
            Err(ServiceError::McpError(error)) if error.code == ErrorCode::INVALID_PARAMS => Ok(()),
            Err(ServiceError::McpError(error)) => bail!(
                "Frames immutable subscription returned unexpected MCP code {}",
                error.code.0
            ),
            Err(_) => bail!(
                "Frames immutable subscription rejection was not an MCP invalid params response"
            ),
            Ok(Some(_)) => bail!("Frames immutable subscription delivered a notification"),
            Ok(None) => bail!("Frames immutable subscription ended without invalid params"),
        }
    })
    .await
    .context("Frames immutable subscription rejection deadline exceeded")?
}

/// The caller owns this handle outside its cancellable operation timer.
pub(super) async fn cancel_owned(owned: &mut Option<Subscription>) -> bool {
    if let Some(subscription) = owned.as_mut() {
        matches!(
            tokio::time::timeout(Duration::from_secs(5), subscription.cancel()).await,
            Ok(Ok(()))
        )
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::{ClientServiceExt, ServerHandler, ServiceExt};
    use std::sync::Arc;
    use tokio::sync::Notify;

    const REVISION: &str = "frames://world/owned/revision/rev-1";
    #[derive(Clone, Copy)]
    enum Outcome {
        Reject,
        WrongCode,
        Notification,
        Complete,
        Pending,
        OuterDeadline,
    }
    #[derive(Clone)]
    struct Source {
        outcome: Outcome,
        started: Arc<Notify>,
        release: Arc<Notify>,
        cancelled: Arc<Notify>,
    }
    impl ServerHandler for Source {
        fn get_info(&self) -> rmcp::model::ServerConfig {
            rmcp::model::ServerConfig::new(
                rmcp::model::ServerCapabilities::builder()
                    .enable_resources()
                    .enable_resources_subscribe()
                    .build(),
            )
        }
        fn accepted_subscription_filter(
            &self,
            requested: &SubscriptionFilter,
        ) -> Option<SubscriptionFilter> {
            Some(requested.clone())
        }
        async fn listen(
            &self,
            context: rmcp::service::SubscriptionContext,
        ) -> std::result::Result<(), rmcp::ErrorData> {
            self.started.notify_one();
            self.release.notified().await;
            match self.outcome {
                Outcome::Reject => Err(rmcp::ErrorData::invalid_params("immutable revision", None)),
                Outcome::WrongCode => Err(rmcp::ErrorData::internal_error("wrong rejection", None)),
                Outcome::Complete => Ok(()),
                Outcome::Notification => {
                    context
                        .sink()
                        .notify_resource_updated(REVISION)
                        .await
                        .unwrap();
                    context.cancelled().await;
                    self.cancelled.notify_one();
                    Ok(())
                }
                Outcome::Pending | Outcome::OuterDeadline => {
                    context.cancelled().await;
                    self.cancelled.notify_one();
                    Ok(())
                }
            }
        }
    }

    async fn exercise(outcome: Outcome) -> Result<()> {
        let source = Source {
            outcome,
            started: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            cancelled: Arc::new(Notify::new()),
        };
        let handler = source.clone();
        // Use the maintained rmcp duplex fixture and Discover lifecycle used by gateway subscription controls.
        let (server_io, client_io) = tokio::io::duplex(8192);
        let server = tokio::spawn(async move { handler.serve(server_io).await });
        let client = ()
            .serve_with_lifecycle(
                client_io,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        let server = server.await??;
        let filter = SubscriptionFilter::builder()
            .resource_subscriptions([REVISION])
            .build();
        let mut owned = None;
        let deadline = Instant::now()
            + if matches!(outcome, Outcome::Pending) {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(2)
            };
        let result = {
            let observation =
                require_rejection(client.peer(), filter.clone(), deadline, &mut owned);
            tokio::pin!(observation);
            tokio::select! {
                result = &mut observation => panic!("probe decided before post-ack domain response: {result:?}"),
                _ = source.started.notified() => {},
            }
            assert!(
                tokio::time::timeout(Duration::from_millis(10), &mut observation)
                    .await
                    .is_err(),
                "protocol acknowledgement was mistaken for domain admission"
            );
            source.release.notify_one();
            if matches!(outcome, Outcome::OuterDeadline) {
                tokio::time::timeout(Duration::from_millis(100), observation)
                    .await
                    .context("outer operation deadline")
                    .and_then(|result| result)
            } else {
                observation.await
            }
        };
        assert_eq!(
            owned
                .as_ref()
                .expect("acknowledged listener retained")
                .acknowledged(),
            &filter
        );
        if matches!(outcome, Outcome::Reject) {
            result?;
            assert!(
                owned.as_ref().unwrap().end().is_some(),
                "terminal rejection must end the request"
            );
        } else {
            assert!(
                result.is_err(),
                "notification, EOF, other code or expiry cannot prove immutable rejection"
            );
        }
        assert!(
            cancel_owned(&mut owned).await,
            "ended rejection or pending request cleanup failed"
        );
        if matches!(
            outcome,
            Outcome::Pending | Outcome::OuterDeadline | Outcome::Notification
        ) {
            tokio::time::timeout(Duration::from_secs(1), source.cancelled.notified()).await?;
        }
        tokio::time::timeout(Duration::from_secs(2), client.cancel()).await??;
        tokio::time::timeout(Duration::from_secs(2), server.cancel()).await??;
        Ok(())
    }

    #[tokio::test]
    async fn acknowledgement_then_invalid_params_is_observed_as_terminal_rejection() -> Result<()> {
        exercise(Outcome::Reject).await
    }
    #[tokio::test]
    async fn immutable_probe_refuses_notification_eof_wrong_code_and_deadline() -> Result<()> {
        for outcome in [
            Outcome::Notification,
            Outcome::Complete,
            Outcome::WrongCode,
            Outcome::Pending,
            Outcome::OuterDeadline,
        ] {
            exercise(outcome).await?;
        }
        Ok(())
    }
}
