use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_map_mcp::contract::{MapMobilityProfileUri, MapRouteHandoff};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    PlatformStore, deterministic_tenant_id, deterministic_work_context_id,
};

use crate::contract::{
    ControlGrantId, GrantVehicleControlRequest, MissionPlanId, MissionPlanLifecycle,
    PrepareVehicleMissionRequest, RevokeVehicleControlRequest, SessionId, VehicleControlGrant,
    VehicleControlPermission, VehicleId, VehicleMissionPlan,
};

mod execution;
pub(super) use execution::{DispatchedMission, MissionExecutionGuard};
pub(super) mod task_link;
use task_link::ExecutionProfile;
mod map_handoff;
mod reads;
use map_handoff::{RouteRequirement, validate_map_handoff};
#[cfg(test)]
mod execution_test_support;
#[cfg(test)]
mod lease_tests;
#[cfg(test)]
mod map_tests;
#[cfg(test)]
mod task_link_tests;
pub(super) use reads::ControlCollection;

const PLAN_TTL: Duration = Duration::minutes(15);

#[derive(Clone)]
pub(super) struct VehicleControlAuthority {
    store: PlatformStore,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ControlAuthorityError {
    #[error("{0}")]
    Invalid(String),
    #[error("You don't have permission to control this vehicle.")]
    Forbidden,
    #[error(
        "vehicle grant was not found; list current grants with `list_active_vehicle_control_grants`"
    )]
    NotFound,
    #[error(
        "vehicle control state changed; read the current grants and mission plan before retrying"
    )]
    Conflict,
    #[error(
        "vehicle `{0}` has an executing or unresolved mission; inspect its mission and Task before retrying"
    )]
    VehicleBusy(String),
    #[error(transparent)]
    Task(#[from] veoveo_task_runtime::TaskError),
    #[error(transparent)]
    Store(#[from] veoveo_platform_store::StoreError),
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Index(#[from] anyhow::Error),
}

type Result<T> = std::result::Result<T, ControlAuthorityError>;

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct GrantRecord {
    id: RecordId,
    tenant: RecordId,
    work_context: RecordId,
    grant_id: String,
    session_id: String,
    vehicle_id: String,
    principal_key: String,
    permissions: Vec<String>,
    map_mobility_profile_uri: String,
    allow_planning_advisory: bool,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    created_by: String,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<String>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct GrantContent {
    tenant: RecordId,
    work_context: RecordId,
    grant_id: String,
    session_id: String,
    vehicle_id: String,
    principal_key: String,
    permissions: Vec<String>,
    map_mobility_profile_uri: String,
    allow_planning_advisory: bool,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    created_by: String,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<String>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct PlanRecord {
    id: RecordId,
    tenant: RecordId,
    work_context: RecordId,
    plan_id: String,
    mission_id: String,
    principal_key: String,
    session_id: String,
    vehicle_id: String,
    map_route_uri: String,
    map_route_digest_sha256: String,
    map_mobility_profile_uri: String,
    state: String,
    execution_profile: ExecutionProfile,
    canonical_json: String,
    expires_at: DateTime<Utc>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct PlanContent {
    tenant: RecordId,
    work_context: RecordId,
    plan_id: String,
    mission_id: String,
    principal_key: String,
    session_id: String,
    vehicle_id: String,
    map_route_uri: String,
    map_route_digest_sha256: String,
    map_mobility_profile_uri: String,
    state: String,
    execution_profile: ExecutionProfile,
    canonical_json: String,
    expires_at: DateTime<Utc>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl VehicleControlAuthority {
    pub(super) fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub(super) async fn grant(
        &self,
        identity: &GatewayInternalIdentity,
        request: GrantVehicleControlRequest,
    ) -> Result<VehicleControlGrant> {
        validate_grant_request(&request)?;
        let (tenant, work_context) = context_records(identity)?;
        let now = Utc::now();
        let content = GrantContent {
            tenant,
            work_context,
            grant_id: request.grant_id.to_string(),
            session_id: request.session_id.to_string(),
            vehicle_id: request.vehicle_id.to_string(),
            principal_key: request.principal_key,
            permissions: permission_strings(&request.permissions),
            map_mobility_profile_uri: request.map_mobility_profile_uri.as_str().to_owned(),
            allow_planning_advisory: request.allow_planning_advisory,
            valid_from: request.valid_from,
            valid_until: request.valid_until,
            created_by: identity.actor.id.to_string(),
            revoked_at: None,
            revoked_by: None,
            revision: 0,
            created_at: now,
            updated_at: now,
        };
        let record_id = scoped_record_id(
            "uav_vehicle_control_grant",
            identity,
            request.grant_id.as_str(),
        );
        let result = self
            .store
            .client()
            .query("CREATE ONLY $record CONTENT $content RETURN AFTER;")
            .bind(("record", record_id.clone()))
            .bind(("content", content.clone()))
            .await
            .and_then(|response| response.check());
        if result.is_err() {
            let existing = self.grant_record(&record_id).await?;
            if same_grant(&existing, &content) {
                return grant_view(existing);
            }
            return Err(ControlAuthorityError::Conflict);
        }
        grant_view(self.grant_record(&record_id).await?)
    }

    pub(super) async fn revoke(
        &self,
        identity: &GatewayInternalIdentity,
        request: RevokeVehicleControlRequest,
    ) -> Result<VehicleControlGrant> {
        let record_id = scoped_record_id(
            "uav_vehicle_control_grant",
            identity,
            request.grant_id.as_str(),
        );
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("UPDATE ONLY $record SET revoked_at = $now, revoked_by = $actor, updated_at = $now, revision += 1 WHERE revoked_at = NONE AND revision = $revision RETURN AFTER;")
            .bind(("record", record_id))
            .bind(("now", now))
            .bind(("actor", identity.actor.id.to_string()))
            .bind(("revision", checked_i64(request.expected_revision)?))
            .await?
            .check()?;
        let record: Option<GrantRecord> = response.take(0)?;
        grant_view(record.ok_or(ControlAuthorityError::Conflict)?)
    }

    pub(super) async fn prepare_plan(
        &self,
        identity: &GatewayInternalIdentity,
        request: PrepareVehicleMissionRequest,
    ) -> Result<VehicleMissionPlan> {
        let grant = self
            .require_route_permission(
                identity,
                &request.session_id,
                &request.vehicle_id,
                VehicleControlPermission::Plan,
                &request.map_route,
            )
            .await?;
        validate_map_handoff(&request, &grant)?;
        let (tenant, work_context) = context_records(identity)?;
        let now = Utc::now();
        let plan_id = MissionPlanId::new(format!("plan-{}", Uuid::now_v7()))
            .map_err(|error| ControlAuthorityError::Invalid(error.to_string()))?;
        let plan = VehicleMissionPlan {
            plan_id: plan_id.clone(),
            mission_id: request.mission_id,
            principal_key: identity.actor.id.to_string(),
            session_id: request.session_id,
            vehicle_id: request.vehicle_id,
            expected_world_revision_uri: request.expected_world_revision_uri,
            map_route: request.map_route,
            speed_mps: request.speed_mps,
            hold_seconds_at_destination: request.hold_seconds_at_destination,
            state: MissionPlanLifecycle::Prepared,
            expires_at: now + PLAN_TTL,
            revision: 0,
            created_at: now,
            updated_at: now,
        };
        let content = PlanContent {
            tenant,
            work_context,
            plan_id: plan_id.to_string(),
            mission_id: plan.mission_id.to_string(),
            principal_key: plan.principal_key.clone(),
            session_id: plan.session_id.to_string(),
            vehicle_id: plan.vehicle_id.to_string(),
            map_route_uri: plan.map_route.route_uri.clone(),
            map_route_digest_sha256: plan.map_route.route_digest_sha256.clone(),
            map_mobility_profile_uri: plan.map_route.mobility_profile_uri.as_str().to_owned(),
            state: "prepared".to_owned(),
            execution_profile: ExecutionProfile::TaskLinkedV1,
            canonical_json: serde_json::to_string(&plan)?,
            expires_at: plan.expires_at,
            revision: 0,
            created_at: now,
            updated_at: now,
        };
        let record_id = scoped_record_id("uav_vehicle_mission_plan", identity, plan_id.as_str());
        self.store
            .client()
            .query("CREATE ONLY $record CONTENT $content RETURN NONE;")
            .bind(("record", record_id))
            .bind(("content", content))
            .await?
            .check()?;
        Ok(plan)
    }

    async fn grant_record(&self, record_id: &RecordId) -> Result<GrantRecord> {
        select_only(&self.store, record_id.clone(), "control grant").await
    }

    async fn plan_record(&self, record_id: &RecordId) -> Result<PlanRecord> {
        select_only(&self.store, record_id.clone(), "mission plan").await
    }
}

async fn select_only<T>(store: &PlatformStore, record: RecordId, _name: &str) -> Result<T>
where
    T: SurrealValue,
{
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record;")
        .bind(("record", record))
        .await?
        .check()?;
    response
        .take::<Option<T>>(0)?
        .ok_or(ControlAuthorityError::NotFound)
}

fn validate_grant_request(request: &GrantVehicleControlRequest) -> Result<()> {
    if request.principal_key.trim().is_empty()
        || request.principal_key.len() > 2_048
        || request.principal_key.chars().any(char::is_control)
    {
        return Err(ControlAuthorityError::Invalid(
            "principal_key must be a bounded non-empty authenticated principal id".to_owned(),
        ));
    }
    if request.permissions.is_empty() {
        return Err(ControlAuthorityError::Invalid(
            "a vehicle control grant requires at least one permission".to_owned(),
        ));
    }
    if request
        .valid_until
        .is_some_and(|until| until <= request.valid_from)
    {
        return Err(ControlAuthorityError::Invalid(
            "valid_until must be later than valid_from".to_owned(),
        ));
    }
    Ok(())
}

fn same_grant(record: &GrantRecord, content: &GrantContent) -> bool {
    record.tenant == content.tenant
        && record.work_context == content.work_context
        && record.grant_id == content.grant_id
        && record.session_id == content.session_id
        && record.vehicle_id == content.vehicle_id
        && record.principal_key == content.principal_key
        && record.permissions == content.permissions
        && record.map_mobility_profile_uri == content.map_mobility_profile_uri
        && record.allow_planning_advisory == content.allow_planning_advisory
        && record.valid_from == content.valid_from
        && record.valid_until == content.valid_until
        && record.created_by == content.created_by
}

fn grant_view(record: GrantRecord) -> Result<VehicleControlGrant> {
    Ok(VehicleControlGrant {
        grant_id: ControlGrantId::new(record.grant_id)
            .map_err(|error| ControlAuthorityError::Invalid(error.to_string()))?,
        session_id: SessionId::new(record.session_id)
            .map_err(|error| ControlAuthorityError::Invalid(error.to_string()))?,
        vehicle_id: VehicleId::new(record.vehicle_id)
            .map_err(|error| ControlAuthorityError::Invalid(error.to_string()))?,
        principal_key: record.principal_key,
        permissions: record
            .permissions
            .into_iter()
            .map(|value| match value.as_str() {
                "inspect" => Ok(VehicleControlPermission::Inspect),
                "plan" => Ok(VehicleControlPermission::Plan),
                "execute" => Ok(VehicleControlPermission::Execute),
                "abort" => Ok(VehicleControlPermission::Abort),
                _ => Err(ControlAuthorityError::Invalid(
                    "persisted vehicle permission is invalid".to_owned(),
                )),
            })
            .collect::<Result<_>>()?,
        map_mobility_profile_uri: MapMobilityProfileUri::parse(&record.map_mobility_profile_uri)
            .map_err(|_| ControlAuthorityError::Invalid(
                "persisted grant has an invalid Map profile URI; repair the retained grant before retrying".into()
            ))?,
        allow_planning_advisory: record.allow_planning_advisory,
        valid_from: record.valid_from,
        valid_until: record.valid_until,
        created_by: record.created_by,
        revoked_at: record.revoked_at,
        revoked_by: record.revoked_by,
        revision: u64::try_from(record.revision).map_err(|_| ControlAuthorityError::Conflict)?,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

// This validates records already selected by SQL; it never filters a result page.
fn visible_plan_view(
    record: &PlanRecord,
    identity: &GatewayInternalIdentity,
) -> Result<VehicleMissionPlan> {
    let plan = plan_view(record)?;
    let (tenant, context) = context_records(identity)?;
    if record.tenant != tenant
        || record.work_context != context
        || record.id
            != scoped_record_id("uav_vehicle_mission_plan", identity, plan.plan_id.as_str())
    {
        return Err(ControlAuthorityError::Invalid(
            "persisted mission plan has inconsistent ownership or physical identity; repair the retained plan before retrying".into(),
        ));
    }
    Ok(plan)
}

fn plan_view(record: &PlanRecord) -> Result<VehicleMissionPlan> {
    let plan: VehicleMissionPlan = serde_json::from_str(&record.canonical_json)?;
    let state = match plan.state {
        MissionPlanLifecycle::Prepared => "prepared",
        MissionPlanLifecycle::Executing => "executing",
        MissionPlanLifecycle::Completed => "completed",
        MissionPlanLifecycle::Failed => "failed",
        MissionPlanLifecycle::Cancelled => "cancelled",
    };
    if plan.plan_id.as_str() != record.plan_id
        || plan.mission_id.as_str() != record.mission_id
        || plan.principal_key != record.principal_key
        || plan.session_id.as_str() != record.session_id
        || plan.vehicle_id.as_str() != record.vehicle_id
        || plan.map_route.route_uri != record.map_route_uri
        || plan.map_route.route_digest_sha256 != record.map_route_digest_sha256
        || plan.map_route.mobility_profile_uri.as_str() != record.map_mobility_profile_uri
        || state != record.state
        || checked_i64(plan.revision)? != record.revision
        || plan.expires_at != record.expires_at
        || plan.created_at != record.created_at
        || plan.updated_at != record.updated_at
    {
        return Err(ControlAuthorityError::Invalid(
            "persisted mission plan disagrees with indexed metadata; repair the retained plan before retrying".into(),
        ));
    }
    RouteRequirement::new(&plan.map_route)?;
    Ok(plan)
}

fn permission_strings(permissions: &BTreeSet<VehicleControlPermission>) -> Vec<String> {
    permissions
        .iter()
        .map(|permission| match permission {
            VehicleControlPermission::Inspect => "inspect",
            VehicleControlPermission::Plan => "plan",
            VehicleControlPermission::Execute => "execute",
            VehicleControlPermission::Abort => "abort",
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn context_records(identity: &GatewayInternalIdentity) -> Result<(RecordId, RecordId)> {
    let tenant_key = identity.authority.tenant.to_string();
    let work_context_key = identity.authority.work_context.to_string();
    Ok((
        deterministic_tenant_id(&tenant_key)?.record_id(),
        deterministic_work_context_id(&tenant_key, &work_context_key)?.record_id(),
    ))
}

fn scoped_record_id(
    table: &'static str,
    identity: &GatewayInternalIdentity,
    local_id: &str,
) -> RecordId {
    scoped_context_record_id(
        table,
        &identity.authority.tenant,
        &identity.authority.work_context,
        local_id,
    )
}

fn scoped_context_record_id(
    table: &str,
    tenant: &veoveo_types::TenantId,
    context: &veoveo_types::WorkContextId,
    local_id: &str,
) -> RecordId {
    let key = hex::encode(Sha256::digest(
        format!("{tenant}:{context}:{local_id}").as_bytes(),
    ));
    RecordId::new(table, key)
}

fn vehicle_lease_record_id(
    identity: &GatewayInternalIdentity,
    session_id: &SessionId,
    vehicle_id: &VehicleId,
) -> RecordId {
    scoped_record_id(
        "uav_vehicle_command_lease",
        identity,
        &format!("{session_id}:{vehicle_id}"),
    )
}

fn single_resource_uri(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && !suffix.contains('/'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn checked_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| ControlAuthorityError::Conflict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_validation_is_strict() {
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(!valid_sha256(&"g".repeat(64)));
    }
}
