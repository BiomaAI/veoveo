use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use chrono::TimeDelta;

use crate::{
    contract::{
        HumanMovementMode, MapFamily, Meters, MobilityProfile, Ratio, ReachableAreaRequest,
        ReachableBudget, RoadVehicleClass, RouteAlternative, RouteCost, RouteEndpoint,
        RouteInstruction, RouteLeg, RouteObjective, RouteObjectiveKind, RouteRequest, RouteStatus,
        Seconds, TravelTimeModel, Wgs84LineString, Wgs84Polygon, Wgs84Position,
    },
    routes::PlannerOutput,
};

use super::client::{
    BicycleOptions, CostingOptions, IsochroneContour, IsochroneRequest, Location, MatrixDateTime,
    MatrixLocation, MatrixRequest, MotorcycleOptions, MotorizedOptions, PedestrianOptions,
    RouteRequest as ValhallaRouteRequest, Trip, ValhallaClient,
};

#[derive(Clone, Debug)]
pub struct ValhallaPlanner {
    client: ValhallaClient,
}

#[derive(Clone, Debug)]
pub(in crate::routes) struct ValhallaMatrix {
    pub durations_seconds: Vec<f32>,
    pub distances_meters: Vec<f32>,
    pub unavailable_cells: Vec<u32>,
    pub algorithm: String,
}

impl ValhallaPlanner {
    pub fn new(client: ValhallaClient) -> Self {
        Self { client }
    }

    pub(in crate::routes) async fn plan(
        &self,
        request: &RouteRequest,
        profile: &MobilityProfile,
        positions: &[Wgs84Position],
        source_release_ids: &BTreeSet<crate::contract::DatasetReleaseId>,
    ) -> Result<PlannerOutput> {
        if positions.len() < 2 {
            bail!("Valhalla routing requires at least two resolved positions");
        }
        if request.alternatives > 0 && positions.len() > 2 {
            bail!("Valhalla alternatives are unavailable for routes with waypoints");
        }
        let (costing, options, map_family) = costing(profile, request)?;
        let exclude_polygons = request
            .constraints
            .avoided_areas
            .iter()
            .map(|polygon| {
                polygon.validate()?;
                Ok(polygon
                    .exterior
                    .iter()
                    .map(|position| [position.longitude_deg, position.latitude_deg])
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>>>()?;
        if !request.constraints.required_areas.is_empty() {
            bail!("required-area constraints are not supported by the land adapter");
        }
        let engine_request = ValhallaRouteRequest {
            locations: positions
                .iter()
                .map(|position| Location {
                    lat: position.latitude_deg,
                    lon: position.longitude_deg,
                    r#type: "break",
                })
                .collect(),
            costing,
            costing_options: options,
            units: "kilometers",
            language: "en-US",
            shape_format: "polyline6",
            alternates: request.alternatives,
            admin_crossings: true,
            exclude_polygons,
        };
        let response = self.client.route(&engine_request).await?;
        if response.trip.status != 0 {
            bail!(
                "Valhalla returned route status {}: {}",
                response.trip.status,
                response.trip.status_message
            );
        }
        let legs = trip_legs(&response.trip, map_family, source_release_ids)?;
        let arrival_time = TimeDelta::try_seconds(response.trip.summary.time.round() as i64)
            .map(|delta| request.departure_time + delta);
        let alternatives = response
            .alternates
            .iter()
            .enumerate()
            .map(|(index, alternate)| {
                let legs = trip_legs(&alternate.trip, map_family, source_release_ids)?;
                Ok(RouteAlternative {
                    rank: u16::try_from(index + 1).context("too many route alternatives")?,
                    summary: trip_cost(&alternate.trip)?,
                    legs,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(PlannerOutput {
            status: RouteStatus::PlanningAdvisory,
            legs,
            alternatives,
            arrival_time,
            crossed_boundary_ids: response
                .trip
                .admins
                .iter()
                .filter(|admin| !admin.country_code.is_empty() || !admin.state_code.is_empty())
                .map(|admin| format!("{}:{}", admin.country_code, admin.state_code))
                .collect(),
        })
    }

    pub(in crate::routes) async fn reachable_area(
        &self,
        request: &ReachableAreaRequest,
        profile: &MobilityProfile,
        origin: &Wgs84Position,
    ) -> Result<Vec<Wgs84Polygon>> {
        if !request.constraints.required_areas.is_empty()
            || !request.constraints.required_facility_stops.is_empty()
            || request.constraints.latest_arrival.is_some()
            || request.constraints.minimum_energy_reserve.is_some()
        {
            bail!(
                "reachable-area land routing does not support required areas, facility stops, arrival limits, or reserve constraints"
            );
        }
        let synthetic = RouteRequest {
            mobility_profile_id: request.mobility_profile_id.clone(),
            mobility_profile_version: request.mobility_profile_version,
            origin: RouteEndpoint::Position {
                position: origin.clone(),
            },
            destination: RouteEndpoint::Position {
                position: origin.clone(),
            },
            waypoints: Vec::new(),
            departure_time: request.departure_time,
            objective: RouteObjective {
                kind: RouteObjectiveKind::Fastest,
                weights: None,
            },
            constraints: request.constraints.clone(),
            alternatives: 0,
            data_policy: request.data_policy.clone(),
        };
        let (costing, costing_options, _) = costing(profile, &synthetic)?;
        let contour = match request.budget {
            ReachableBudget::Duration { value } => {
                let minutes = value.get() / 60.0;
                if !(1.0..=240.0).contains(&minutes) {
                    bail!("reachable duration must be within 1..=240 minutes");
                }
                IsochroneContour {
                    time: Some(minutes),
                    distance: None,
                }
            }
            ReachableBudget::Distance { value } => {
                let kilometers = value.get() / 1_000.0;
                if !(0.1..=1_000.0).contains(&kilometers) {
                    bail!("reachable distance must be within 0.1..=1000 kilometers");
                }
                IsochroneContour {
                    time: None,
                    distance: Some(kilometers),
                }
            }
        };
        let exclude_polygons = request
            .constraints
            .avoided_areas
            .iter()
            .map(|polygon| {
                polygon.validate()?;
                Ok(polygon
                    .exterior
                    .iter()
                    .map(|position| [position.longitude_deg, position.latitude_deg])
                    .collect())
            })
            .collect::<Result<Vec<_>>>()?;
        let response = self
            .client
            .isochrone(&IsochroneRequest {
                locations: vec![Location {
                    lat: origin.latitude_deg,
                    lon: origin.longitude_deg,
                    r#type: "break",
                }],
                costing,
                costing_options,
                contours: vec![contour],
                polygons: true,
                denoise: 1.0,
                generalize: request.generalization.map(|value| value.get()),
                exclude_polygons,
            })
            .await?;
        parse_isochrone_polygons(&response)
    }

    pub(in crate::routes) async fn matrix(
        &self,
        request: &RouteRequest,
        profile: &MobilityProfile,
        positions: &[Wgs84Position],
        time_model: TravelTimeModel,
        prioritize_bidirectional: bool,
    ) -> Result<ValhallaMatrix> {
        if positions.is_empty() {
            bail!("Valhalla travel model requires at least one location");
        }
        if !request.constraints.required_areas.is_empty()
            || !request.constraints.required_facility_stops.is_empty()
            || request.constraints.latest_arrival.is_some()
            || request.constraints.minimum_energy_reserve.is_some()
        {
            bail!(
                "travel-model matrices do not support required areas, facility stops, arrival limits, or reserve constraints"
            );
        }
        let (costing, costing_options, _) = costing(profile, request)?;
        let locations = positions
            .iter()
            .map(|position| MatrixLocation {
                lat: position.latitude_deg,
                lon: position.longitude_deg,
            })
            .collect::<Vec<_>>();
        let date_time = match time_model {
            TravelTimeModel::Static => None,
            TravelTimeModel::InvariantLocalDeparture { local_time } => Some(MatrixDateTime {
                r#type: 3,
                value: local_time.format("%Y-%m-%dT%H:%M").to_string(),
            }),
        };
        let exclude_polygons = request
            .constraints
            .avoided_areas
            .iter()
            .map(|polygon| {
                polygon.validate()?;
                Ok(polygon
                    .exterior
                    .iter()
                    .map(|position| [position.longitude_deg, position.latitude_deg])
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>>>()?;
        let response = self
            .client
            .matrix(&MatrixRequest {
                sources: locations.clone(),
                targets: locations,
                costing,
                costing_options,
                units: "kilometers",
                verbose: false,
                prioritize_bidirectional,
                date_time,
                exclude_polygons,
            })
            .await?;
        if response.units != "kilometers" {
            bail!(
                "Valhalla matrix returned unsupported units {}",
                response.units
            );
        }
        let dimension = positions.len();
        validate_matrix_shape(
            &response.sources_to_targets.durations,
            dimension,
            "duration",
        )?;
        validate_matrix_shape(
            &response.sources_to_targets.distances,
            dimension,
            "distance",
        )?;
        let mut durations_seconds = Vec::with_capacity(dimension * dimension);
        let mut distances_meters = Vec::with_capacity(dimension * dimension);
        let mut unavailable_cells = Vec::new();
        for row in 0..dimension {
            for column in 0..dimension {
                let duration = response.sources_to_targets.durations[row][column];
                let distance = response.sources_to_targets.distances[row][column];
                match (duration, distance) {
                    (Some(duration), Some(distance))
                        if duration.is_finite()
                            && duration >= 0.0
                            && distance.is_finite()
                            && distance >= 0.0 =>
                    {
                        durations_seconds.push(f32_matrix_value(duration, "duration")?);
                        distances_meters.push(f32_matrix_value(distance * 1_000.0, "distance")?);
                    }
                    (None, None) => {
                        unavailable_cells.push(u32::try_from(row * dimension + column)?);
                        durations_seconds.push(0.0);
                        distances_meters.push(0.0);
                    }
                    _ => bail!(
                        "Valhalla matrix cell {row},{column} has inconsistent or invalid values"
                    ),
                }
            }
        }
        Ok(ValhallaMatrix {
            durations_seconds,
            distances_meters,
            unavailable_cells,
            algorithm: response.algorithm,
        })
    }
}

fn validate_matrix_shape(rows: &[Vec<Option<f64>>], dimension: usize, field: &str) -> Result<()> {
    if rows.len() != dimension || rows.iter().any(|row| row.len() != dimension) {
        bail!("Valhalla {field} matrix does not match the requested square dimension");
    }
    Ok(())
}

fn f32_matrix_value(value: f64, field: &str) -> Result<f32> {
    if !value.is_finite() || value < 0.0 || value > f32::MAX as f64 {
        bail!("Valhalla {field} matrix value is outside float32 range");
    }
    Ok(value as f32)
}

fn parse_isochrone_polygons(value: &serde_json::Value) -> Result<Vec<Wgs84Polygon>> {
    let features = value
        .get("features")
        .and_then(serde_json::Value::as_array)
        .context("Valhalla isochrone omitted GeoJSON features")?;
    let mut result = Vec::new();
    for feature in features {
        let geometry = feature
            .get("geometry")
            .context("Valhalla isochrone feature omitted geometry")?;
        let geometry_type = geometry
            .get("type")
            .and_then(serde_json::Value::as_str)
            .context("Valhalla isochrone geometry omitted type")?;
        let coordinates = geometry
            .get("coordinates")
            .and_then(serde_json::Value::as_array)
            .context("Valhalla isochrone geometry omitted coordinates")?;
        match geometry_type {
            "Polygon" => result.push(parse_polygon(coordinates)?),
            "MultiPolygon" => {
                for polygon in coordinates {
                    result.push(parse_polygon(
                        polygon
                            .as_array()
                            .context("Valhalla multipolygon member is not an array")?,
                    )?);
                }
            }
            other => bail!("Valhalla isochrone returned unsupported geometry {other}"),
        }
    }
    if result.is_empty() {
        bail!("Valhalla isochrone returned no reachable polygons");
    }
    Ok(result)
}

fn parse_polygon(rings: &[serde_json::Value]) -> Result<Wgs84Polygon> {
    let mut rings = rings.iter().map(parse_ring);
    let exterior = rings
        .next()
        .context("Valhalla isochrone polygon has no exterior")??;
    let polygon = Wgs84Polygon {
        exterior,
        interiors: rings.collect::<Result<Vec<_>>>()?,
    };
    polygon.validate()?;
    Ok(polygon)
}

fn parse_ring(value: &serde_json::Value) -> Result<Vec<Wgs84Position>> {
    value
        .as_array()
        .context("Valhalla isochrone ring is not an array")?
        .iter()
        .map(|position| {
            let values = position
                .as_array()
                .context("Valhalla isochrone position is not an array")?;
            if values.len() < 2 {
                bail!("Valhalla isochrone position has fewer than two ordinates");
            }
            Wgs84Position::new(
                values[0]
                    .as_f64()
                    .context("Valhalla longitude is not numeric")?,
                values[1]
                    .as_f64()
                    .context("Valhalla latitude is not numeric")?,
                None,
            )
            .map_err(Into::into)
        })
        .collect()
}

fn costing(
    profile: &MobilityProfile,
    request: &RouteRequest,
) -> Result<(String, CostingOptions, MapFamily)> {
    let shortest = match request.objective.kind {
        RouteObjectiveKind::Fastest => false,
        RouteObjectiveKind::Shortest => true,
        RouteObjectiveKind::Weighted => false,
        RouteObjectiveKind::LowestEnergy
        | RouteObjectiveKind::LowestRisk
        | RouteObjectiveKind::LowestCost => {
            bail!("the land adapter cannot optimize this objective without a governed cost model")
        }
    };
    let mut options = CostingOptions::default();
    match profile {
        MobilityProfile::Human(profile) => {
            let speed_kph = profile.preferred_speed.get() * 3.6;
            if !(0.5..=25.0).contains(&speed_kph) {
                bail!("human preferred speed is outside Valhalla pedestrian limits");
            }
            options.pedestrian = Some(PedestrianOptions {
                walking_speed: Some(speed_kph),
                step_penalty: (!profile.stairs_allowed).then_some(43_200.0),
                r#type: matches!(
                    profile.mode,
                    HumanMovementMode::ManualMobilityAid | HumanMovementMode::PoweredMobilityAid
                )
                .then_some("wheelchair"),
                shortest: Some(shortest),
            });
            Ok(("pedestrian".to_owned(), options, MapFamily::ActiveMobility))
        }
        MobilityProfile::RoadVehicle(profile) => match profile.class {
            RoadVehicleClass::Bicycle => {
                options.bicycle = Some(BicycleOptions {
                    cycling_speed: Some(profile.performance.nominal_speed.get() * 3.6),
                    shortest: Some(shortest),
                });
                Ok(("bicycle".to_owned(), options, MapFamily::ActiveMobility))
            }
            RoadVehicleClass::PoweredTwoWheeler => {
                options.motorcycle = Some(MotorcycleOptions {
                    top_speed: bounded_top_speed(profile.performance.maximum_speed.get() * 3.6)?,
                    shortest: Some(shortest),
                });
                Ok(("motorcycle".to_owned(), options, MapFamily::RoadStreet))
            }
            class => {
                let truck = matches!(
                    class,
                    RoadVehicleClass::RigidTruck | RoadVehicleClass::ArticulatedTruck
                );
                let bus = matches!(class, RoadVehicleClass::BusCoach);
                let motorized = MotorizedOptions {
                    height: Some(profile.dimensions.height.get()),
                    width: Some(profile.dimensions.width.get()),
                    length: Some(profile.dimensions.length.get()),
                    weight: Some(profile.gross_mass.get() / 1_000.0),
                    axle_load: truck.then_some(profile.maximum_axle_load.get() / 1_000.0),
                    axle_count: truck.then_some(profile.axle_count),
                    hazmat: truck.then_some(profile.hazardous_cargo),
                    exclude_unpaved: Some(!profile.unpaved_allowed),
                    top_speed: bounded_top_speed(profile.performance.maximum_speed.get() * 3.6)?,
                    shortest: Some(shortest),
                    use_distance: weighted_distance(request)?,
                };
                let costing = if truck {
                    options.truck = Some(motorized);
                    "truck"
                } else if bus {
                    options.bus = Some(motorized);
                    "bus"
                } else {
                    options.auto = Some(motorized);
                    "auto"
                };
                Ok((costing.to_owned(), options, MapFamily::RoadStreet))
            }
        },
        _ => bail!("mobility profile is not compatible with the Valhalla land adapter"),
    }
}

fn bounded_top_speed(speed_kph: f64) -> Result<Option<f64>> {
    if !speed_kph.is_finite() || speed_kph <= 0.0 {
        bail!("vehicle maximum speed must be positive and finite");
    }
    if !(10.0..=252.0).contains(&speed_kph) {
        bail!("vehicle maximum speed is outside Valhalla motorized limits");
    }
    Ok(Some(speed_kph))
}

fn weighted_distance(request: &RouteRequest) -> Result<Option<f64>> {
    if request.objective.kind != RouteObjectiveKind::Weighted {
        return Ok(None);
    }
    let weights = request
        .objective
        .weights
        .as_ref()
        .context("weighted objective requires weights")?;
    if weights.energy.get() > 0.0 || weights.risk.get() > 0.0 || weights.cost.get() > 0.0 {
        bail!("Valhalla weighted routing supports only duration and distance");
    }
    let total = weights.duration.get() + weights.distance.get();
    if total <= 0.0 {
        bail!("weighted route objective must have a positive duration or distance weight");
    }
    Ok(Some(weights.distance.get() / total))
}

fn trip_legs(
    trip: &Trip,
    map_family: MapFamily,
    source_release_ids: &BTreeSet<crate::contract::DatasetReleaseId>,
) -> Result<Vec<RouteLeg>> {
    trip.legs
        .iter()
        .enumerate()
        .map(|(sequence, leg)| {
            let decoded = polyline::decode_polyline(&leg.shape, 6)?;
            let geometry = Wgs84LineString {
                coordinates: decoded
                    .0
                    .into_iter()
                    .map(|coordinate| Wgs84Position::new(coordinate.x, coordinate.y, None))
                    .collect::<Result<Vec<_>, _>>()?,
            };
            geometry.validate()?;
            let instructions = leg
                .maneuvers
                .iter()
                .enumerate()
                .filter_map(|(maneuver_sequence, maneuver)| {
                    geometry
                        .coordinates
                        .get(maneuver.begin_shape_index)
                        .map(|position| {
                            let heading = maneuver
                                .begin_heading
                                .map(|heading| {
                                    crate::contract::Degrees::new(heading.rem_euclid(360.0))
                                })
                                .transpose();
                            heading.map(|heading| RouteInstruction {
                                sequence: maneuver_sequence as u32,
                                position: position.clone(),
                                text: maneuver.instruction.clone(),
                                heading,
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RouteLeg {
                sequence: u32::try_from(sequence).context("too many route legs")?,
                map_family,
                geometry,
                cost: RouteCost {
                    distance: Meters::new(leg.summary.length * 1_000.0)?,
                    duration: Seconds::new(leg.summary.time)?,
                    energy: None,
                    fuel: None,
                    monetary_minor_units: None,
                    risk: Ratio::new(0.0)?,
                },
                instructions,
                source_release_ids: source_release_ids.clone(),
                restriction_ids: BTreeSet::new(),
            })
        })
        .collect()
}

fn trip_cost(trip: &Trip) -> Result<RouteCost> {
    Ok(RouteCost {
        distance: Meters::new(trip.summary.length * 1_000.0)?,
        duration: Seconds::new(trip.summary.time)?,
        energy: None,
        fuel: None,
        monetary_minor_units: None,
        risk: Ratio::new(0.0)?,
    })
}

pub(in crate::routes) fn sum_cost(legs: &[RouteLeg]) -> Result<RouteCost> {
    let distance = legs.iter().map(|leg| leg.cost.distance.get()).sum();
    let duration = legs.iter().map(|leg| leg.cost.duration.get()).sum();
    let energy_values = legs
        .iter()
        .map(|leg| leg.cost.energy.map(|value| value.get()))
        .collect::<Vec<_>>();
    let fuel_values = legs
        .iter()
        .map(|leg| leg.cost.fuel.map(|value| value.get()))
        .collect::<Vec<_>>();
    let monetary = legs
        .iter()
        .map(|leg| leg.cost.monetary_minor_units)
        .collect::<Vec<_>>();
    let energy = energy_values
        .iter()
        .all(Option::is_some)
        .then(|| energy_values.into_iter().flatten().sum::<f64>())
        .map(crate::contract::KilowattHours::new)
        .transpose()?;
    let fuel = fuel_values
        .iter()
        .all(Option::is_some)
        .then(|| fuel_values.into_iter().flatten().sum::<f64>())
        .map(crate::contract::Liters::new)
        .transpose()?;
    Ok(RouteCost {
        distance: Meters::new(distance)?,
        duration: Seconds::new(duration)?,
        energy,
        fuel,
        monetary_minor_units: monetary
            .iter()
            .all(Option::is_some)
            .then(|| monetary.into_iter().flatten().sum()),
        risk: Ratio::new(
            legs.iter()
                .map(|leg| leg.cost.risk.get())
                .fold(0.0_f64, f64::max),
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::contract::{
        EnergyProfile, EnergySource, Kilograms, KilowattHours, MobilityProfileId,
        MobilityProfileMetadata, RoadVehicleProfile, RouteConstraints, RouteDataPolicy,
        VehicleDimensions, VehiclePerformance,
    };

    fn route_request(kind: RouteObjectiveKind) -> RouteRequest {
        let position = Wgs84Position::new(-89.21, 13.69, None).unwrap();
        RouteRequest {
            mobility_profile_id: MobilityProfileId::new(),
            mobility_profile_version: 1,
            origin: RouteEndpoint::Position {
                position: position.clone(),
            },
            destination: RouteEndpoint::Position { position },
            waypoints: Vec::new(),
            departure_time: Utc::now(),
            objective: RouteObjective {
                kind,
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
            alternatives: 0,
            data_policy: RouteDataPolicy {
                allow_planning_advisory: true,
                allow_stale_operational_data: false,
                required_map_families: BTreeSet::new(),
            },
        }
    }

    fn road_profile(class: RoadVehicleClass) -> MobilityProfile {
        MobilityProfile::RoadVehicle(RoadVehicleProfile {
            metadata: MobilityProfileMetadata {
                profile_id: MobilityProfileId::new(),
                name: format!("{class:?}"),
                version: 1,
                valid_from: Utc::now(),
                valid_until: None,
                labels: BTreeSet::new(),
            },
            planning: crate::contract::MobilityPlanningEnvelope {
                minimum_speed: crate::contract::MetersPerSecond::new(1.0).unwrap(),
                minimum_turn_radius: Some(Meters::new(8.0).unwrap()),
                maximum_climb_angle: Some(crate::contract::Degrees::new(15.0).unwrap()),
                maximum_descent_angle: Some(crate::contract::Degrees::new(15.0).unwrap()),
                vertical_clearance: Meters::new(0.5).unwrap(),
                lateral_clearance: Meters::new(0.5).unwrap(),
                operating_ceiling: None,
                maximum_range: Some(Meters::new(500_000.0).unwrap()),
                maximum_route_points: 1_000,
                maximum_segment_length: Meters::new(50_000.0).unwrap(),
                allowed_terrain_classes: BTreeSet::from(["paved".to_owned()]),
                allowed_restriction_kinds: BTreeSet::new(),
            },
            class,
            dimensions: VehicleDimensions {
                length: Meters::new(12.0).unwrap(),
                width: Meters::new(2.5).unwrap(),
                height: Meters::new(3.8).unwrap(),
            },
            gross_mass: Kilograms::new(18_000.0).unwrap(),
            performance: VehiclePerformance {
                maximum_speed: crate::contract::MetersPerSecond::new(30.0).unwrap(),
                nominal_speed: crate::contract::MetersPerSecond::new(12.0).unwrap(),
                maximum_range: Some(Meters::new(500_000.0).unwrap()),
                payload_capacity: Some(Kilograms::new(8_000.0).unwrap()),
            },
            energy: EnergyProfile {
                source: EnergySource::Battery,
                battery_capacity: Some(KilowattHours::new(500.0).unwrap()),
                liquid_fuel_capacity: None,
                minimum_reserve: Ratio::new(0.2).unwrap(),
            },
            axle_count: 3,
            maximum_axle_load: Kilograms::new(8_000.0).unwrap(),
            hazardous_cargo: true,
            unpaved_allowed: false,
            emissions_class: None,
        })
    }

    #[test]
    fn top_speed_does_not_silently_clamp() {
        assert!(bounded_top_speed(9.9).is_err());
        assert_eq!(bounded_top_speed(100.0).unwrap(), Some(100.0));
        assert!(bounded_top_speed(253.0).is_err());
    }

    #[test]
    fn every_road_class_selects_the_intended_valhalla_costing() {
        let cases = [
            (
                RoadVehicleClass::Bicycle,
                "bicycle",
                MapFamily::ActiveMobility,
            ),
            (
                RoadVehicleClass::PoweredTwoWheeler,
                "motorcycle",
                MapFamily::RoadStreet,
            ),
            (
                RoadVehicleClass::PassengerCar,
                "auto",
                MapFamily::RoadStreet,
            ),
            (
                RoadVehicleClass::LightCommercial,
                "auto",
                MapFamily::RoadStreet,
            ),
            (RoadVehicleClass::RigidTruck, "truck", MapFamily::RoadStreet),
            (
                RoadVehicleClass::ArticulatedTruck,
                "truck",
                MapFamily::RoadStreet,
            ),
            (RoadVehicleClass::BusCoach, "bus", MapFamily::RoadStreet),
            (
                RoadVehicleClass::EmergencyService,
                "auto",
                MapFamily::RoadStreet,
            ),
        ];

        for (class, expected_costing, expected_family) in cases {
            let (costing_name, _, family) = costing(
                &road_profile(class),
                &route_request(RouteObjectiveKind::Shortest),
            )
            .unwrap();
            assert_eq!(
                costing_name, expected_costing,
                "wrong costing for {class:?}"
            );
            assert_eq!(family, expected_family, "wrong family for {class:?}");
        }
    }

    #[test]
    fn truck_costing_carries_governed_physical_limits() {
        let (_, options, _) = costing(
            &road_profile(RoadVehicleClass::ArticulatedTruck),
            &route_request(RouteObjectiveKind::Fastest),
        )
        .unwrap();
        let truck = options.truck.expect("truck options");
        assert_eq!(truck.height, Some(3.8));
        assert_eq!(truck.width, Some(2.5));
        assert_eq!(truck.length, Some(12.0));
        assert_eq!(truck.weight, Some(18.0));
        assert_eq!(truck.axle_load, Some(8.0));
        assert_eq!(truck.axle_count, Some(3));
        assert_eq!(truck.hazmat, Some(true));
        assert_eq!(truck.exclude_unpaved, Some(true));
    }
}
