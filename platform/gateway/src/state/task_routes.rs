use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_mcp_contract::CanonicalTaskId;
use veoveo_platform_store::{
    PrincipalKind, RecordIdKey, StoreError, TaskId, deterministic_principal_id,
    deterministic_tenant_id, deterministic_work_context_id,
};

use super::GatewayState;

const DEFAULT_TASK_ROUTE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

#[cfg(test)]
#[path = "task_routes/tests.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct GatewayTaskRouteDraft {
    pub tenant_key: String,
    pub owner_key: String,
    pub owner_issuer: String,
    pub owner_subject: String,
    pub owner_kind: PrincipalKind,
    pub work_context: String,
    pub profile: String,
    pub server: String,
    pub source_task_id: String,
    pub source_task: Option<TaskId>,
    pub authority_digest: String,
    pub ttl_ms: Option<u64>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, SurrealValue)]
pub(crate) struct GatewayTaskRouteRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub profile: RecordId,
    pub server: RecordId,
    pub source_task_id: String,
    pub source_task: Option<RecordId>,
    pub authority_digest: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, serde::Serialize, SurrealValue)]
struct GatewayTaskRouteContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    profile: RecordId,
    server: RecordId,
    source_task_id: String,
    source_task: Option<RecordId>,
    authority_digest: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl GatewayState {
    pub(crate) async fn create_task_route(
        &self,
        draft: GatewayTaskRouteDraft,
    ) -> Result<(CanonicalTaskId, GatewayTaskRouteRecord), StoreError> {
        if draft.source_task_id.is_empty() || draft.source_task_id.len() > 512 {
            return Err(StoreError::InvalidGatewayTaskRoute {
                reason: "source task id must contain 1 to 512 bytes".to_owned(),
            });
        }
        if draft.authority_digest.len() != 64
            || !draft
                .authority_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StoreError::InvalidGatewayTaskRoute {
                reason: "authority digest must contain 64 lowercase hexadecimal digits".to_owned(),
            });
        }
        let now = Utc::now();
        let ttl_ms = draft.ttl_ms.unwrap_or(DEFAULT_TASK_ROUTE_TTL_MS);
        let ttl = chrono::TimeDelta::try_milliseconds(i64::try_from(ttl_ms).map_err(|_| {
            StoreError::InvalidGatewayTaskRoute {
                reason: "task route TTL exceeds supported range".to_owned(),
            }
        })?)
        .ok_or_else(|| StoreError::InvalidGatewayTaskRoute {
            reason: "task route TTL is invalid".to_owned(),
        })?;
        let content = GatewayTaskRouteContent {
            tenant: deterministic_tenant_id(&draft.tenant_key)?.record_id(),
            owner: deterministic_principal_id(&draft.tenant_key, &draft.owner_key)?.record_id(),
            work_context: deterministic_work_context_id(&draft.tenant_key, &draft.work_context)?
                .record_id(),
            profile: RecordId::new("profile", draft.profile),
            server: RecordId::new("mcp_server", draft.server),
            source_task_id: draft.source_task_id,
            source_task: draft.source_task.map(|task| task.record_id()),
            authority_digest: draft.authority_digest,
            created_at: now,
            expires_at: now.checked_add_signed(ttl).ok_or_else(|| {
                StoreError::InvalidGatewayTaskRoute {
                    reason: "task route expiry exceeds supported range".into(),
                }
            })?,
        };
        if let Some(route) = self.task_route_for_source(&content).await? {
            return reusable_route(route, &content);
        }
        self.platform
            .ensure_identity(
                &draft.tenant_key,
                &draft.owner_key,
                &draft.owner_issuer,
                &draft.owner_subject,
                draft.owner_kind,
            )
            .await?;
        let canonical = new_task_route_id()?;
        let record_id = RecordId::new("gateway_task_route", canonical.as_str().to_owned());
        let created = async {
            let mut response = self
                .platform
                .client()
                .query("CREATE ONLY $record CONTENT $content;")
                .bind(("record", record_id))
                .bind(("content", content.clone()))
                .await?
                .check()?;
            let route: Option<GatewayTaskRouteRecord> = response.take(0)?;
            route.ok_or_else(|| StoreError::InvalidGatewayTaskRoute {
                reason: "gateway task route creation returned no record".to_owned(),
            })
        }
        .await;
        match created {
            Ok(route) => reusable_route(route, &content),
            Err(error) => {
                // A concurrent replica or lost CREATE reply may already have
                // committed the unique source mapping. Read it; never redispatch
                // the tool, replace its authority or extend its original expiry.
                match self.task_route_for_source(&content).await {
                    Ok(Some(route)) => reusable_route(route, &content),
                    _ => Err(error),
                }
            }
        }
    }

    async fn task_route_for_source(
        &self,
        content: &GatewayTaskRouteContent,
    ) -> Result<Option<GatewayTaskRouteRecord>, StoreError> {
        let mut response = self.platform.client().query(
            "SELECT * FROM gateway_task_route WHERE server = $server AND source_task_id = $source
             AND owner = $owner AND profile = $profile LIMIT 1;",
        ).bind(("server", content.server.clone()))
            .bind(("source", content.source_task_id.clone()))
            .bind(("owner", content.owner.clone()))
            .bind(("profile", content.profile.clone()))
            .await?.check()?;
        let mut rows: Vec<GatewayTaskRouteRecord> = response.take(0)?;
        Ok(rows.pop())
    }

    pub(crate) async fn task_route(
        &self,
        canonical: &CanonicalTaskId,
    ) -> Result<Option<GatewayTaskRouteRecord>, StoreError> {
        let record = RecordId::new("gateway_task_route", canonical.as_str().to_owned());
        let mut response = self
            .platform
            .client()
            .query("SELECT * FROM ONLY $record WHERE expires_at > time::now();")
            .bind(("record", record))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }
}

fn reusable_route(
    route: GatewayTaskRouteRecord,
    expected: &GatewayTaskRouteContent,
) -> Result<(CanonicalTaskId, GatewayTaskRouteRecord), StoreError> {
    if route.tenant != expected.tenant
        || route.owner != expected.owner
        || route.work_context != expected.work_context
        || route.profile != expected.profile
        || route.server != expected.server
        || route.source_task_id != expected.source_task_id
        || route.source_task != expected.source_task
        || route.authority_digest != expected.authority_digest
        || route.expires_at <= Utc::now()
    {
        return Err(StoreError::InvalidGatewayTaskRoute {
            reason: "existing task route does not admit this retry".into(),
        });
    }
    let RecordIdKey::String(id) = &route.id.key else {
        return Err(StoreError::InvalidGatewayTaskRoute {
            reason: "invalid stored task route identity".into(),
        });
    };
    let canonical =
        CanonicalTaskId::new(id.clone()).map_err(|_| StoreError::InvalidGatewayTaskRoute {
            reason: "invalid stored task route identity".into(),
        })?;
    Ok((canonical, route))
}

fn new_task_route_id() -> Result<CanonicalTaskId, StoreError> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy).map_err(|error| StoreError::InvalidGatewayTaskRoute {
        reason: format!("task route entropy failed: {error}"),
    })?;
    CanonicalTaskId::new(format!("gtr_{}", URL_SAFE_NO_PAD.encode(entropy))).map_err(|error| {
        StoreError::InvalidGatewayTaskRoute {
            reason: error.to_string(),
        }
    })
}
