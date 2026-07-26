use std::collections::BTreeSet;

use geo::{Buffer, Intersects};
use geo_types::{Coord, Geometry, LineString};

use crate::contract::{
    FeatureGeometry, GeoJsonPosition, MobilityProfile, Restriction, RestrictionEffectKind,
    RestrictionLimit, SpatialFinding, SpatialFindingCode, SpatialFindingSeverity, SpatialGeometry,
    SpatialGeometryRole, VerticalReference,
};

use super::{derive::line_length, projection::LocalProjection};

pub(super) struct SpatialValidation {
    pub findings: Vec<SpatialFinding>,
    pub intersected_restriction_ids: BTreeSet<crate::contract::RestrictionId>,
}

pub(super) fn validate(
    profile: &MobilityProfile,
    terrain_classes: &BTreeSet<String>,
    geometries: &[SpatialGeometry],
    restrictions: &[Restriction],
    projection: &LocalProjection,
) -> SpatialValidation {
    let mut findings = Vec::new();
    for terrain_class in terrain_classes.difference(&profile.planning().allowed_terrain_classes) {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::TerrainClassNotAllowed,
            None,
            format!("terrain class {terrain_class:?} is outside the mobility envelope"),
        ));
    }
    let route_lines = geometries
        .iter()
        .filter(|geometry| is_route_role(geometry.role))
        .flat_map(|geometry| geometry_lines(&geometry.geometry))
        .collect::<Vec<_>>();
    validate_route_envelope(profile, &route_lines, projection, &mut findings);
    let intersected_restriction_ids = validate_restrictions(
        profile,
        &route_lines,
        restrictions,
        projection,
        &mut findings,
    );
    SpatialValidation {
        findings,
        intersected_restriction_ids,
    }
}

fn is_route_role(role: SpatialGeometryRole) -> bool {
    matches!(
        role,
        SpatialGeometryRole::ResampledLine
            | SpatialGeometryRole::OrderedTour
            | SpatialGeometryRole::ParallelLane
            | SpatialGeometryRole::Racetrack
            | SpatialGeometryRole::CoverageTrack
            | SpatialGeometryRole::Ingress
            | SpatialGeometryRole::ValidatedRoute
    )
}

fn validate_route_envelope(
    profile: &MobilityProfile,
    lines: &[Vec<GeoJsonPosition>],
    projection: &LocalProjection,
    findings: &mut Vec<SpatialFinding>,
) {
    let envelope = profile.planning();
    let route_points = lines.iter().map(Vec::len).sum::<usize>();
    if route_points > envelope.maximum_route_points as usize {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::RoutePointLimitExceeded,
            None,
            format!(
                "derived route has {route_points} points; the mobility envelope allows {}",
                envelope.maximum_route_points
            ),
        ));
    }
    let mut total_length = 0.0;
    let mut minimum_radius = f64::INFINITY;
    let mut maximum_ascent = 0.0_f64;
    let mut maximum_descent = 0.0_f64;
    let mut maximum_height = f64::NEG_INFINITY;
    for line in lines {
        let projected = projected_line(line, projection);
        total_length += line_length(&projected);
        for segment in projected.0.windows(2) {
            let length = distance(segment[0], segment[1]);
            if length > envelope.maximum_segment_length.get() {
                findings.push(finding(
                    SpatialFindingSeverity::Violation,
                    SpatialFindingCode::SegmentLengthExceeded,
                    None,
                    format!(
                        "route segment length {length:.3} m exceeds the {:.3} m envelope",
                        envelope.maximum_segment_length.get()
                    ),
                ));
                break;
            }
        }
        for triple in projected.0.windows(3) {
            if let Some(radius) = circumradius(triple[0], triple[1], triple[2]) {
                minimum_radius = minimum_radius.min(radius);
            }
        }
        for segment in line.windows(2) {
            if let (Some(first), Some(second)) = (
                segment[0].ellipsoidal_height_m(),
                segment[1].ellipsoidal_height_m(),
            ) {
                let horizontal = distance(
                    projection.project(&wgs84(&segment[0])),
                    projection.project(&wgs84(&segment[1])),
                );
                if horizontal > 0.0 {
                    let angle = ((second - first) / horizontal).atan().to_degrees();
                    if angle >= 0.0 {
                        maximum_ascent = maximum_ascent.max(angle);
                    } else {
                        maximum_descent = maximum_descent.max(-angle);
                    }
                }
            }
        }
        for position in line {
            if let Some(height) = position.ellipsoidal_height_m() {
                maximum_height = maximum_height.max(height);
            }
        }
    }
    if profile
        .maximum_planning_range()
        .is_some_and(|maximum| total_length > maximum.get())
    {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::RangeExceeded,
            None,
            format!("route length {total_length:.3} m exceeds the mobility range"),
        ));
    }
    if envelope
        .minimum_turn_radius
        .is_some_and(|minimum| minimum_radius < minimum.get())
    {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::TurnRadiusExceeded,
            None,
            format!(
                "route curvature reaches a {minimum_radius:.3} m radius below the mobility minimum"
            ),
        ));
    }
    if envelope
        .maximum_climb_angle
        .is_some_and(|maximum| maximum_ascent > maximum.get())
    {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::ClimbLimitExceeded,
            None,
            format!("route climb angle reaches {maximum_ascent:.3} degrees"),
        ));
    }
    if envelope
        .maximum_descent_angle
        .is_some_and(|maximum| maximum_descent > maximum.get())
    {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::DescentLimitExceeded,
            None,
            format!("route descent angle reaches {maximum_descent:.3} degrees"),
        ));
    }
    if profile
        .maximum_planning_altitude()
        .is_some_and(|ceiling| maximum_height > ceiling.get())
    {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::CeilingExceeded,
            None,
            format!("route height reaches {maximum_height:.3} m above the operating ceiling"),
        ));
    }
}

fn validate_restrictions(
    profile: &MobilityProfile,
    lines: &[Vec<GeoJsonPosition>],
    restrictions: &[Restriction],
    projection: &LocalProjection,
    findings: &mut Vec<SpatialFinding>,
) -> BTreeSet<crate::contract::RestrictionId> {
    let mut intersected = BTreeSet::new();
    for restriction in restrictions {
        if restriction.cancelled_by.is_some()
            || !restriction
                .affected_mobility_families
                .contains(&profile.family())
        {
            continue;
        }
        let area = projection.project_polygon(&restriction.geometry);
        for line in lines {
            let projected = projected_line(line, projection);
            let intersects = if profile.planning().lateral_clearance.get() > 0.0 {
                projected
                    .buffer(profile.planning().lateral_clearance.get())
                    .intersects(&area)
            } else {
                Geometry::LineString(projected.clone()).intersects(&Geometry::Polygon(area.clone()))
            };
            if !intersects || !vertical_overlap(restriction, line, findings) {
                continue;
            }
            if !intersected.insert(restriction.restriction_id.clone()) {
                break;
            }
            if !profile
                .planning()
                .allowed_restriction_kinds
                .contains(&restriction.kind)
            {
                findings.push(finding(
                    SpatialFindingSeverity::Violation,
                    SpatialFindingCode::RestrictionClassNotAllowed,
                    Some(restriction),
                    format!(
                        "route intersects disallowed restriction class {:?}",
                        restriction.kind
                    ),
                ));
            }
            match restriction.effect.kind {
                RestrictionEffectKind::Prohibit => findings.push(finding(
                    SpatialFindingSeverity::Violation,
                    SpatialFindingCode::ProhibitedRestriction,
                    Some(restriction),
                    "route intersects an active prohibition".to_owned(),
                )),
                RestrictionEffectKind::Require => findings.push(finding(
                    SpatialFindingSeverity::Violation,
                    SpatialFindingCode::RequiredRestrictionCondition,
                    Some(restriction),
                    "route requires a condition that geometry alone cannot establish".to_owned(),
                )),
                RestrictionEffectKind::Limit => {
                    if restriction_limit_exceeded(profile, restriction, line) {
                        findings.push(finding(
                            SpatialFindingSeverity::Violation,
                            SpatialFindingCode::RestrictionLimitExceeded,
                            Some(restriction),
                            "mobility profile or route exceeds an active restriction limit"
                                .to_owned(),
                        ));
                    }
                }
                RestrictionEffectKind::Penalize | RestrictionEffectKind::Advise => {
                    findings.push(finding(
                        SpatialFindingSeverity::Advisory,
                        SpatialFindingCode::RestrictionAdvisory,
                        Some(restriction),
                        "route intersects an active advisory or penalty".to_owned(),
                    ));
                }
            }
            break;
        }
    }
    intersected
}

fn vertical_overlap(
    restriction: &Restriction,
    line: &[GeoJsonPosition],
    findings: &mut Vec<SpatialFinding>,
) -> bool {
    let Some(band) = &restriction.vertical_band else {
        return true;
    };
    if band.reference != VerticalReference::Ellipsoid {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::VerticalReferenceUnavailable,
            Some(restriction),
            "restriction vertical reference cannot be compared with ellipsoidal route heights"
                .to_owned(),
        ));
        return true;
    }
    let heights = line
        .iter()
        .filter_map(GeoJsonPosition::ellipsoidal_height_m)
        .collect::<Vec<_>>();
    if heights.len() != line.len() {
        findings.push(finding(
            SpatialFindingSeverity::Violation,
            SpatialFindingCode::VerticalReferenceUnavailable,
            Some(restriction),
            "route omits heights required for vertical restriction validation".to_owned(),
        ));
        return true;
    }
    let minimum = heights.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    band.lower_m.is_none_or(|lower| maximum >= lower)
        && band.upper_m.is_none_or(|upper| minimum <= upper)
}

fn restriction_limit_exceeded(
    profile: &MobilityProfile,
    restriction: &Restriction,
    line: &[GeoJsonPosition],
) -> bool {
    let Some(limit) = &restriction.effect.limit else {
        return true;
    };
    let dimensions = profile_dimensions(profile);
    match limit {
        RestrictionLimit::MaximumHeight { value } => dimensions.is_none_or(|dimensions| {
            dimensions.height.get() + profile.planning().vertical_clearance.get() > value.get()
        }),
        RestrictionLimit::MaximumWidth { value } => dimensions.is_none_or(|dimensions| {
            dimensions.width.get() + 2.0 * profile.planning().lateral_clearance.get() > value.get()
        }),
        RestrictionLimit::MaximumLength { value } => {
            dimensions.is_none_or(|dimensions| dimensions.length > *value)
        }
        RestrictionLimit::MaximumMass { value } => {
            profile_mass(profile).is_none_or(|mass| mass > value.get())
        }
        RestrictionLimit::MaximumSpeed { value } => profile.maximum_speed() > *value,
        RestrictionLimit::MinimumDepth { value } => {
            profile_required_depth(profile).is_none_or(|depth| depth > value.get())
        }
        RestrictionLimit::MinimumAltitude { value } => {
            route_height_range(line).is_none_or(|(minimum, _)| minimum < value.get())
        }
        RestrictionLimit::MaximumAltitude { value } => {
            route_height_range(line).is_none_or(|(_, maximum)| maximum > value.get())
        }
        RestrictionLimit::MinimumReserve { value } => {
            profile_reserve(profile).is_none_or(|reserve| reserve < value.get())
        }
    }
}

fn profile_dimensions(profile: &MobilityProfile) -> Option<&crate::contract::VehicleDimensions> {
    match profile {
        MobilityProfile::Human(_) => None,
        MobilityProfile::RoadVehicle(profile) => Some(&profile.dimensions),
        MobilityProfile::OffRoadVehicle(profile) => Some(&profile.dimensions),
        MobilityProfile::RailVehicle(profile) => Some(&profile.dimensions),
        MobilityProfile::SurfaceVessel(profile) => Some(&profile.dimensions),
        MobilityProfile::SubsurfaceVessel(profile) => Some(&profile.dimensions),
        MobilityProfile::FixedWing(profile) => Some(&profile.dimensions),
        MobilityProfile::Rotorcraft(profile) => Some(&profile.dimensions),
        MobilityProfile::Uas(profile) => Some(&profile.dimensions),
    }
}

fn profile_mass(profile: &MobilityProfile) -> Option<f64> {
    match profile {
        MobilityProfile::Human(_) => None,
        MobilityProfile::RoadVehicle(profile) => Some(profile.gross_mass.get()),
        MobilityProfile::OffRoadVehicle(profile) => Some(profile.gross_mass.get()),
        MobilityProfile::RailVehicle(profile) => Some(profile.gross_mass.get()),
        MobilityProfile::SurfaceVessel(profile) => Some(profile.displacement.get()),
        MobilityProfile::SubsurfaceVessel(profile) => Some(profile.displacement.get()),
        MobilityProfile::FixedWing(profile) => Some(profile.maximum_takeoff_mass.get()),
        MobilityProfile::Rotorcraft(profile) => Some(profile.maximum_takeoff_mass.get()),
        MobilityProfile::Uas(profile) => Some(profile.maximum_takeoff_mass.get()),
    }
}

fn profile_required_depth(profile: &MobilityProfile) -> Option<f64> {
    match profile {
        MobilityProfile::SurfaceVessel(profile) => {
            Some(profile.draft.get() + profile.minimum_under_keel_clearance.get())
        }
        MobilityProfile::SubsurfaceVessel(profile) => Some(
            profile.maximum_operating_depth.get() + profile.minimum_bathymetric_clearance.get(),
        ),
        _ => None,
    }
}

fn profile_reserve(profile: &MobilityProfile) -> Option<f64> {
    match profile {
        MobilityProfile::Human(_) => None,
        MobilityProfile::RoadVehicle(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::OffRoadVehicle(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::RailVehicle(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::SurfaceVessel(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::SubsurfaceVessel(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::FixedWing(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::Rotorcraft(profile) => Some(profile.energy.minimum_reserve.get()),
        MobilityProfile::Uas(profile) => Some(profile.energy.minimum_reserve.get()),
    }
}

fn route_height_range(line: &[GeoJsonPosition]) -> Option<(f64, f64)> {
    let heights = line
        .iter()
        .map(GeoJsonPosition::ellipsoidal_height_m)
        .collect::<Option<Vec<_>>>()?;
    Some((
        heights.iter().copied().fold(f64::INFINITY, f64::min),
        heights.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    ))
}

fn geometry_lines(geometry: &FeatureGeometry) -> Vec<Vec<GeoJsonPosition>> {
    match geometry {
        FeatureGeometry::LineString(line) => vec![line.clone()],
        FeatureGeometry::MultiLineString(lines) => lines.clone(),
        _ => Vec::new(),
    }
}

fn projected_line(line: &[GeoJsonPosition], projection: &LocalProjection) -> LineString<f64> {
    LineString::new(
        line.iter()
            .map(|position| projection.project(&wgs84(position)))
            .collect(),
    )
}

fn wgs84(position: &GeoJsonPosition) -> crate::contract::Wgs84Position {
    crate::contract::Wgs84Position {
        longitude_deg: position.longitude_deg(),
        latitude_deg: position.latitude_deg(),
        ellipsoidal_height_m: position.ellipsoidal_height_m(),
    }
}

fn circumradius(first: Coord<f64>, middle: Coord<f64>, last: Coord<f64>) -> Option<f64> {
    let cross = ((middle.x - first.x) * (last.y - first.y)
        - (middle.y - first.y) * (last.x - first.x))
        .abs();
    if cross < 1.0e-9 {
        return None;
    }
    Some(distance(first, middle) * distance(middle, last) * distance(last, first) / (2.0 * cross))
}

fn distance(first: Coord<f64>, second: Coord<f64>) -> f64 {
    (second.x - first.x).hypot(second.y - first.y)
}

fn finding(
    severity: SpatialFindingSeverity,
    code: SpatialFindingCode,
    restriction: Option<&Restriction>,
    message: String,
) -> SpatialFinding {
    SpatialFinding {
        severity,
        code,
        restriction_id: restriction.map(|restriction| restriction.restriction_id.clone()),
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{TimeDelta, Utc};

    use super::*;
    use crate::contract::{
        AuthorityClass, Degrees, HumanMobilityProfile, HumanMovementMode, Kilograms, Meters,
        MetersPerSecond, MobilityFamily, MobilityPlanningEnvelope, MobilityProfileId,
        MobilityProfileMetadata, Ratio, RestrictionEffect, RestrictionId, RestrictionKind, Seconds,
        SpatialDerivationOperation, Wgs84LineString, Wgs84Polygon, Wgs84Position,
    };

    fn point(longitude_deg: f64, latitude_deg: f64) -> Wgs84Position {
        Wgs84Position::new(longitude_deg, latitude_deg, None).unwrap()
    }

    fn profile() -> MobilityProfile {
        MobilityProfile::Human(HumanMobilityProfile {
            metadata: MobilityProfileMetadata {
                profile_id: MobilityProfileId::new(),
                name: "bounded walker".to_owned(),
                version: 1,
                valid_from: Utc::now() - TimeDelta::minutes(1),
                valid_until: None,
                labels: BTreeSet::new(),
            },
            planning: MobilityPlanningEnvelope {
                minimum_speed: MetersPerSecond::new(0.1).unwrap(),
                minimum_turn_radius: None,
                maximum_climb_angle: Some(Degrees::new(30.0).unwrap()),
                maximum_descent_angle: Some(Degrees::new(30.0).unwrap()),
                vertical_clearance: Meters::new(0.5).unwrap(),
                lateral_clearance: Meters::new(2.0).unwrap(),
                operating_ceiling: None,
                maximum_range: Some(Meters::new(1_000.0).unwrap()),
                maximum_route_points: 10,
                maximum_segment_length: Meters::new(25.0).unwrap(),
                allowed_terrain_classes: BTreeSet::from(["paved".to_owned()]),
                allowed_restriction_kinds: BTreeSet::from([RestrictionKind::Closure]),
            },
            mode: HumanMovementMode::Walk,
            preferred_speed: MetersPerSecond::new(1.0).unwrap(),
            maximum_speed: MetersPerSecond::new(2.0).unwrap(),
            carried_load: Kilograms::new(5.0).unwrap(),
            maximum_slope: Ratio::new(0.2).unwrap(),
            maximum_step: Meters::new(0.3).unwrap(),
            maximum_continuous_duration: Seconds::new(3_600.0).unwrap(),
            stairs_allowed: true,
            unpaved_allowed: false,
            accessibility_requirements: BTreeSet::new(),
        })
    }

    #[test]
    fn complete_route_validation_checks_segments_and_prohibitions() {
        let line = Wgs84LineString {
            coordinates: vec![point(0.0, 0.0), point(0.001, 0.0)],
        };
        let operation = SpatialDerivationOperation::ValidateRoute {
            route: line.clone(),
        };
        let projection = LocalProjection::for_operation(&operation).unwrap();
        let geometry = SpatialGeometry {
            role: SpatialGeometryRole::ValidatedRoute,
            ordinal: 0,
            geometry: FeatureGeometry::LineString(
                line.coordinates
                    .iter()
                    .map(|position| {
                        GeoJsonPosition::new(position.longitude_deg, position.latitude_deg, None)
                    })
                    .collect(),
            ),
        };
        let now = Utc::now();
        let restriction = Restriction {
            restriction_id: RestrictionId::new(),
            kind: RestrictionKind::Closure,
            geometry: Wgs84Polygon {
                exterior: vec![
                    point(0.0004, -0.0001),
                    point(0.0006, -0.0001),
                    point(0.0006, 0.0001),
                    point(0.0004, 0.0001),
                    point(0.0004, -0.0001),
                ],
                interiors: Vec::new(),
            },
            vertical_band: None,
            affected_mobility_families: BTreeSet::from([MobilityFamily::Human]),
            effect: RestrictionEffect {
                kind: RestrictionEffectKind::Prohibit,
                limit: None,
                explanation: None,
            },
            valid_from: now - TimeDelta::minutes(1),
            valid_until: None,
            authority: AuthorityClass::Regulator,
            source_release_id: None,
            issued_at: now,
            cancelled_by: None,
            record_version: 1,
        };
        let output = validate(
            &profile(),
            &BTreeSet::from(["paved".to_owned()]),
            &[geometry],
            &[restriction],
            &projection,
        );
        let codes = output
            .findings
            .iter()
            .map(|finding| finding.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains(&SpatialFindingCode::SegmentLengthExceeded));
        assert!(codes.contains(&SpatialFindingCode::ProhibitedRestriction));
        assert_eq!(output.intersected_restriction_ids.len(), 1);
    }
}
