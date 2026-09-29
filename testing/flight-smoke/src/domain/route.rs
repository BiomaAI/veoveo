//! The Map server admits the route; the flight client checks its returned contract.
use super::*;
use veoveo_map_mcp::contract::{
    MapFamily, RouteConstraints, RouteDataPolicy, RouteEndpoint, RouteObjective,
    RouteObjectiveKind, RoutePlan, RouteRequest, RouteStatus, Wgs84Position as MapPosition,
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
    let route: RoutePlan =
        serde_json::from_value(value).context("decoding the Map-owned route result")?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
