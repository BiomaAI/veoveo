//! Request-scoped reconciliation and invalidation before access loss ends a stream.
use std::{collections::BTreeSet, time::Duration};

use chrono::Utc;
use rmcp::{ErrorData as McpError, service::SubscriptionContext};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_contract::parse_artifact_plane_uri;
use veoveo_artifact_mcp::contract::{ArtifactResource, parse_grants_uri, parse_metadata_uri};
use veoveo_mcp_contract::{ArtifactPlane, ArtifactPlaneError};

use super::{ArtifactInvalidation, ArtifactSubscriptions, SubscriptionKind, visible_ids};

pub(crate) async fn listen(
    plane: &HttpArtifactPlane,
    source: &ArtifactSubscriptions,
    context: SubscriptionContext,
) -> Result<(), McpError> {
    let caller = super::super::auth::caller(context.request_context())?;
    let accepted = context.accepted();
    // Observe before authorization and baseline reads so concurrent changes queue.
    let mut updates = source.listen();
    let mut subscriptions = Vec::new();
    let tracks_list = accepted.resources_list_changed == Some(true)
        || accepted.resource_subscriptions.iter().flatten().any(|uri| {
            matches!(
                ArtifactResource::parse(uri),
                Ok(ArtifactResource::Index { .. })
            )
        });
    for uri in accepted.resource_subscriptions.iter().flatten() {
        let kind = if matches!(
            ArtifactResource::parse(uri),
            Ok(ArtifactResource::Index { .. })
        ) {
            SubscriptionKind::Index
        } else if let Some(id) = parse_metadata_uri(uri) {
            SubscriptionKind::Metadata(id)
        } else if let Some(id) = parse_grants_uri(uri) {
            SubscriptionKind::Grants(id)
        } else if let Some(id) = parse_artifact_plane_uri(uri) {
            SubscriptionKind::Content(id)
        } else {
            return Err(McpError::invalid_params(
                "resource is not subscribable",
                None,
            ));
        };
        subscriptions.push((uri.clone(), kind));
    }
    let members = subscriptions
        .iter()
        .filter_map(|(_, kind)| match kind {
            SubscriptionKind::Index => None,
            SubscriptionKind::Content(id)
            | SubscriptionKind::Metadata(id)
            | SubscriptionKind::Grants(id) => Some(*id),
        })
        .collect::<Vec<_>>();
    let selection = (!tracks_list).then_some(members.as_slice());
    let (mut visible, mut deadline) = tokio::time::timeout(Duration::from_secs(60), async {
        // Read deadlines first: expiry during HTTP reads must still wake us.
        let deadline = source
            .deadline(&caller, selection)
            .await
            .map_err(internal)?;
        for (_, kind) in &subscriptions {
            match kind {
                SubscriptionKind::Index => {}
                SubscriptionKind::Content(id) | SubscriptionKind::Metadata(id) => {
                    plane.head(&caller, id).await.map_err(plane_error)?;
                }
                SubscriptionKind::Grants(id) => {
                    plane.list_grants(&caller, id).await.map_err(plane_error)?;
                }
            }
        }
        let visible = if tracks_list {
            visible_ids(plane, &caller).await.map_err(plane_error)?
        } else {
            BTreeSet::new()
        };
        Ok::<_, McpError>((visible, deadline))
    })
    .await
    .map_err(|_| internal("Artifact subscription baseline exceeded 60 seconds"))??;
    if accepted.resources_list_changed == Some(true) {
        notify_list(&context).await?;
    }
    for (uri, _) in &subscriptions {
        notify(&context, uri).await?;
    }
    loop {
        let delay = (deadline - Utc::now()).to_std().unwrap_or_default();
        let changed = tokio::select! {
            () = context.cancelled() => return Ok(()),
            _ = tokio::time::sleep(delay) => None,
            update = updates.recv() => match update {
                Ok(ArtifactInvalidation::Changed(id)) => Some(id),
                Ok(ArtifactInvalidation::Reconcile) => None,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
            }
        };
        // A concurrent database event may win select! after the timer is ready.
        // Reconcile every admitted address when any stored deadline has passed.
        let changed = if deadline <= Utc::now() {
            None
        } else {
            changed
        };
        if caller.identity.expires_at <= Utc::now() {
            if accepted.resources_list_changed == Some(true) {
                notify_list(&context).await?;
            }
            for (uri, _) in &subscriptions {
                notify(&context, uri).await?;
            }
            return Err(McpError::invalid_request(
                "artifact authorization expired",
                None,
            ));
        }
        let (current, notifications, lost_member, next_deadline) =
            tokio::time::timeout(Duration::from_secs(60), async {
                let deadline = source
                    .deadline(&caller, selection)
                    .await
                    .map_err(internal)?;
                let current = if tracks_list {
                    visible_ids(plane, &caller).await.map_err(plane_error)?
                } else {
                    BTreeSet::new()
                };
                let mut notifications = Vec::new();
                let mut lost_member = false;
                for (uri, kind) in &subscriptions {
                    let should_notify = match kind {
                        SubscriptionKind::Index => {
                            current != visible || changed.is_none_or(|id| current.contains(&id))
                        }
                        SubscriptionKind::Content(id) | SubscriptionKind::Metadata(id)
                            if changed.is_none_or(|changed| *id == changed) =>
                        {
                            lost_member |= unavailable(plane.head(&caller, id).await)?;
                            true
                        }
                        SubscriptionKind::Grants(id)
                            if changed.is_none_or(|changed| *id == changed) =>
                        {
                            lost_member |= unavailable(plane.list_grants(&caller, id).await)?;
                            true
                        }
                        _ => false,
                    };
                    if should_notify {
                        notifications.push(uri);
                    }
                }
                Ok::<_, McpError>((current, notifications, lost_member, deadline))
            })
            .await
            .map_err(|_| internal("Artifact subscription reconciliation exceeded 60 seconds"))??;
        if current != visible && accepted.resources_list_changed == Some(true) {
            notify_list(&context).await?;
        }
        for uri in notifications {
            notify(&context, uri).await?;
        }
        // Only already admitted addresses are invalidated; no revoked contents
        // or newly discovered identities are sent before closing the stream.
        if lost_member {
            return Err(McpError::resource_not_found(
                "artifact is no longer readable",
                None,
            ));
        }
        visible = current;
        deadline = next_deadline;
    }
}

fn unavailable<T>(result: Result<T, ArtifactPlaneError>) -> Result<bool, McpError> {
    match result {
        Ok(_) => Ok(false),
        Err(ArtifactPlaneError::NotFound | ArtifactPlaneError::Denied(_)) => Ok(true),
        Err(error) => Err(plane_error(error)),
    }
}
fn plane_error(error: ArtifactPlaneError) -> McpError {
    super::super::handler::plane_error(error)
}
fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
async fn notify(context: &SubscriptionContext, uri: &str) -> Result<(), McpError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        context.sink().notify_resource_updated(uri.to_owned()),
    )
    .await
    .map_err(|_| internal("Artifact notification timed out"))?
    .map_err(internal)
}
async fn notify_list(context: &SubscriptionContext) -> Result<(), McpError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        context.sink().notify_resource_list_changed(),
    )
    .await
    .map_err(|_| internal("Artifact catalog notification timed out"))?
    .map_err(internal)
}
