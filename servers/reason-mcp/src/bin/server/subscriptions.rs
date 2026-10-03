//! Task-owner updates and Artifact-authorized finding changes share the MCP stream.
use super::{
    app_state::AppState, internal, invalid_params, knowledge, ownership, resources,
    task_extension::ReasonTaskService,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use rmcp::{ErrorData as McpError, service::SubscriptionContext};
use std::{collections::BTreeMap, time::Duration};
use veoveo_mcp_contract::PlaneCaller;
use veoveo_platform_store::ResourceInvalidation;
use veoveo_reason_mcp::{
    contract::{AnalysisId, AnalysisResource, FindingResource},
    knowledge::observe,
};
use veoveo_task_runtime::{DurableTaskService, TaskResourceSubscriptions};
use veoveo_types::{ResourceAddress, ResourceUri, Sha256Digest};

pub(super) fn finding(uri: &str) -> Option<FindingResource> {
    match FindingResource::parse(uri).ok()? {
        FindingResource::Page { .. } => None,
        address => Some(address),
    }
}

struct State {
    fingerprints: BTreeMap<ResourceUri, Sha256Digest>,
    deadline: DateTime<Utc>,
    lost_member: bool,
}

async fn snapshot(
    app: &AppState,
    caller: &PlaneCaller,
    addresses: &[(ResourceUri, FindingResource)],
) -> Result<State, McpError> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let scope = knowledge::scope(caller).map_err(internal)?;
        let mut state = State {
            fingerprints: BTreeMap::new(),
            deadline: caller.identity.expires_at,
            lost_member: false,
        };
        let mut selected = BTreeMap::new();
        for (uri, address) in addresses {
            let id = match address {
                FindingResource::Member { analysis, .. } => Some(*analysis),
                _ => None,
            };
            if let std::collections::btree_map::Entry::Vacant(entry) = selected.entry(id) {
                entry.insert(
                    observe::snapshot(app.tasks.platform_store(), &scope, id)
                        .await
                        .map_err(internal)?,
                );
            }
            let current = &selected[&id];
            if id.is_some() && !current.present {
                state.lost_member = true;
            }
            state
                .fingerprints
                .insert(uri.clone(), current.fingerprint.clone());
            if let Some(deadline) = current.deadline {
                state.deadline = state.deadline.min(deadline);
            }
        }
        Ok(state)
    })
    .await
    .map_err(|_| internal("finding observation exceeded 60 seconds"))?
}

async fn notify(context: &SubscriptionContext, uri: &ResourceUri) -> Result<(), McpError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        context.sink().notify_resource_updated(uri.to_string()),
    )
    .await
    .map_err(|_| internal("finding notification timed out"))?
    .map_err(internal)
}

pub(super) async fn listen(
    app: &AppState,
    task_service: &ReasonTaskService,
    context: SubscriptionContext,
) -> Result<(), McpError> {
    let mut changes = app.finding_changes.subscribe();
    let caller = veoveo_mcp_contract::hosting::plane_caller(context.request_context())?;
    let mut tasks_filter = context.accepted().clone();
    let mut addresses = Vec::new();
    let mut tasks_resources = Vec::new();
    for uri in tasks_filter
        .resource_subscriptions
        .take()
        .into_iter()
        .flatten()
    {
        if let Some(address) = finding(&uri) {
            addresses.push((address.to_uri().map_err(invalid_params)?, address));
        } else {
            tasks_resources.push(uri);
        }
    }
    if addresses.len() > 32 {
        return Err(invalid_params("at most 32 finding resources per listener"));
    }
    tasks_filter.resource_subscriptions = (!tasks_resources.is_empty()).then_some(tasks_resources);
    let tasks = TaskResourceSubscriptions::from_filter::<AnalysisResource>(&tasks_filter)?;
    for id in tasks.resource_task_ids() {
        resources::analysis_snapshot(
            &app.tasks,
            &ownership::runtime_owner(&caller.identity),
            AnalysisId::try_from(id).map_err(invalid_params)?,
        )
        .await?;
    }
    let task_caller = task_service.authenticate(context.request_context())?;
    let mut tasks = tasks.subscribe(task_service, &task_caller).await?;
    if !addresses.is_empty() {
        let ready = async {
            while changes.borrow_and_update().is_none() {
                changes.changed().await.map_err(internal)?;
            }
            Ok::<_, McpError>(())
        };
        tokio::select! {
            _ = context.cancelled() => return Ok(()),
            result = tokio::time::timeout(Duration::from_secs(10), ready) => result.map_err(|_| internal("finding observer is unavailable"))??,
        }
    }
    let mut previous = snapshot(app, &caller, &addresses).await?;
    if previous.lost_member {
        return Err(lost_member());
    }
    for (uri, _) in &addresses {
        notify(&context, uri).await?;
    }
    loop {
        let delay = (previous.deadline - Utc::now())
            .to_std()
            .unwrap_or_default();
        let reconcile = tokio::select! {
            _ = context.cancelled() => return Ok(()),
            _ = tokio::time::sleep(delay) => false,
            event = changes.changed(), if !addresses.is_empty() => {
                event.map_err(internal)?;
                !matches!(*changes.borrow_and_update(), Some(ResourceInvalidation::Live))
            }
            event = tasks.next() => {
                let Some(update) = event else { return Ok(()); };
                let update = update?;
                if let Some(task) = update.task {
                    tokio::time::timeout(Duration::from_secs(10), context.sink().notify_task_status(task))
                        .await.map_err(|_| internal("Task notification timed out"))?.map_err(internal)?;
                }
                for uri in update.resources { notify(&context, &uri).await?; }
                continue;
            }
        };
        let current = tokio::select! {
            _ = context.cancelled() => return Ok(()),
            result = snapshot(app, &caller, &addresses) => result?,
        };
        for (uri, hash) in &current.fingerprints {
            if reconcile || previous.fingerprints.get(uri) != Some(hash) {
                notify(&context, uri).await?;
            }
        }
        // The caller previously admitted these identities. Invalidate their
        // cached member and collection before ending the revoked subscription;
        // never emit content or continue observing the inaccessible member.
        if current.lost_member {
            return Err(lost_member());
        }
        previous = current;
    }
}

fn lost_member() -> McpError {
    McpError::resource_not_found("finding is no longer readable", None)
}
