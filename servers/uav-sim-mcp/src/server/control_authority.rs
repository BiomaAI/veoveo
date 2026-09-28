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

mod map_handoff;
mod reads;
use map_handoff::{RouteRequirement, validate_map_handoff};
#[cfg(test)]
mod map_tests;
pub(super) use reads::{ControlCollection, grant_collection};

const PLAN_TTL: Duration = Duration::minutes(15);
const COMMAND_LEASE_TTL: Duration = Duration::hours(1);

#[derive(Clone)]
pub(super) struct VehicleControlAuthority {
    store: PlatformStore,
}

#[derive(Clone, Debug)]
pub(super) struct MissionExecutionGuard {
    plan_record_id: RecordId,
    plan_id: MissionPlanId,
    lease: CommandLease,
}

impl MissionExecutionGuard {
    pub(super) fn plan_id(&self) -> &MissionPlanId {
        &self.plan_id
    }
}

#[derive(Clone, Debug)]
struct CommandLease {
    record_id: RecordId,
    lease_token: String,
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
        "vehicle grant changed since you read it; read it again with `list_active_vehicle_control_grants` and retry with its current revision"
    )]
    Conflict,
    #[error(
        "vehicle `{0}` is already flying a mission; wait for that mission to finish or cancel its Task, then retry"
    )]
    VehicleBusy(String),
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
    canonical_json: String,
    expires_at: DateTime<Utc>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct LeaseRecord {
    id: RecordId,
    tenant: RecordId,
    work_context: RecordId,
    session_id: String,
    vehicle_id: String,
    principal_key: String,
    mission_id: String,
    lease_token: String,
    expires_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct LeaseContent {
    tenant: RecordId,
    work_context: RecordId,
    session_id: String,
    vehicle_id: String,
    principal_key: String,
    mission_id: String,
    lease_token: String,
    expires_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
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

    pub(super) async fn begin_execution(
        &self,
        identity: &GatewayInternalIdentity,
        plan_id: &MissionPlanId,
        expected_revision: u64,
    ) -> Result<(VehicleMissionPlan, MissionExecutionGuard)> {
        let record_id = scoped_record_id("uav_vehicle_mission_plan", identity, plan_id.as_str());
        let record = self.plan_record(&record_id).await?;
        let (tenant, context) = context_records(identity)?;
        if record.tenant != tenant
            || record.work_context != context
            || record.principal_key != identity.actor.id.as_str()
            || record.state != "prepared"
            || record.revision != checked_i64(expected_revision)?
            || record.expires_at <= Utc::now()
        {
            return Err(ControlAuthorityError::Conflict);
        }
        let plan = visible_plan_view(&record, identity)?;
        if &plan.plan_id != plan_id {
            return Err(ControlAuthorityError::Conflict);
        }
        self.require_route_permission(
            identity,
            &plan.session_id,
            &plan.vehicle_id,
            VehicleControlPermission::Execute,
            &plan.map_route,
        )
        .await?;
        let lease = self.acquire_lease(identity, &plan).await?;
        self.admit_execution(identity, record, plan, lease).await
    }

    async fn admit_execution(
        &self,
        identity: &GatewayInternalIdentity,
        record: PlanRecord,
        mut plan: VehicleMissionPlan,
        lease: CommandLease,
    ) -> Result<(VehicleMissionPlan, MissionExecutionGuard)> {
        let (tenant, context) = context_records(identity)?;
        let now = Utc::now();
        let route = RouteRequirement::new(&plan.map_route)?;
        let profile = route.profile.as_str().to_owned();
        let advisory = route.advisory;
        plan.state = MissionPlanLifecycle::Executing;
        plan.revision += 1;
        plan.updated_at = now;
        let permitted = reads::PERMITTED;
        let mut response = self
            .store
            .client()
            .query(format!("UPDATE ONLY $record SET state = 'executing', canonical_json = $canonical, updated_at = $now, revision += 1
                WHERE tenant = $tenant AND work_context = $work_context AND principal_key = $principal
                AND state = 'prepared' AND revision = $revision AND expires_at > $now AND canonical_json = $previous
                AND array::len((SELECT VALUE id FROM uav_vehicle_control_grant WHERE {permitted} LIMIT 1)) = 1
                RETURN AFTER;"))
            .bind(("record", record.id.clone()))
            .bind(("canonical", serde_json::to_string(&plan)?))
            .bind(("previous", record.canonical_json))
            .bind(("now", now))
            .bind(("revision", record.revision))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string()))
            .bind(("simulation_session", plan.session_id.to_string()))
            .bind(("vehicle", plan.vehicle_id.to_string()))
            .bind(("permissions", permission_strings(&BTreeSet::from([VehicleControlPermission::Execute]))))
            .bind(("profile", profile))
            .bind(("advisory", advisory))
            .await?
            .check()?;
        let updated: Option<PlanRecord> = response.take(0)?;
        if updated.is_none() {
            self.release_lease(&lease).await?;
            return Err(ControlAuthorityError::Conflict);
        }
        let guard = MissionExecutionGuard {
            plan_record_id: record.id,
            plan_id: plan.plan_id.clone(),
            lease,
        };
        Ok((plan, guard))
    }

    pub(super) async fn finish_execution(
        &self,
        guard: &MissionExecutionGuard,
        succeeded: bool,
    ) -> Result<()> {
        let record = self.plan_record(&guard.plan_record_id).await?;
        let mut plan = plan_view(&record)?;
        plan.state = if succeeded {
            MissionPlanLifecycle::Completed
        } else {
            MissionPlanLifecycle::Failed
        };
        plan.revision =
            u64::try_from(record.revision).map_err(|_| ControlAuthorityError::Conflict)? + 1;
        plan.updated_at = Utc::now();
        self.store
            .client()
            .query("UPDATE ONLY $record SET state = $state, canonical_json = $canonical, updated_at = $now, revision += 1 WHERE state = 'executing' AND revision = $revision RETURN NONE;")
            .bind(("record", guard.plan_record_id.clone()))
            .bind(("state", if succeeded { "completed" } else { "failed" }))
            .bind(("canonical", serde_json::to_string(&plan)?))
            .bind(("now", plan.updated_at))
            .bind(("revision", record.revision))
            .await?
            .check()?;
        self.release_lease(&guard.lease).await
    }

    async fn acquire_lease(
        &self,
        identity: &GatewayInternalIdentity,
        plan: &VehicleMissionPlan,
    ) -> Result<CommandLease> {
        let (tenant, work_context) = context_records(identity)?;
        let record_id = vehicle_lease_record_id(identity, &plan.session_id, &plan.vehicle_id);
        let now = Utc::now();
        let lease_token = Uuid::now_v7().to_string();
        let content = LeaseContent {
            tenant,
            work_context,
            session_id: plan.session_id.to_string(),
            vehicle_id: plan.vehicle_id.to_string(),
            principal_key: identity.actor.id.to_string(),
            mission_id: plan.mission_id.to_string(),
            lease_token: lease_token.clone(),
            expires_at: now + COMMAND_LEASE_TTL,
            released_at: None,
            revision: 0,
            created_at: now,
            updated_at: now,
        };
        let mut existing_response = self
            .store
            .client()
            .query("SELECT * FROM ONLY $record;")
            .bind(("record", record_id.clone()))
            .await?
            .check()?;
        let existing: Option<LeaseRecord> = existing_response.take(0)?;
        if let Some(existing) = existing.as_ref()
            && existing.released_at.is_none()
            && existing.expires_at > now
            && self.lease_has_executing_plan(existing).await?
        {
            return Err(ControlAuthorityError::VehicleBusy(
                plan.vehicle_id.to_string(),
            ));
        }
        let expected_revision = existing.as_ref().map_or(-1, |lease| lease.revision);
        let query = if existing.is_some() {
            "UPDATE ONLY $record CONTENT $content WHERE revision = $revision RETURN AFTER;"
        } else {
            "CREATE ONLY $record CONTENT $content RETURN AFTER;"
        };
        let mut request = self
            .store
            .client()
            .query(query)
            .bind(("record", record_id.clone()))
            .bind(("content", content));
        if expected_revision >= 0 {
            request = request.bind(("revision", expected_revision));
        }
        let mut response = request.await?.check()?;
        let acquired: Option<LeaseRecord> = response.take(0)?;
        if acquired.is_none() {
            return Err(ControlAuthorityError::VehicleBusy(
                plan.vehicle_id.to_string(),
            ));
        }
        Ok(CommandLease {
            record_id,
            lease_token,
        })
    }

    async fn release_lease(&self, lease: &CommandLease) -> Result<()> {
        let now = Utc::now();
        self.store
            .client()
            .query("UPDATE ONLY $record SET released_at = $now, updated_at = $now, revision += 1 WHERE lease_token = $lease_token AND released_at = NONE RETURN NONE;")
            .bind(("record", lease.record_id.clone()))
            .bind(("lease_token", lease.lease_token.clone()))
            .bind(("now", now))
            .await?
            .check()?;
        Ok(())
    }

    async fn lease_has_executing_plan(&self, lease: &LeaseRecord) -> Result<bool> {
        let mut response = self
            .store
            .client()
            .query("SELECT VALUE count() FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $work_context AND session_id = $session_id AND vehicle_id = $vehicle_id AND principal_key = $principal_key AND mission_id = $mission_id AND state = 'executing' GROUP ALL;")
            .bind(("tenant", lease.tenant.clone()))
            .bind(("work_context", lease.work_context.clone()))
            .bind(("session_id", lease.session_id.clone()))
            .bind(("vehicle_id", lease.vehicle_id.clone()))
            .bind(("principal_key", lease.principal_key.clone()))
            .bind(("mission_id", lease.mission_id.clone()))
            .await?
            .check()?;
        let counts: Vec<i64> = response.take(0)?;
        Ok(counts.into_iter().next().unwrap_or_default() > 0)
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
    let key = hex::encode(Sha256::digest(
        format!(
            "{}:{}:{local_id}",
            identity.authority.tenant, identity.authority.work_context
        )
        .as_bytes(),
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
