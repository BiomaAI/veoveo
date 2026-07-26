use std::collections::BTreeMap;

use anyhow::{Result, bail};
use geo::{Buffer, Intersects};
use geo_types::{Coord, Geometry, LineString, MultiLineString, MultiPolygon};

use crate::contract::{
    FeatureGeometry, GeoJsonPosition, SpatialDerivationOperation, SpatialGeometry,
    SpatialGeometryRole, StandoffSide, StationKind, TurnDirection,
};

use super::projection::LocalProjection;

#[derive(Debug)]
pub(super) struct DerivedGeometry {
    pub geometries: Vec<SpatialGeometry>,
    pub ordered_input_ids: Vec<String>,
    pub connected_components: Vec<Vec<String>>,
}

pub(super) fn derive(
    operation: &SpatialDerivationOperation,
    projection: &LocalProjection,
) -> Result<DerivedGeometry> {
    let mut result = DerivedGeometry {
        geometries: Vec::new(),
        ordered_input_ids: Vec::new(),
        connected_components: Vec::new(),
    };
    match operation {
        SpatialDerivationOperation::ResampleLine {
            line,
            maximum_segment_length,
        } => {
            let line = resample_line(&projection.project_line(line), maximum_segment_length.get())?;
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::ResampledLine,
                0,
                projection.unproject_line(&line),
            ));
        }
        SpatialDerivationOperation::OrderPoints {
            points,
            start_id,
            close_tour,
        } => {
            let projected = points
                .iter()
                .map(|point| (point.id.clone(), projection.project(&point.position)))
                .collect::<Vec<_>>();
            let order = nearest_neighbor_order(&projected, start_id.as_deref())?;
            let mut coordinates = order
                .iter()
                .map(|index| projected[*index].1)
                .collect::<Vec<_>>();
            result.ordered_input_ids = order
                .iter()
                .map(|index| projected[*index].0.clone())
                .collect();
            if *close_tour {
                coordinates.push(coordinates[0]);
            }
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::OrderedTour,
                0,
                projection.unproject_line(&LineString::new(coordinates)),
            ));
        }
        SpatialDerivationOperation::PolygonBoundary { polygon } => {
            let polygon = projection.project_polygon(polygon);
            let lines = std::iter::once(polygon.exterior().clone())
                .chain(polygon.interiors().iter().cloned())
                .collect();
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::Boundary,
                0,
                projection.unproject_multi_line(&MultiLineString::new(lines)),
            ));
        }
        SpatialDerivationOperation::StandoffPerimeter {
            polygon,
            distance,
            side,
        } => {
            let signed_distance = match side {
                StandoffSide::Inward => -distance.get(),
                StandoffSide::Outward => distance.get(),
            };
            let buffered = projection.project_polygon(polygon).buffer(signed_distance);
            if buffered.0.is_empty() {
                bail!("the requested standoff collapses the polygon");
            }
            let lines = buffered
                .0
                .iter()
                .flat_map(|polygon| {
                    std::iter::once(polygon.exterior().clone())
                        .chain(polygon.interiors().iter().cloned())
                })
                .collect();
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::StandoffPerimeter,
                0,
                projection.unproject_multi_line(&MultiLineString::new(lines)),
            ));
        }
        SpatialDerivationOperation::Corridor {
            centerline,
            half_width,
        } => {
            let corridor = projection.project_line(centerline).buffer(half_width.get());
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::Corridor,
                0,
                projection.unproject_multi_polygon(&corridor),
            ));
        }
        SpatialDerivationOperation::ParallelLanes {
            centerline,
            lane_count,
            lane_spacing,
            corridor_half_width,
        } => {
            let centerline = projection.project_line(centerline);
            let corridor = centerline.buffer(corridor_half_width.get());
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::Corridor,
                0,
                projection.unproject_multi_polygon(&corridor),
            ));
            for lane_index in 0..*lane_count {
                let offset = (f64::from(lane_index) - (f64::from(*lane_count) - 1.0) / 2.0)
                    * lane_spacing.get();
                let lane = offset_line(&centerline, offset)?;
                result.geometries.push(spatial_geometry(
                    SpatialGeometryRole::ParallelLane,
                    lane_index,
                    projection.unproject_line(&lane),
                ));
            }
        }
        SpatialDerivationOperation::Racetrack {
            center: _,
            heading,
            straight_length,
            turn_radius,
            direction,
            sample_spacing,
        } => {
            let mut track = racetrack(
                heading.get(),
                straight_length.get(),
                turn_radius.get(),
                sample_spacing.get(),
            );
            if *direction == TurnDirection::CounterClockwise {
                track.reverse();
            }
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::Racetrack,
                0,
                projection.unproject_line(&LineString::new(track)),
            ));
        }
        SpatialDerivationOperation::Stations {
            line,
            spacing,
            station_kind,
        } => {
            let stations = points_along_line(&projection.project_line(line), spacing.get())?;
            let geometry = FeatureGeometry::MultiPoint(
                stations
                    .iter()
                    .map(|coordinate| projection.unproject(*coordinate))
                    .collect(),
            );
            result.geometries.push(spatial_geometry(
                match station_kind {
                    StationKind::Relay => SpatialGeometryRole::RelayStations,
                    StationKind::Station => SpatialGeometryRole::Stations,
                },
                0,
                geometry,
            ));
        }
        SpatialDerivationOperation::Coverage {
            area,
            lane_spacing,
            heading,
            boundary_standoff,
        } => {
            let area = projection.project_polygon(area);
            let area = if boundary_standoff.get() == 0.0 {
                MultiPolygon::new(vec![area])
            } else {
                area.buffer(-boundary_standoff.get())
            };
            if area.0.is_empty() {
                bail!("the requested coverage standoff collapses the area");
            }
            let tracks = coverage_tracks(&area, lane_spacing.get(), heading.get())?;
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::CoverageTrack,
                0,
                projection.unproject_multi_line(&tracks),
            ));
        }
        SpatialDerivationOperation::ConnectedComponents {
            geometries,
            connection_tolerance,
        } => {
            let projected = geometries
                .iter()
                .map(|input| projection.project_feature_geometry(&input.geometry))
                .collect::<Vec<_>>();
            result.connected_components =
                connected_components(geometries, &projected, connection_tolerance.get());
            result.geometries = geometries
                .iter()
                .enumerate()
                .map(|(index, input)| {
                    spatial_geometry(
                        SpatialGeometryRole::Component,
                        index as u32,
                        input.geometry.clone(),
                    )
                })
                .collect();
        }
        SpatialDerivationOperation::Ingress {
            target: _,
            inbound_heading,
            lead_in_distance,
            final_approach_distance,
        } => {
            let radians = inbound_heading.get().to_radians();
            let along = Coord {
                x: radians.sin(),
                y: radians.cos(),
            };
            let coordinate = |distance: f64| Coord {
                x: -along.x * distance,
                y: -along.y * distance,
            };
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::Ingress,
                0,
                projection.unproject_line(&LineString::new(vec![
                    coordinate(lead_in_distance.get()),
                    coordinate(final_approach_distance.get()),
                    Coord { x: 0.0, y: 0.0 },
                ])),
            ));
        }
        SpatialDerivationOperation::ValidateRoute { route } => {
            result.geometries.push(spatial_geometry(
                SpatialGeometryRole::ValidatedRoute,
                0,
                FeatureGeometry::LineString(
                    route
                        .coordinates
                        .iter()
                        .map(|position| {
                            GeoJsonPosition::new(
                                position.longitude_deg,
                                position.latitude_deg,
                                position.ellipsoidal_height_m,
                            )
                        })
                        .collect(),
                ),
            ));
        }
    }
    Ok(result)
}

fn spatial_geometry(
    role: SpatialGeometryRole,
    ordinal: u32,
    geometry: FeatureGeometry,
) -> SpatialGeometry {
    SpatialGeometry {
        role,
        ordinal,
        geometry,
    }
}

pub(super) fn resample_line(
    line: &LineString<f64>,
    maximum_segment_length: f64,
) -> Result<LineString<f64>> {
    if line.0.len() < 2 || maximum_segment_length <= 0.0 {
        bail!("line resampling requires a line and a positive segment length");
    }
    let mut output = vec![line.0[0]];
    for segment in line.0.windows(2) {
        let distance = distance(segment[0], segment[1]);
        let segments = (distance / maximum_segment_length).ceil().max(1.0) as usize;
        for index in 1..=segments {
            let fraction = index as f64 / segments as f64;
            output.push(interpolate(segment[0], segment[1], fraction));
        }
    }
    Ok(LineString::new(output))
}

pub(super) fn line_length(line: &LineString<f64>) -> f64 {
    line.0
        .windows(2)
        .map(|segment| distance(segment[0], segment[1]))
        .sum()
}

fn points_along_line(line: &LineString<f64>, spacing: f64) -> Result<Vec<Coord<f64>>> {
    let total = line_length(line);
    if total == 0.0 {
        bail!("station geometry requires a non-zero-length line");
    }
    let mut targets = Vec::new();
    let mut along_distance = 0.0;
    while along_distance < total {
        targets.push(along_distance);
        along_distance += spacing;
    }
    targets.push(total);
    let mut output = Vec::with_capacity(targets.len());
    let mut segment_start_distance = 0.0;
    let mut segment_index = 0;
    for target in targets {
        while segment_index + 1 < line.0.len() {
            let segment_length = distance(line.0[segment_index], line.0[segment_index + 1]);
            if target <= segment_start_distance + segment_length
                || segment_index + 2 == line.0.len()
            {
                let fraction = if segment_length == 0.0 {
                    0.0
                } else {
                    (target - segment_start_distance) / segment_length
                };
                output.push(interpolate(
                    line.0[segment_index],
                    line.0[segment_index + 1],
                    fraction.clamp(0.0, 1.0),
                ));
                break;
            }
            segment_start_distance += segment_length;
            segment_index += 1;
        }
    }
    Ok(output)
}

fn nearest_neighbor_order(
    points: &[(String, Coord<f64>)],
    start_id: Option<&str>,
) -> Result<Vec<usize>> {
    let first = if let Some(start_id) = start_id {
        points
            .iter()
            .position(|(id, _)| id == start_id)
            .ok_or_else(|| anyhow::anyhow!("start point is absent"))?
    } else {
        points
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| left.0.cmp(&right.0))
            .map(|(index, _)| index)
            .expect("point cardinality validated")
    };
    let mut used = vec![false; points.len()];
    let mut order = Vec::with_capacity(points.len());
    let mut current = first;
    used[current] = true;
    order.push(current);
    while order.len() < points.len() {
        let next = points
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .min_by(|(_, left), (_, right)| {
                squared_distance(points[current].1, left.1)
                    .total_cmp(&squared_distance(points[current].1, right.1))
                    .then_with(|| left.0.cmp(&right.0))
            })
            .map(|(index, _)| index)
            .expect("at least one unused point");
        used[next] = true;
        order.push(next);
        current = next;
    }
    Ok(order)
}

fn offset_line(line: &LineString<f64>, offset: f64) -> Result<LineString<f64>> {
    let segment_normals = line
        .0
        .windows(2)
        .map(|segment| {
            let dx = segment[1].x - segment[0].x;
            let dy = segment[1].y - segment[0].y;
            let length = dx.hypot(dy);
            if length == 0.0 {
                None
            } else {
                Some(Coord {
                    x: -dy / length,
                    y: dx / length,
                })
            }
        })
        .collect::<Vec<_>>();
    if segment_normals.iter().all(Option::is_none) {
        bail!("parallel-lane centerline has zero length");
    }
    let mut output = Vec::with_capacity(line.0.len());
    for index in 0..line.0.len() {
        let before = index
            .checked_sub(1)
            .and_then(|index| segment_normals[index]);
        let after = segment_normals.get(index).copied().flatten();
        let normal = match (before, after) {
            (Some(before), Some(after)) => {
                let x = before.x + after.x;
                let y = before.y + after.y;
                let length = x.hypot(y);
                if length < 1.0e-9 {
                    after
                } else {
                    Coord {
                        x: x / length,
                        y: y / length,
                    }
                }
            }
            (Some(normal), None) | (None, Some(normal)) => normal,
            (None, None) => Coord { x: 0.0, y: 0.0 },
        };
        output.push(Coord {
            x: line.0[index].x + normal.x * offset,
            y: line.0[index].y + normal.y * offset,
        });
    }
    Ok(LineString::new(output))
}

fn racetrack(
    heading_degrees: f64,
    straight_length: f64,
    radius: f64,
    spacing: f64,
) -> Vec<Coord<f64>> {
    let radians = heading_degrees.to_radians();
    let along = Coord {
        x: radians.sin(),
        y: radians.cos(),
    };
    let right = Coord {
        x: radians.cos(),
        y: -radians.sin(),
    };
    let coordinate = |along_distance: f64, right_distance: f64| Coord {
        x: along.x * along_distance + right.x * right_distance,
        y: along.y * along_distance + right.y * right_distance,
    };
    let half = straight_length / 2.0;
    let straight_steps = (straight_length / spacing).ceil().max(1.0) as usize;
    let arc_steps = (std::f64::consts::PI * radius / spacing).ceil().max(2.0) as usize;
    let mut points = Vec::with_capacity(2 * straight_steps + 2 * arc_steps + 1);
    for index in 0..=straight_steps {
        let fraction = index as f64 / straight_steps as f64;
        points.push(coordinate(-half + straight_length * fraction, radius));
    }
    for index in 1..=arc_steps {
        let angle = std::f64::consts::PI * index as f64 / arc_steps as f64;
        points.push(coordinate(
            half + radius * angle.sin(),
            radius * angle.cos(),
        ));
    }
    for index in 1..=straight_steps {
        let fraction = index as f64 / straight_steps as f64;
        points.push(coordinate(half - straight_length * fraction, -radius));
    }
    for index in 1..=arc_steps {
        let angle = std::f64::consts::PI + std::f64::consts::PI * index as f64 / arc_steps as f64;
        points.push(coordinate(
            -half + radius * angle.sin(),
            radius * angle.cos(),
        ));
    }
    points
}

fn coverage_tracks(
    polygons: &MultiPolygon<f64>,
    spacing: f64,
    heading_degrees: f64,
) -> Result<MultiLineString<f64>> {
    let radians = heading_degrees.to_radians();
    let along = Coord {
        x: radians.sin(),
        y: radians.cos(),
    };
    let cross = Coord {
        x: -along.y,
        y: along.x,
    };
    let rotate = |coordinate: Coord<f64>| Coord {
        x: coordinate.x * along.x + coordinate.y * along.y,
        y: coordinate.x * cross.x + coordinate.y * cross.y,
    };
    let inverse = |coordinate: Coord<f64>| Coord {
        x: coordinate.x * along.x + coordinate.y * cross.x,
        y: coordinate.x * along.y + coordinate.y * cross.y,
    };
    let mut lines = Vec::new();
    let mut reverse = false;
    for polygon in &polygons.0 {
        let rings = std::iter::once(polygon.exterior())
            .chain(polygon.interiors())
            .map(|ring| ring.0.iter().copied().map(rotate).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let minimum_cross = rings
            .iter()
            .flatten()
            .map(|coordinate| coordinate.y)
            .fold(f64::INFINITY, f64::min);
        let maximum_cross = rings
            .iter()
            .flatten()
            .map(|coordinate| coordinate.y)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut scan = minimum_cross + spacing / 2.0;
        if scan > maximum_cross {
            scan = (minimum_cross + maximum_cross) / 2.0;
        }
        while scan <= maximum_cross {
            let mut intersections = rings
                .iter()
                .flat_map(|ring| scanline_intersections(ring, scan))
                .collect::<Vec<_>>();
            intersections.sort_by(f64::total_cmp);
            for pair in intersections.chunks_exact(2) {
                if pair[1] - pair[0] <= 1.0e-9 {
                    continue;
                }
                let mut coordinates = vec![
                    inverse(Coord {
                        x: pair[0],
                        y: scan,
                    }),
                    inverse(Coord {
                        x: pair[1],
                        y: scan,
                    }),
                ];
                if reverse {
                    coordinates.reverse();
                }
                reverse = !reverse;
                lines.push(LineString::new(coordinates));
            }
            scan += spacing;
        }
    }
    if lines.is_empty() {
        bail!("coverage parameters produce no tracks");
    }
    Ok(MultiLineString::new(lines))
}

fn scanline_intersections(ring: &[Coord<f64>], scan: f64) -> Vec<f64> {
    ring.windows(2)
        .filter_map(|segment| {
            let first = segment[0];
            let second = segment[1];
            let crosses =
                (first.y <= scan && scan < second.y) || (second.y <= scan && scan < first.y);
            crosses
                .then(|| first.x + (scan - first.y) * (second.x - first.x) / (second.y - first.y))
        })
        .collect()
}

fn connected_components(
    inputs: &[crate::contract::SpatialGeometryInput],
    geometries: &[Geometry<f64>],
    tolerance: f64,
) -> Vec<Vec<String>> {
    let comparison = if tolerance > 0.0 {
        geometries
            .iter()
            .map(|geometry| Geometry::MultiPolygon(geometry.buffer(tolerance / 2.0)))
            .collect::<Vec<_>>()
    } else {
        geometries.to_vec()
    };
    let mut parents = (0..geometries.len()).collect::<Vec<_>>();
    for left in 0..comparison.len() {
        for right in left + 1..comparison.len() {
            if comparison[left].intersects(&comparison[right]) {
                union(&mut parents, left, right);
            }
        }
    }
    let mut groups = BTreeMap::<usize, Vec<String>>::new();
    for (index, input) in inputs.iter().enumerate() {
        let root = find(&mut parents, index);
        groups.entry(root).or_default().push(input.id.clone());
    }
    groups.into_values().collect()
}

fn find(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = find(parents, parents[index]);
    }
    parents[index]
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left_root = find(parents, left);
    let right_root = find(parents, right);
    if left_root != right_root {
        let (first, second) = if left_root < right_root {
            (left_root, right_root)
        } else {
            (right_root, left_root)
        };
        parents[second] = first;
    }
}

fn interpolate(first: Coord<f64>, second: Coord<f64>, fraction: f64) -> Coord<f64> {
    Coord {
        x: first.x + (second.x - first.x) * fraction,
        y: first.y + (second.y - first.y) * fraction,
    }
}

fn distance(first: Coord<f64>, second: Coord<f64>) -> f64 {
    squared_distance(first, second).sqrt()
}

fn squared_distance(first: Coord<f64>, second: Coord<f64>) -> f64 {
    (second.x - first.x).powi(2) + (second.y - first.y).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        Degrees, Meters, SpatialGeometryInput, SpatialPointInput, Wgs84LineString, Wgs84Polygon,
        Wgs84Position,
    };

    #[test]
    fn resampling_bounds_every_segment() {
        let line = LineString::new(vec![Coord { x: 0.0, y: 0.0 }, Coord { x: 25.0, y: 0.0 }]);
        let sampled = resample_line(&line, 10.0).unwrap();
        assert_eq!(sampled.0.len(), 4);
        assert!(
            sampled
                .0
                .windows(2)
                .all(|pair| distance(pair[0], pair[1]) <= 10.0)
        );
    }

    #[test]
    fn scanline_uses_even_odd_polygon_fill() {
        let ring = vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 10.0, y: 0.0 },
            Coord { x: 10.0, y: 10.0 },
            Coord { x: 0.0, y: 10.0 },
            Coord { x: 0.0, y: 0.0 },
        ];
        assert_eq!(scanline_intersections(&ring, 5.0), vec![10.0, 0.0]);
    }

    #[test]
    fn every_spatial_operation_produces_valid_bounded_geometry() {
        let point = |longitude_deg, latitude_deg| {
            Wgs84Position::new(longitude_deg, latitude_deg, None).unwrap()
        };
        let line = Wgs84LineString {
            coordinates: vec![point(-89.2, 13.7), point(-89.199, 13.701)],
        };
        let polygon = Wgs84Polygon {
            exterior: vec![
                point(-89.2, 13.7),
                point(-89.199, 13.7),
                point(-89.199, 13.701),
                point(-89.2, 13.701),
                point(-89.2, 13.7),
            ],
            interiors: Vec::new(),
        };
        let operations = vec![
            SpatialDerivationOperation::ResampleLine {
                line: line.clone(),
                maximum_segment_length: Meters::new(25.0).unwrap(),
            },
            SpatialDerivationOperation::OrderPoints {
                points: vec![
                    SpatialPointInput {
                        id: "a".to_owned(),
                        position: point(-89.2, 13.7),
                    },
                    SpatialPointInput {
                        id: "b".to_owned(),
                        position: point(-89.199, 13.701),
                    },
                ],
                start_id: None,
                close_tour: true,
            },
            SpatialDerivationOperation::PolygonBoundary {
                polygon: polygon.clone(),
            },
            SpatialDerivationOperation::StandoffPerimeter {
                polygon: polygon.clone(),
                distance: Meters::new(5.0).unwrap(),
                side: StandoffSide::Inward,
            },
            SpatialDerivationOperation::Corridor {
                centerline: line.clone(),
                half_width: Meters::new(10.0).unwrap(),
            },
            SpatialDerivationOperation::ParallelLanes {
                centerline: line.clone(),
                lane_count: 3,
                lane_spacing: Meters::new(5.0).unwrap(),
                corridor_half_width: Meters::new(10.0).unwrap(),
            },
            SpatialDerivationOperation::Racetrack {
                center: point(-89.2, 13.7),
                heading: Degrees::new(45.0).unwrap(),
                straight_length: Meters::new(100.0).unwrap(),
                turn_radius: Meters::new(20.0).unwrap(),
                direction: TurnDirection::Clockwise,
                sample_spacing: Meters::new(10.0).unwrap(),
            },
            SpatialDerivationOperation::Stations {
                line: line.clone(),
                spacing: Meters::new(25.0).unwrap(),
                station_kind: StationKind::Relay,
            },
            SpatialDerivationOperation::Coverage {
                area: polygon.clone(),
                lane_spacing: Meters::new(20.0).unwrap(),
                heading: Degrees::new(0.0).unwrap(),
                boundary_standoff: Meters::new(2.0).unwrap(),
            },
            SpatialDerivationOperation::ConnectedComponents {
                geometries: vec![
                    SpatialGeometryInput {
                        id: "a".to_owned(),
                        geometry: FeatureGeometry::Point(GeoJsonPosition::new(-89.2, 13.7, None)),
                    },
                    SpatialGeometryInput {
                        id: "b".to_owned(),
                        geometry: FeatureGeometry::Point(GeoJsonPosition::new(
                            -89.199, 13.701, None,
                        )),
                    },
                ],
                connection_tolerance: Meters::new(10.0).unwrap(),
            },
            SpatialDerivationOperation::Ingress {
                target: point(-89.2, 13.7),
                inbound_heading: Degrees::new(90.0).unwrap(),
                lead_in_distance: Meters::new(100.0).unwrap(),
                final_approach_distance: Meters::new(25.0).unwrap(),
            },
            SpatialDerivationOperation::ValidateRoute {
                route: line.clone(),
            },
        ];
        for operation in operations {
            operation.validate().unwrap();
            let projection = LocalProjection::for_operation(&operation).unwrap();
            let output = derive(&operation, &projection).unwrap();
            assert!(!output.geometries.is_empty(), "{operation:?}");
            for geometry in output.geometries {
                geometry.geometry.validate().unwrap();
            }
        }
    }
}
