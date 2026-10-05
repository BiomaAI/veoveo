//! The Map server admits the route; the flight client checks its returned contract.
use super::*;
use veoveo_map_mcp::contract::{
    MapFamily, MapRouteHandoff, MapTaskProduct, PrepareRouteHandoffRequest, RouteConstraints,
    RouteDataPolicy, RouteEndpoint, RouteObjective, RouteObjectiveKind, RoutePlan, RouteRequest,
    RouteStatus, Wgs84Position as MapPosition,
};
use veoveo_uav_sim_mcp::contract::{
    ExecuteVehicleMissionPlanRequest, MissionId, MissionLifecycle, MissionPlanLifecycle,
    MissionResult, PrepareVehicleMissionRequest, SimulationState, VehicleFlightState,
    VehicleMissionPlan, VehicleState,
};

pub(crate) async fn verify(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    installation.operator.validate_credentials()?;
    preflight(
        &OperatorClient {
            conformance,
            installation,
        },
        &scenario,
    )
    .await
}

pub(super) fn request(
    profile: &MapMobilityProfileUri,
    origin: &Wgs84Position,
    destination: &Wgs84Position,
) -> Result<RouteRequest> {
    let position = |value: &Wgs84Position| {
        MapPosition::new(
            value.longitude_degrees,
            value.latitude_degrees,
            Some(value.ellipsoid_height_m),
        )
        .map(|position| RouteEndpoint::Position { position })
    };
    Ok(RouteRequest {
        mobility_profile_id: profile.id().clone(),
        mobility_profile_version: profile.version(),
        origin: position(origin)?,
        destination: position(destination)?,
        waypoints: Vec::new(),
        departure_time: Utc::now(),
        alternatives: 0,
        objective: RouteObjective {
            kind: RouteObjectiveKind::Shortest,
            weights: None,
        },
        constraints: RouteConstraints {
            required_areas: Vec::new(),
            avoided_areas: Vec::new(),
            required_facility_stops: Vec::new(),
            latest_arrival: None,
            minimum_energy_reserve: None,
            required_authority_classes: BTreeSet::new(),
        },
        data_policy: RouteDataPolicy {
            allow_planning_advisory: true,
            allow_stale_operational_data: false,
            required_map_families: BTreeSet::from([MapFamily::Aviation]),
        },
    })
}

pub(super) async fn plan(
    operator: &OperatorClient<'_>,
    profile: &MapMobilityProfileUri,
    origin: &Wgs84Position,
    destination: &Wgs84Position,
    timeout: Duration,
) -> Result<RoutePlan> {
    let request = request(profile, origin, destination)?;
    let value = operator
        .task_tool("map__route", serde_json::to_value(&request)?, timeout)
        .await?;
    let product: MapTaskProduct<RoutePlan> =
        serde_json::from_value(value).context("decoding the Map-owned route result")?;
    let route = product.into_output();
    ensure!(
        route.mobility_profile_id == request.mobility_profile_id
            && route.mobility_profile_version == request.mobility_profile_version
            && route.departure_time == request.departure_time
            && matches!(
                route.status,
                RouteStatus::Validated | RouteStatus::PlanningAdvisory
            )
            && !route.legs.is_empty()
            && !route.provenance.base_release_ids.is_empty()
            && route
                .legs
                .iter()
                .all(|leg| leg.map_family == MapFamily::Aviation
                    && !leg.source_release_ids.is_empty()
                    && leg.geometry.validate().is_ok()),
        "Map returned an unusable aviation route or mismatched profile: {route:?}"
    );
    Ok(route)
}

pub(super) async fn preflight(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<()> {
    let mut origin = scenario.world.origin()?.clone();
    origin.ellipsoid_height_m += scenario.takeoff.relative_altitude_m;
    let destination = nearby_mission_position(&origin, scenario.mission.longitude_offset_degrees)?;
    plan(operator, &scenario.map_mobility_profile_uri, &origin, &destination, Duration::from_secs(120))
        .await.context("flight Map prerequisite failed before flight commands; acquire and activate the scenario's aviation source, then run uav-route-verify")?;
    println!(
        "UAV Map route prerequisite passed for {}",
        scenario.map_mobility_profile_uri.as_str()
    );
    Ok(())
}

pub(super) async fn execute(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision: &FrameWorldRevisionUri,
    route: &RoutePlan,
) -> Result<()> {
    let handoff: MapRouteHandoff = serde_json::from_value(
        operator
            .call_tool(
                "map__prepare_route_handoff",
                serde_json::to_value(PrepareRouteHandoffRequest {
                    route_id: route.route_id.clone(),
                })?,
            )
            .await?,
    )?;
    let request = PrepareVehicleMissionRequest {
        session_id: scenario.session_id.clone(),
        mission_id: MissionId::parse(format!("acceptance-{}", uuid::Uuid::now_v7()))?,
        vehicle_id: scenario.vehicle_id.clone(),
        expected_world_revision_uri: revision.clone(),
        map_route: handoff,
        speed_mps: scenario.mission.speed_mps,
        hold_seconds_at_destination: scenario.mission.hold_seconds,
    };
    let plan: VehicleMissionPlan = serde_json::from_value(
        operator
            .call_tool(
                "uav-sim__prepare_vehicle_mission",
                serde_json::to_value(&request)?,
            )
            .await?,
    )?;
    ensure!(
        plan.session_id == request.session_id
            && plan.vehicle_id == request.vehicle_id
            && plan.mission_id == request.mission_id
            && plan.expected_world_revision_uri == request.expected_world_revision_uri
            && plan.map_route == request.map_route
            && plan.speed_mps == request.speed_mps
            && plan.hold_seconds_at_destination == request.hold_seconds_at_destination
            && plan.state == MissionPlanLifecycle::Prepared,
        "prepared mission does not match the admitted route and vehicle"
    );
    let timeout = governed_mission_timeout(
        &route.summary,
        scenario.mission.speed_mps,
        scenario.mission.task_timeout_seconds,
    )?;
    let result: MissionResult = serde_json::from_value(
        operator
            .task_tool(
                "uav-sim__execute_vehicle_mission_plan",
                serde_json::to_value(ExecuteVehicleMissionPlanRequest {
                    plan_id: plan.plan_id,
                    expected_revision: plan.revision,
                })?,
                timeout,
            )
            .await?,
    )?;
    ensure!(
        result.mission_id == request.mission_id
            && result.lifecycle == MissionLifecycle::Completed
            && result.completed_waypoints > 0,
        "UAV did not complete its selected mission: {result:?}"
    );
    Ok(())
}

fn launch_return_destination(
    launch: &Wgs84Position,
    current: &Wgs84Position,
) -> Result<Wgs84Position> {
    launch.validate().map_err(anyhow::Error::msg)?;
    current.validate().map_err(anyhow::Error::msg)?;
    // The route returns horizontally at flight altitude. Landing owns descent.
    let mut destination = launch.clone();
    destination.ellipsoid_height_m = current.ellipsoid_height_m;
    Ok(destination)
}

async fn selected_vehicle(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision: &FrameWorldRevisionUri,
) -> Result<VehicleState> {
    let state: SimulationState =
        serde_json::from_value(simulation_state(operator, scenario).await?)?;
    ensure!(
        state.session_id == scenario.session_id,
        "UAV returned another session"
    );
    ensure!(
        state
            .world
            .as_ref()
            .is_some_and(|world| world.revision_uri == *revision),
        "UAV launch-site observation belongs to another world revision"
    );
    state
        .vehicles
        .into_iter()
        .find(|v| v.vehicle_id == scenario.vehicle_id)
        .context("UAV state omitted the selected vehicle")
}

fn within_launch_surface(vehicle: &VehicleState) -> Result<()> {
    // The reference's 40 m square launch surface is centered on the world origin.
    // Its inscribed circle gives landing a finite, conservative horizontal bound.
    let distance = vehicle.enu.east_m.hypot(vehicle.enu.north_m);
    ensure!(
        distance.is_finite() && distance <= 20.0,
        "selected UAV is {distance:.2} m from its launch site; landing acceptance requires at most 20 m"
    );
    Ok(())
}

pub(super) async fn return_to_launch(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision: &FrameWorldRevisionUri,
    profile: &MapMobilityProfileUri,
) -> Result<()> {
    let vehicle = selected_vehicle(operator, scenario, revision).await?;
    ensure!(
        vehicle.flight_state == VehicleFlightState::Flying,
        "return to launch requires the selected UAV to be flying"
    );
    let destination = launch_return_destination(scenario.world.origin()?, &vehicle.wgs84)?;
    let route = plan(
        operator,
        profile,
        &vehicle.wgs84,
        &destination,
        Duration::from_secs(scenario.mission.task_timeout_seconds),
    )
    .await?;
    execute(operator, scenario, revision, &route).await?;
    within_launch_surface(&selected_vehicle(operator, scenario, revision).await?)?;
    eprintln!("UAV completed its Map-admitted return to the launch site before landing");
    Ok(())
}

pub(super) async fn assert_landed_at_launch(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision: &FrameWorldRevisionUri,
) -> Result<()> {
    let vehicle = selected_vehicle(operator, scenario, revision).await?;
    ensure!(
        matches!(
            vehicle.flight_state,
            VehicleFlightState::Landed | VehicleFlightState::Standby
        ),
        "selected UAV has not completed landing"
    );
    within_launch_surface(&vehicle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_route_keeps_flight_altitude_above_the_launch_site() {
        let launch = Wgs84Position {
            latitude_degrees: 40.758,
            longitude_degrees: -73.9855,
            ellipsoid_height_m: -17.0,
        };
        let current = Wgs84Position {
            latitude_degrees: 40.763,
            longitude_degrees: -73.976,
            ellipsoid_height_m: 180.0,
        };
        let destination = launch_return_destination(&launch, &current).unwrap();
        assert_eq!(destination.latitude_degrees, launch.latitude_degrees);
        assert_eq!(destination.longitude_degrees, launch.longitude_degrees);
        assert_eq!(destination.ellipsoid_height_m, current.ellipsoid_height_m);
        let invalid = Wgs84Position {
            ellipsoid_height_m: f64::NAN,
            ..current
        };
        assert!(launch_return_destination(&launch, &invalid).is_err());
    }

    #[test]
    fn landing_site_rejects_a_ground_level_vehicle_elsewhere_in_the_city() {
        let mut state: SimulationState =
            serde_json::from_str(include_str!("../../tests/fixtures/world-ready.json")).unwrap();
        let vehicle = &mut state.vehicles[0];
        vehicle.enu.east_m = 814.0;
        vehicle.enu.north_m = 536.0;
        vehicle.enu.up_m = 0.04;
        assert!(within_launch_surface(vehicle).is_err());
        vehicle.enu.east_m = 0.0;
        vehicle.enu.north_m = 20.0;
        within_launch_surface(vehicle).unwrap();
        vehicle.enu.north_m = 20.01;
        assert!(within_launch_surface(vehicle).is_err());
        vehicle.enu.north_m = f64::NAN;
        assert!(within_launch_surface(vehicle).is_err());
    }

    #[test]
    fn route_admission_keeps_profile_altitude_and_fresh_aviation_policy() {
        let profile = MapMobilityProfileUri::parse(
            "map://mobility-profile/mobility-019ffdb2-0598-7476-96d3-f3d7b0769f9e/1",
        )
        .unwrap();
        let origin = Wgs84Position {
            latitude_degrees: 40.758,
            longitude_degrees: -73.9855,
            ellipsoid_height_m: 180.0,
        };
        let destination = nearby_mission_position(&origin, 0.0002).unwrap();
        let request = request(&profile, &origin, &destination).unwrap();
        assert_eq!(request.mobility_profile_id, *profile.id());
        assert_eq!(request.mobility_profile_version, profile.version());
        assert_eq!(
            request.data_policy.required_map_families,
            BTreeSet::from([MapFamily::Aviation])
        );
        assert!(!request.data_policy.allow_stale_operational_data);
        let RouteEndpoint::Position { position } = request.destination else {
            panic!("expected position")
        };
        assert_eq!(position.ellipsoidal_height_m, Some(180.0));
        assert_eq!(position.longitude_deg, -73.9853);
    }
}
