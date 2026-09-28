//! SQL owns grant/plan visibility and selection before page and completion limits.
use super::*;
use crate::{
    contract::{CollectionPage, UavGrantCursor, UavPlanCursor},
    server::index,
};

const VISIBLE: &str = "tenant = $tenant AND work_context = $work_context AND ($include_all OR principal_key = $principal)";
const ACTIVE: &str =
    "revoked_at = NONE AND valid_from <= $now AND (valid_until = NONE OR valid_until > $now)";

// Shared by grant selection and the execution-admission UPDATE. Values are bound
// at the driver boundary; this predicate contains no caller-supplied SQL.
pub(super) const PERMITTED: &str = "tenant = $tenant AND work_context = $work_context
    AND principal_key = $principal AND session_id = $simulation_session AND vehicle_id = $vehicle
    AND revoked_at = NONE AND valid_from <= $now AND (valid_until = NONE OR valid_until > $now)
    AND permissions CONTAINSALL $permissions
    AND ($profile = NONE OR map_mobility_profile_uri = $profile)
    AND ($advisory = false OR allow_planning_advisory = true)";

impl VehicleControlAuthority {
    pub(in crate::server) async fn grants_page(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
        active_session: Option<&SessionId>,
        after: Option<&ControlGrantId>,
    ) -> Result<CollectionPage<VehicleControlGrant>> {
        let (tenant, context) = context_records(identity)?;
        let mut response = self
            .store
            .client()
            .query(format!(
                "SELECT * FROM uav_vehicle_control_grant WHERE {VISIBLE}
             AND ($simulation_session = NONE OR (session_id = $simulation_session AND {ACTIVE}))
             AND ($after = NONE OR grant_id > $after) ORDER BY grant_id ASC LIMIT $limit;"
            ))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string()))
            .bind(("include_all", include_all))
            .bind((
                "simulation_session",
                active_session.map(ToString::to_string),
            ))
            .bind(("now", Utc::now()))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", index::PAGE_SIZE + 1))
            .await?
            .check()?;
        let records: Vec<GrantRecord> = response.take(0)?;
        index::page(
            records,
            |row| {
                Ok(
                    UavGrantCursor::new(
                        active_session,
                        ControlGrantId::new(row.grant_id.clone())?,
                    )?
                    .as_str()
                    .to_owned(),
                )
            },
            |row| Ok(grant_view(row)?),
        )
        .map_err(ControlAuthorityError::Index)
    }

    pub(in crate::server) async fn plans_page(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
        after: Option<&MissionPlanId>,
    ) -> Result<CollectionPage<VehicleMissionPlan>> {
        let (tenant, context) = context_records(identity)?;
        let mut response = self
            .store
            .client()
            .query(format!(
                "SELECT * FROM uav_vehicle_mission_plan WHERE {VISIBLE}
             AND ($after = NONE OR plan_id > $after) ORDER BY plan_id ASC LIMIT $limit;"
            ))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string()))
            .bind(("include_all", include_all))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", index::PAGE_SIZE + 1))
            .await?
            .check()?;
        let records: Vec<PlanRecord> = response.take(0)?;
        index::page(
            records,
            |row| {
                Ok(
                    UavPlanCursor::new(MissionPlanId::new(row.plan_id.clone())?)?
                        .as_str()
                        .to_owned(),
                )
            },
            |row| Ok(visible_plan_view(&row, identity)?),
        )
        .map_err(ControlAuthorityError::Index)
    }

    pub(in crate::server) async fn visible_grant(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
        id: &ControlGrantId,
    ) -> Result<Option<VehicleControlGrant>> {
        let (tenant, context) = context_records(identity)?;
        let mut response = self
            .store
            .client()
            .query(format!("SELECT * FROM ONLY $record WHERE {VISIBLE};"))
            .bind((
                "record",
                scoped_record_id("uav_vehicle_control_grant", identity, id.as_str()),
            ))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string()))
            .bind(("include_all", include_all))
            .await?
            .check()?;
        response
            .take::<Option<GrantRecord>>(0)?
            .map(grant_view)
            .transpose()
    }

    pub(in crate::server) async fn visible_plan(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
        id: &MissionPlanId,
    ) -> Result<Option<VehicleMissionPlan>> {
        let (tenant, context) = context_records(identity)?;
        let mut response = self
            .store
            .client()
            .query(format!("SELECT * FROM ONLY $record WHERE {VISIBLE};"))
            .bind((
                "record",
                scoped_record_id("uav_vehicle_mission_plan", identity, id.as_str()),
            ))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string()))
            .bind(("include_all", include_all))
            .await?
            .check()?;
        response
            .take::<Option<PlanRecord>>(0)?
            .map(|row| visible_plan_view(&row, identity))
            .transpose()
    }

    pub(in crate::server) async fn require_permission(
        &self,
        identity: &GatewayInternalIdentity,
        session: &SessionId,
        vehicle: &VehicleId,
        permission: VehicleControlPermission,
    ) -> Result<VehicleControlGrant> {
        self.select_permission(identity, session, vehicle, permission, None)
            .await
    }

    pub(super) async fn require_route_permission(
        &self,
        identity: &GatewayInternalIdentity,
        session: &SessionId,
        vehicle: &VehicleId,
        permission: VehicleControlPermission,
        handoff: &MapRouteHandoff,
    ) -> Result<VehicleControlGrant> {
        self.select_permission(
            identity,
            session,
            vehicle,
            permission,
            Some(RouteRequirement::new(handoff)?),
        )
        .await
    }

    async fn select_permission(
        &self,
        identity: &GatewayInternalIdentity,
        session: &SessionId,
        vehicle: &VehicleId,
        permission: VehicleControlPermission,
        route: Option<RouteRequirement<'_>>,
    ) -> Result<VehicleControlGrant> {
        let (tenant, context) = context_records(identity)?;
        let mut response = self
            .store
            .client()
            .query(format!(
                "SELECT * FROM uav_vehicle_control_grant WHERE {PERMITTED}
             ORDER BY created_at ASC, grant_id ASC LIMIT 1;"
            ))
            .bind(("tenant", tenant))
            .bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string()))
            .bind(("now", Utc::now()))
            .bind(("simulation_session", session.to_string()))
            .bind(("vehicle", vehicle.to_string()))
            .bind((
                "permissions",
                permission_strings(&BTreeSet::from([permission])),
            ))
            .bind((
                "profile",
                route
                    .as_ref()
                    .map(|route| route.profile.as_str().to_owned()),
            ))
            .bind(("advisory", route.is_some_and(|route| route.advisory)))
            .await?
            .check()?;
        let records: Vec<GrantRecord> = response.take(0)?;
        grant_view(
            records
                .into_iter()
                .next()
                .ok_or(ControlAuthorityError::Forbidden)?,
        )
    }

    pub(in crate::server) async fn inspectable_vehicles(
        &self,
        identity: &GatewayInternalIdentity,
        session: &SessionId,
        vehicles: &[VehicleId],
    ) -> Result<BTreeSet<VehicleId>> {
        let (tenant, context) = context_records(identity)?;
        #[derive(SurrealValue)]
        struct Vehicle {
            vehicle_id: String,
        }
        let mut response = self.store.client().query(format!(
            "SELECT vehicle_id FROM uav_vehicle_control_grant WHERE {VISIBLE} AND {ACTIVE}
             AND session_id = $simulation_session AND vehicle_id IN $vehicles AND permissions CONTAINS 'inspect'
             GROUP BY vehicle_id;"
        )).bind(("tenant", tenant)).bind(("work_context", context)).bind(("include_all", false))
            .bind(("principal", identity.actor.id.to_string())).bind(("now", Utc::now()))
            .bind(("simulation_session", session.to_string())).bind(("vehicles", vehicles.iter().map(ToString::to_string).collect::<Vec<_>>()))
            .await?.check()?;
        let records: Vec<Vehicle> = response.take(0)?;
        records
            .into_iter()
            .map(|row| {
                VehicleId::new(row.vehicle_id)
                    .map_err(|e| ControlAuthorityError::Invalid(e.to_string()))
            })
            .collect()
    }

    pub(in crate::server) async fn complete_ids(
        &self,
        identity: &GatewayInternalIdentity,
        include_all: bool,
        domain: ControlCollection,
        needle: &str,
    ) -> Result<Vec<String>> {
        let (tenant, context) = context_records(identity)?;
        let (table, field) = match domain {
            ControlCollection::Grants => ("uav_vehicle_control_grant", "grant_id"),
            ControlCollection::Plans => ("uav_vehicle_mission_plan", "plan_id"),
        };
        // Both SQL identifiers come only from the closed domain enum.
        #[derive(SurrealValue)]
        struct Completion {
            value: String,
        }
        let mut response = self.store.client().query(format!(
            "SELECT {field} AS value FROM {table} WHERE {VISIBLE}
             AND string::contains(string::lowercase({field}), $needle) ORDER BY value ASC LIMIT $limit;"
        )).bind(("tenant", tenant)).bind(("work_context", context))
            .bind(("principal", identity.actor.id.to_string())).bind(("include_all", include_all))
            .bind(("needle", needle.to_lowercase())).bind(("limit", index::PAGE_SIZE + 1))
            .await?.check()?;
        let rows: Vec<Completion> = response.take(0)?;
        Ok(rows.into_iter().map(|row| row.value).collect())
    }
}

pub(in crate::server) enum ControlCollection {
    Grants,
    Plans,
}
