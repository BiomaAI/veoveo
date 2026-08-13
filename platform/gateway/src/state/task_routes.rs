use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_mcp_contract::CanonicalTaskId;
use veoveo_platform_store::{
    PrincipalKind, StoreError, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id,
};

use super::GatewayState;

const DEFAULT_TASK_ROUTE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

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
            authority_digest: draft.authority_digest,
            created_at: now,
            expires_at: now + ttl,
        };
        let mut response = self
            .platform
            .client()
            .query("CREATE ONLY $record CONTENT $content;")
            .bind(("record", record_id))
            .bind(("content", content))
            .await?
            .check()?;
        let route: Option<GatewayTaskRouteRecord> = response.take(0)?;
        let route = route.ok_or_else(|| StoreError::InvalidGatewayTaskRoute {
            reason: "gateway task route creation returned no record".to_owned(),
        })?;
        Ok((canonical, route))
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
