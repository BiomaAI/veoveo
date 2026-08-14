use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{
    PlatformStore, deterministic_tenant_id, deterministic_work_context_id,
};

use crate::contract::{
    ControlGrantId, GrantVehicleControlRequest, MAP_ROUTE_HANDOFF_SCHEMA, MapRouteHandoffStatus,
    MissionPlanId, MissionPlanLifecycle, PrepareVehicleMissionRequest, RevokeVehicleControlRequest,
    SessionId, VehicleControlGrant, VehicleControlPermission, VehicleId, VehicleMissionPlan,
};

const PLAN_VALIDATION_MAX_AGE: Duration = Duration::minutes(5);
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
    #[error("vehicle control is not authorized for this principal")]
    Forbidden,
    #[error("vehicle control record was not found")]
    NotFound,
    #[error("vehicle control state changed concurrently")]
    Conflict,
    #[error("vehicle `{0}` already has an active command lease")]
    VehicleBusy(String),
    #[error(transparent)]
    Store(#[from] veoveo_platform_store::StoreError),
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
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
            map_mobility_profile_uri: request.map_mobility_profile_uri,
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

    pub(super) async fn visible_grants(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
    ) -> Result<Vec<VehicleControlGrant>> {
        let (tenant, work_context) = context_records(identity)?;
        let query = if include_all {
            "SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $work_context ORDER BY created_at ASC LIMIT 512;"
        } else {
            "SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $work_context AND principal_key = $principal ORDER BY created_at ASC LIMIT 512;"
        };
        let mut request = self
            .store
            .client()
            .query(query)
            .bind(("tenant", tenant))
            .bind(("work_context", work_context));
        if !include_all {
            request = request.bind(("principal", identity.actor.id.to_string()));
        }
        let mut response = request.await?.check()?;
        let records: Vec<GrantRecord> = response.take(0)?;
        records.into_iter().map(grant_view).collect()
    }

    pub(super) async fn active_visible_grants(
        &self,
        identity: &GatewayInternalIdentity,
        session_id: &SessionId,
        include_all: bool,
    ) -> Result<Vec<VehicleControlGrant>> {
        let now = Utc::now();
        Ok(self
            .visible_grants(identity, include_all)
            .await?
            .into_iter()
            .filter(|grant| {
                grant.session_id == *session_id
                    && grant.revoked_at.is_none()
                    && grant.valid_from <= now
                    && grant.valid_until.is_none_or(|until| now < until)
            })
            .collect())
    }

    pub(super) async fn require_permission(
        &self,
        identity: &GatewayInternalIdentity,
        session_id: &SessionId,
        vehicle_id: &VehicleId,
        permission: VehicleControlPermission,
    ) -> Result<VehicleControlGrant> {
        let now = Utc::now();
        self.visible_grants(identity, false)
            .await?
            .into_iter()
            .find(|grant| {
                grant.session_id == *session_id
                    && grant.vehicle_id == *vehicle_id
                    && grant.revoked_at.is_none()
                    && grant.valid_from <= now
                    && grant.valid_until.is_none_or(|until| now < until)
                    && grant.permissions.contains(&permission)
            })
            .ok_or(ControlAuthorityError::Forbidden)
    }

    pub(super) async fn prepare_plan(
        &self,
        identity: &GatewayInternalIdentity,
        request: PrepareVehicleMissionRequest,
    ) -> Result<VehicleMissionPlan> {
        let grant = self
            .require_permission(
                identity,
                &request.session_id,
                &request.vehicle_id,
                VehicleControlPermission::Plan,
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
            map_mobility_profile_uri: plan.map_route.mobility_profile_uri.clone(),
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
        if record.principal_key != identity.actor.id.as_str()
            || record.state != "prepared"
            || record.revision != checked_i64(expected_revision)?
            || record.expires_at <= Utc::now()
        {
            return Err(ControlAuthorityError::Conflict);
        }
        let plan: VehicleMissionPlan = serde_json::from_str(&record.canonical_json)?;
        self.require_permission(
            identity,
            &plan.session_id,
            &plan.vehicle_id,
            VehicleControlPermission::Execute,
        )
        .await?;
        let lease = self.acquire_lease(identity, &plan).await?;
        let now = Utc::now();
        let mut updated_plan = plan;
        updated_plan.state = MissionPlanLifecycle::Executing;
        updated_plan.revision += 1;
        updated_plan.updated_at = now;
        let mut response = self
            .store
            .client()
            .query("UPDATE ONLY $record SET state = 'executing', canonical_json = $canonical, updated_at = $now, revision += 1 WHERE state = 'prepared' AND revision = $revision RETURN AFTER;")
            .bind(("record", record_id.clone()))
            .bind(("canonical", serde_json::to_string(&updated_plan)?))
            .bind(("now", now))
            .bind(("revision", checked_i64(expected_revision)?))
            .await?
            .check()?;
        let updated: Option<PlanRecord> = response.take(0)?;
        if updated.is_none() {
            self.release_lease(&lease).await?;
            return Err(ControlAuthorityError::Conflict);
        }
        Ok((
            updated_plan,
            MissionExecutionGuard {
                plan_record_id: record_id,
                plan_id: plan_id.clone(),
                lease,
            },
        ))
    }

    pub(super) async fn finish_execution(
        &self,
        guard: &MissionExecutionGuard,
        succeeded: bool,
    ) -> Result<()> {
        let record = self.plan_record(&guard.plan_record_id).await?;
        let mut plan: VehicleMissionPlan = serde_json::from_str(&record.canonical_json)?;
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

    pub(super) async fn visible_plans(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
    ) -> Result<Vec<VehicleMissionPlan>> {
        let (tenant, work_context) = context_records(identity)?;
        let query = if include_all {
            "SELECT * FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $work_context ORDER BY created_at ASC LIMIT 512;"
        } else {
            "SELECT * FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $work_context AND principal_key = $principal ORDER BY created_at ASC LIMIT 512;"
        };
        let mut request = self
            .store
            .client()
            .query(query)
            .bind(("tenant", tenant))
            .bind(("work_context", work_context));
        if !include_all {
            request = request.bind(("principal", identity.actor.id.to_string()));
        }
        let mut response = request.await?.check()?;
        let records: Vec<PlanRecord> = response.take(0)?;
        records
            .into_iter()
            .map(|record| serde_json::from_str(&record.canonical_json).map_err(Into::into))
            .collect()
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
    validate_map_profile_uri(&request.map_mobility_profile_uri)?;
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

fn validate_map_handoff(
    request: &PrepareVehicleMissionRequest,
    grant: &VehicleControlGrant,
) -> Result<()> {
    let handoff = &request.map_route;
    if handoff.schema_profile != MAP_ROUTE_HANDOFF_SCHEMA {
        return Err(ControlAuthorityError::Invalid(
            "Map route handoff uses an unsupported schema profile".to_owned(),
        ));
    }
    if handoff.mobility_profile_uri != grant.map_mobility_profile_uri {
        return Err(ControlAuthorityError::Invalid(
            "Map route mobility profile does not match the vehicle grant".to_owned(),
        ));
    }
    if handoff.route_status == MapRouteHandoffStatus::PlanningAdvisory
        && !grant.allow_planning_advisory
    {
        return Err(ControlAuthorityError::Invalid(
            "the vehicle grant does not admit planning-advisory Map routes".to_owned(),
        ));
    }
    if !single_resource_uri(&handoff.route_uri, "map://route/")
        || !valid_sha256(&handoff.route_digest_sha256)
        || handoff.path.len() < 2
        || handoff.path.len() > 10_000
    {
        return Err(ControlAuthorityError::Invalid(
            "Map route handoff identity, digest, or path bounds are invalid".to_owned(),
        ));
    }
    if handoff.path.iter().any(|position| {
        !position.longitude_deg.is_finite()
            || !(-180.0..=180.0).contains(&position.longitude_deg)
            || !position.latitude_deg.is_finite()
            || !(-90.0..=90.0).contains(&position.latitude_deg)
            || position
                .ellipsoidal_height_m
                .is_none_or(|height| !height.is_finite())
    }) {
        return Err(ControlAuthorityError::Invalid(
            "every executable Map route position requires valid ellipsoidal height".to_owned(),
        ));
    }
    let now = Utc::now();
    if handoff.validated_at < now - PLAN_VALIDATION_MAX_AGE
        || handoff.validated_at > now + Duration::seconds(30)
        || handoff.prepared_at < handoff.validated_at
        || handoff.prepared_at > now + Duration::seconds(30)
    {
        return Err(ControlAuthorityError::Invalid(
            "Map route handoff validation is stale or temporally inconsistent".to_owned(),
        ));
    }
    if !request.speed_mps.is_finite()
        || !(0.1..=100.0).contains(&request.speed_mps)
        || !request.hold_seconds_at_destination.is_finite()
        || !(0.0..=3_600.0).contains(&request.hold_seconds_at_destination)
    {
        return Err(ControlAuthorityError::Invalid(
            "mission speed or destination hold is outside UAV bounds".to_owned(),
        ));
    }
    Ok(())
}

fn validate_map_profile_uri(value: &str) -> Result<()> {
    let Some(rest) = value.strip_prefix("map://mobility-profile/") else {
        return Err(ControlAuthorityError::Invalid(
            "map_mobility_profile_uri must use the canonical Map resource".to_owned(),
        ));
    };
    let Some((profile, version)) = rest.split_once('/') else {
        return Err(ControlAuthorityError::Invalid(
            "map_mobility_profile_uri must name one exact profile version".to_owned(),
        ));
    };
    if profile.is_empty()
        || profile.contains('/')
        || version.parse::<u64>().is_err()
        || version.contains('/')
    {
        return Err(ControlAuthorityError::Invalid(
            "map_mobility_profile_uri must name one exact profile version".to_owned(),
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
        map_mobility_profile_uri: record.map_mobility_profile_uri,
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
    fn profile_and_digest_validation_is_strict() {
        assert!(validate_map_profile_uri("map://mobility-profile/uas-demo/1").is_ok());
        assert!(validate_map_profile_uri("map://mobility-profile/uas-demo").is_err());
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(!valid_sha256(&"g".repeat(64)));
    }
}
