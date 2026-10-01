//! Caller-admitted finding resources. Artifact remains the byte and access owner.
use super::{app_state::AppState, internal, invalid_params, ownership::internal_caller};
use anyhow::{Result, ensure};
use chrono::Utc;
use rmcp::{ErrorData as McpError, RoleServer, model::*, service::RequestContext};
use veoveo_mcp_contract::PlaneCaller;
use veoveo_platform_store::{
    ArtifactReadScope, PlatformIdentity, deterministic_principal_id, deterministic_tenant_id,
};
use veoveo_reason_mcp::{
    contract::*,
    knowledge::{FindingPosition, FindingSelection, readable_findings, summary},
};
use veoveo_types::ResourceAddress;

pub(super) fn scope(caller: &PlaneCaller) -> Result<ArtifactReadScope> {
    let identity = &caller.identity;
    let tenant = identity
        .actor
        .tenant
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("finding reads require a tenant"))?;
    ensure!(
        *tenant == identity.authority.tenant && identity.expires_at > Utc::now(),
        "finding authority is invalid or expired"
    );
    ArtifactReadScope::new(
        &PlatformIdentity {
            tenant_id: deterministic_tenant_id(tenant.as_str())?,
            principal_id: deterministic_principal_id(tenant.as_str(), identity.actor.id.as_str())?,
            tenant_key: tenant.to_string(),
            principal_key: identity.actor.id.to_string(),
        },
        caller.memberships.iter().map(|m| m.group.clone()),
        caller.clearance().clone(),
        Some(identity.authority.work_context.clone()),
    )
    .map_err(Into::into)
}

pub(super) async fn read(
    state: &AppState,
    address: FindingResource,
    context: &RequestContext<RoleServer>,
) -> Result<ReadResourceResult, McpError> {
    let caller = internal_caller(context)?;
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let scope = scope(&caller).map_err(internal)?;
        let uri = address.to_uri().map_err(invalid_params)?;
        let collection = address.collection();
        match address {
            FindingResource::Root { .. } | FindingResource::Page { .. } => {
                let cursor = match address {
                    FindingResource::Page { cursor } => Some(cursor),
                    _ => None,
                };
                let after = cursor.map(|c| FindingPosition {
                    created_at: c.created_at(),
                    analysis: c.analysis(),
                });
                let mut findings = readable_findings(
                    state.tasks.platform_store(),
                    &scope,
                    FindingSelection::Page(after.as_ref()),
                )
                .await
                .map_err(internal)?;
                self::scope(&caller).map_err(internal)?;
                let has_more = findings.len() > 100;
                findings.truncate(100);
                let next_cursor = has_more.then(|| findings.last()).flatten().map(|f| {
                    FindingCursor::new(collection, f.position.created_at, f.position.analysis)
                });
                let page = FindingPage {
                    items: findings
                        .into_iter()
                        .map(|f| FindingIndexEntry {
                            title: format!(
                                "{} {} {}",
                                f.pipeline,
                                collection.segment(),
                                f.position.analysis
                            ),
                            uri: FindingResource::Member {
                                collection,
                                analysis: f.position.analysis,
                            },
                        })
                        .collect(),
                    next_cursor,
                };
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(
                        serde_json::to_string(&page).map_err(internal)?,
                        uri.as_str(),
                    )
                    .with_mime_type("application/json"),
                ]))
            }
            FindingResource::Member {
                collection,
                analysis,
            } => {
                let select = FindingSelection::Member(analysis);
                let finding = readable_findings(state.tasks.platform_store(), &scope, select)
                    .await
                    .map_err(internal)?
                    .into_iter()
                    .next()
                    .ok_or_else(not_found)?;
                let before = state
                    .artifacts
                    .metadata_snapshot(&caller, &finding.results)
                    .await
                    .map_err(internal)?;
                // TODO(foundations): Read bounded finding data captured at publication;
                // valid full results can exceed this inline response ceiling.
                if before.metadata().byte_len > state.max_inline_resource_bytes {
                    return Err(McpError::invalid_request(
                        "Reason result exceeds the configured source-read byte limit",
                        None,
                    ));
                }
                let object = state
                    .artifacts
                    .get(&caller, &finding.results)
                    .await
                    .map_err(internal)?
                    .ok_or_else(not_found)?;
                if object.bytes.len() as u64 != before.metadata().byte_len
                    || object.bytes.len() as u64 > state.max_inline_resource_bytes
                {
                    return Err(internal(
                        "Reason result byte length differs from its metadata",
                    ));
                }
                let results: ReasoningResults =
                    serde_json::from_slice(&object.bytes).map_err(|_| {
                        internal("Reason result does not satisfy the current result contract")
                    })?;
                // Recheck grants and Task retention after byte I/O. Conditional reads
                // follow the same path and cannot reuse a revoked observation.
                let current = state
                    .artifacts
                    .metadata_snapshot(&caller, &finding.results)
                    .await
                    .map_err(internal)?;
                let rechecked = readable_findings(state.tasks.platform_store(), &scope, select)
                    .await
                    .map_err(internal)?;
                if rechecked.first() != Some(&finding) {
                    return Err(not_found());
                }
                if before != current {
                    return Err(internal("finding access changed during its read; retry"));
                }
                self::scope(&caller).map_err(internal)?;
                let (text, observation) =
                    summary::summarize(collection, &finding, &current, &results, Utc::now())
                        .map_err(internal)?;
                veoveo_mcp_knowledge_extension::server::member_result(
                    &uri,
                    "application/json",
                    text,
                    observation,
                    &summary::collection(collection),
                    Some(&context.meta),
                )
                .map_err(invalid_params)
            }
        }
    })
    .await
    .map_err(|_| internal("finding read exceeded 60 seconds"))?
}

fn not_found() -> McpError {
    McpError::resource_not_found("finding not found", None)
}
