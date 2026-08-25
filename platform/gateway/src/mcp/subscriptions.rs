use std::{collections::BTreeMap, pin::Pin};

use futures::{Stream, StreamExt, stream::SelectAll};
use rmcp::{
    model::{ErrorData as McpError, ServerNotification, SubscriptionFilter},
    service::{ServiceError, SubscriptionContext},
};
use veoveo_mcp_contract::{
    CanonicalTaskId, DiscoveryFailureMode, GatewayAction, GatewayDiscoverySurface, ServerSlug,
};

use crate::mcp_support::{mcp_internal, upstream_error};

use super::{GatewayMcp, tools::project_detailed_task_resource_uris};

#[derive(Default)]
struct UpstreamSubscriptionRoute {
    filter: SubscriptionFilter,
    task_ids: BTreeMap<String, String>,
}

type RoutedNotificationStream = Pin<
    Box<
        dyn Stream<Item = (ServerSlug, bool, Result<ServerNotification, ServiceError>)>
            + Send
            + 'static,
    >,
>;

struct OpenedUpstreamSubscription {
    task_ids: BTreeMap<String, String>,
    connection: super::RequestUpstream,
    notifications: RoutedNotificationStream,
}

impl GatewayMcp {
    pub(super) fn accepted_subscriptions(
        &self,
        requested: &SubscriptionFilter,
    ) -> SubscriptionFilter {
        let catalog = self.catalog.current();
        let mut accepted = SubscriptionFilter::new();
        accepted.tools_list_changed = requested.tools_list_changed;
        accepted.prompts_list_changed = requested.prompts_list_changed;
        accepted.resources_list_changed = requested.resources_list_changed;
        let resources = requested
            .resource_subscriptions
            .iter()
            .flatten()
            .filter(|uri| self.project_resource_for_upstream(uri).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        if !resources.is_empty() {
            accepted.resource_subscriptions = Some(resources);
        }
        let tasks_exposed =
            catalog
                .profile_servers(&self.profile_id)
                .iter()
                .any(|(exposure, server)| {
                    exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                        && server.capabilities.tasks
                });
        if tasks_exposed {
            let task_ids = requested
                .task_ids
                .iter()
                .flatten()
                .filter(|task_id| CanonicalTaskId::new((*task_id).clone()).is_ok())
                .cloned()
                .collect::<Vec<_>>();
            if !task_ids.is_empty() {
                accepted.task_ids = Some(task_ids);
            }
        }
        accepted
    }

    pub(super) async fn handle_listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context();
        let subject = self.authenticated(request_context)?;
        let accepted = context.accepted().clone();
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint =
            super::invocation_authorization_fingerprint(&subject.actor, &subject.authority)?;
        let mut discovery_changes = self.discovery.subscribe();

        // A catalog listener primes discovery on the same replica that owns the
        // stream. Each server completes independently and wakes the caller through
        // the shared cache without delaying subscription acknowledgement.
        if accepted.resources_list_changed == Some(true) {
            let _ = self
                .available_resources(request_context.clone(), subject.clone())
                .await?;
        }
        let isolate_tools = snapshot
            .catalog()
            .profile(&self.profile_id)
            .is_some_and(|profile| profile.discovery_failure_mode == DiscoveryFailureMode::Isolate);
        if accepted.tools_list_changed == Some(true) && isolate_tools {
            let _ = self
                .available_tools(request_context.clone(), subject.clone())
                .await?;
        }
        let mut routes = BTreeMap::<ServerSlug, UpstreamSubscriptionRoute>::new();

        if accepted.tools_list_changed == Some(true)
            || accepted.prompts_list_changed == Some(true)
            || accepted.resources_list_changed == Some(true)
        {
            let catalog = self.catalog.current();
            for server in self.profile_servers() {
                let (_, _, manifest) = catalog
                    .profile_server(&self.profile_id, &server)
                    .ok_or_else(|| mcp_internal(format!("unknown profile server `{server}`")))?;
                let route = routes.entry(server).or_default();
                route.filter.tools_list_changed = (accepted.tools_list_changed == Some(true)
                    && manifest.capabilities.tools_list_changed)
                    .then_some(true);
                route.filter.prompts_list_changed = (accepted.prompts_list_changed == Some(true)
                    && manifest.capabilities.prompts_list_changed)
                    .then_some(true);
                route.filter.resources_list_changed = (accepted.resources_list_changed
                    == Some(true)
                    && manifest.capabilities.resources_list_changed)
                    .then_some(true);
            }
        }

        for uri in accepted.resource_subscriptions.iter().flatten() {
            let projection = self.project_resource_for_upstream(uri)?;
            self.authorize_projected_resource(
                request_context,
                GatewayAction::SubscriptionsListen,
                &projection,
            )
            .await?;
            routes
                .entry(projection.server)
                .or_default()
                .filter
                .resource_subscriptions
                .get_or_insert_default()
                .push(projection.upstream_uri.to_string());
        }

        if accepted
            .task_ids
            .as_ref()
            .is_some_and(|ids| !ids.is_empty())
            && !self.client_allows_task_projection(&subject)?
        {
            return Err(McpError::missing_required_client_capability(
                rmcp::model::ClientCapabilities::builder()
                    .enable_tasks()
                    .build(),
            ));
        }
        for canonical_task_id in accepted.task_ids.iter().flatten() {
            let route = self
                .authorize_canonical_task_for_subject(
                    &subject,
                    GatewayAction::SubscriptionsListen,
                    canonical_task_id,
                )
                .await?;
            let upstream_route = routes.entry(route.server).or_default();
            upstream_route
                .filter
                .task_ids
                .get_or_insert_default()
                .push(route.task_id.clone());
            upstream_route
                .task_ids
                .insert(route.task_id, canonical_task_id.clone());
        }

        routes.retain(|_, route| !subscription_filter_is_empty(&route.filter));
        let gateway = self.clone();
        let downstream = request_context.peer.clone();
        let subscription_subject = subject.clone();
        let mut pending = futures::stream::iter(routes.into_iter().map(move |(server, route)| {
            let gateway = gateway.clone();
            let downstream = downstream.clone();
            let subject = subscription_subject.clone();
            async move {
                let required = route.filter.resource_subscriptions.is_some()
                    || route.filter.task_ids.is_some();
                let result = gateway
                    .open_upstream_subscription(
                        server.clone(),
                        route,
                        downstream,
                        subject,
                        required,
                    )
                    .await;
                (server, required, result)
            }
        }))
        .buffer_unordered(super::discovery::MAX_CONCURRENT_DISCOVERY);
        let mut pending_done = false;
        let mut notifications = SelectAll::<RoutedNotificationStream>::new();
        let mut task_routes = BTreeMap::<ServerSlug, BTreeMap<String, String>>::new();
        let mut upstream_connections = Vec::new();
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                opened = pending.next(), if !pending_done => {
                    match opened {
                        Some((server, _, Ok(opened))) => {
                            task_routes.insert(server, opened.task_ids);
                            notifications.push(opened.notifications);
                            upstream_connections.push(opened.connection);
                        }
                        Some((_server, true, Err(error))) => return Err(error),
                        Some((server, false, Err(error))) => {
                            tracing::warn!(%server, %error, "isolated upstream catalog-listener failure");
                        }
                        None => pending_done = true,
                    }
                }
                update = notifications.next(), if !notifications.is_empty() => {
                    let Some(update) = update else { continue; };
                    let (server, required, update) = update;
                    match update {
                        Ok(mut notification) => {
                            self.project_subscription_notification(
                                &server,
                                task_routes.get(&server),
                                &mut notification,
                            )?;
                            context.sink().send(notification).await.map_err(|error| {
                                mcp_internal(format!("failed to forward subscription notification: {error}"))
                            })?;
                        }
                        Err(error) if required => return Err(upstream_error(error)),
                        Err(error) => {
                            tracing::warn!(%error, "isolated upstream subscription stream failure");
                        }
                    }
                }
                change = discovery_changes.recv() => {
                    match change {
                        Ok(change) if change.belongs_to(
                            catalog_generation,
                            &subject.actor.id,
                            &authorization_fingerprint,
                        ) => match change.surface {
                            GatewayDiscoverySurface::Resources
                            | GatewayDiscoverySurface::ResourceTemplates
                                if accepted.resources_list_changed == Some(true) => {
                                    context.sink().notify_resource_list_changed().await.map_err(|error| {
                                        mcp_internal(format!("failed to publish resource catalog change: {error}"))
                                    })?;
                                }
                            GatewayDiscoverySurface::Tools
                                if accepted.tools_list_changed == Some(true) => {
                                    context.sink().notify_tool_list_changed().await.map_err(|error| {
                                        mcp_internal(format!("failed to publish tool catalog change: {error}"))
                                    })?;
                                }
                            _ => {}
                        },
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "gateway discovery changes lagged");
                            if accepted.resources_list_changed == Some(true) {
                                context.sink().notify_resource_list_changed().await.map_err(|error| {
                                    mcp_internal(format!("failed to publish resource catalog resync: {error}"))
                                })?;
                            }
                            if accepted.tools_list_changed == Some(true) {
                                context.sink().notify_tool_list_changed().await.map_err(|error| {
                                    mcp_internal(format!("failed to publish tool catalog resync: {error}"))
                                })?;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
            }
        }
    }

    async fn open_upstream_subscription(
        &self,
        server: ServerSlug,
        route: UpstreamSubscriptionRoute,
        downstream: rmcp::service::Peer<rmcp::service::RoleServer>,
        subject: crate::AuthenticatedSubject,
        required: bool,
    ) -> Result<OpenedUpstreamSubscription, McpError> {
        let needs_tasks = route.filter.task_ids.is_some();
        let upstream = self
            .upstream_with_tasks(&server, downstream, &subject, needs_tasks)
            .await?;
        let observed = upstream
            .peer
            .peer_info()
            .ok_or_else(|| mcp_internal(format!("upstream `{server}` was not discovered")))?;
        let effective = route.filter.supported_by(&observed.capabilities);
        if effective != route.filter {
            return Err(mcp_internal(format!(
                "upstream `{server}` does not support the accepted subscription filter"
            )));
        }
        let subscription = upstream
            .peer
            .listen(effective)
            .await
            .map_err(upstream_error)?;
        if subscription.acknowledged() != &route.filter {
            return Err(mcp_internal(format!(
                "upstream `{server}` narrowed the accepted subscription filter"
            )));
        }
        Ok(OpenedUpstreamSubscription {
            task_ids: route.task_ids,
            connection: upstream,
            notifications: Box::pin(routed_notifications(server, required, subscription)),
        })
    }

    fn project_subscription_notification(
        &self,
        server: &ServerSlug,
        task_routes: Option<&BTreeMap<String, String>>,
        notification: &mut ServerNotification,
    ) -> Result<(), McpError> {
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(server)
            .ok_or_else(|| mcp_internal(format!("unknown subscription server `{server}`")))?;
        match notification {
            ServerNotification::ResourceUpdatedNotification(update) => {
                update.params.uri = self
                    .project_upstream_resource(server, &update.params.uri)?
                    .gateway_uri
                    .to_string();
            }
            ServerNotification::TaskStatusNotification(update) => {
                let source_id = update.params.task.task.task_id.clone();
                let canonical = task_routes
                    .and_then(|routes| routes.get(&source_id))
                    .ok_or_else(|| mcp_internal("upstream notified an unrequested task"))?;
                project_detailed_task_resource_uris(manifest, &mut update.params.task)?;
                update.params.task.task.task_id = canonical.clone();
            }
            _ => {}
        }
        Ok(())
    }
}

fn subscription_filter_is_empty(filter: &SubscriptionFilter) -> bool {
    filter.tools_list_changed != Some(true)
        && filter.prompts_list_changed != Some(true)
        && filter.resources_list_changed != Some(true)
        && filter
            .resource_subscriptions
            .as_ref()
            .is_none_or(Vec::is_empty)
        && filter.task_ids.as_ref().is_none_or(Vec::is_empty)
}

fn routed_notifications(
    server: ServerSlug,
    required: bool,
    mut subscription: rmcp::service::Subscription,
) -> impl Stream<Item = (ServerSlug, bool, Result<ServerNotification, ServiceError>)> + Send + 'static
{
    async_stream::stream! {
        loop {
            match subscription.next().await {
                Ok(Some(notification)) => yield (server.clone(), required, Ok(notification)),
                Ok(None) => break,
                Err(error) => {
                    yield (server.clone(), required, Err(error));
                    break;
                }
            }
        }
    }
}
