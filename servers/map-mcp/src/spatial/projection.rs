use anyhow::{Result, bail};
use geo_types::{
    Coord, Geometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
};

use crate::contract::{
    FeatureGeometry, GeoJsonPosition, SpatialDerivationOperation, SpatialProjection,
    Wgs84LineString, Wgs84Polygon, Wgs84Position,
};

pub(super) const EARTH_RADIUS_M: f64 = 6_371_008.8;
const MAX_SPAN_DEGREES: f64 = 2.0;
const MAX_ORIGIN_LATITUDE_DEGREES: f64 = 85.0;

#[derive(Debug, Clone)]
pub(super) struct LocalProjection {
    origin: Wgs84Position,
    longitude_radians: f64,
    latitude_radians: f64,
    cosine_latitude: f64,
}

impl LocalProjection {
    pub(super) fn for_operation(operation: &SpatialDerivationOperation) -> Result<Self> {
        let positions = operation_positions(operation);
        Self::from_positions(&positions)
    }

    fn from_positions(positions: &[Wgs84Position]) -> Result<Self> {
        if positions.is_empty() {
            bail!("spatial derivation requires at least one position");
        }
        let west = positions
            .iter()
            .map(|position| position.longitude_deg)
            .fold(f64::INFINITY, f64::min);
        let east = positions
            .iter()
            .map(|position| position.longitude_deg)
            .fold(f64::NEG_INFINITY, f64::max);
        let south = positions
            .iter()
            .map(|position| position.latitude_deg)
            .fold(f64::INFINITY, f64::min);
        let north = positions
            .iter()
            .map(|position| position.latitude_deg)
            .fold(f64::NEG_INFINITY, f64::max);
        if east - west > MAX_SPAN_DEGREES || north - south > MAX_SPAN_DEGREES {
            bail!("spatial derivations are bounded to a two-degree longitude and latitude span");
        }
        let count = positions.len() as f64;
        let longitude_deg = positions
            .iter()
            .map(|position| position.longitude_deg)
            .sum::<f64>()
            / count;
        let latitude_deg = positions
            .iter()
            .map(|position| position.latitude_deg)
            .sum::<f64>()
            / count;
        if latitude_deg.abs() > MAX_ORIGIN_LATITUDE_DEGREES {
            bail!("local spatial derivations are unavailable above 85 degrees latitude");
        }
        let origin = Wgs84Position::new(longitude_deg, latitude_deg, None)?;
        let longitude_radians = longitude_deg.to_radians();
        let latitude_radians = latitude_deg.to_radians();
        Ok(Self {
            origin,
            longitude_radians,
            latitude_radians,
            cosine_latitude: latitude_radians.cos(),
        })
    }

    pub(super) fn contract(&self) -> SpatialProjection {
        SpatialProjection {
            profile: "local_equirectangular_wgs84".to_owned(),
            origin: self.origin.clone(),
            earth_radius_m: EARTH_RADIUS_M,
        }
    }

    pub(super) fn project(&self, position: &Wgs84Position) -> Coord<f64> {
        Coord {
            x: EARTH_RADIUS_M
                * (position.longitude_deg.to_radians() - self.longitude_radians)
                * self.cosine_latitude,
            y: EARTH_RADIUS_M * (position.latitude_deg.to_radians() - self.latitude_radians),
        }
    }

    pub(super) fn unproject(&self, coordinate: Coord<f64>) -> GeoJsonPosition {
        GeoJsonPosition::new(
            (self.longitude_radians + coordinate.x / (EARTH_RADIUS_M * self.cosine_latitude))
                .to_degrees(),
            (self.latitude_radians + coordinate.y / EARTH_RADIUS_M).to_degrees(),
            None,
        )
    }

    pub(super) fn project_line(&self, line: &Wgs84LineString) -> LineString<f64> {
        LineString::new(
            line.coordinates
                .iter()
                .map(|position| self.project(position))
                .collect(),
        )
    }

    pub(super) fn project_polygon(&self, polygon: &Wgs84Polygon) -> Polygon<f64> {
        Polygon::new(
            LineString::new(
                polygon
                    .exterior
                    .iter()
                    .map(|position| self.project(position))
                    .collect(),
            ),
            polygon
                .interiors
                .iter()
                .map(|ring| {
                    LineString::new(ring.iter().map(|position| self.project(position)).collect())
                })
                .collect(),
        )
    }

    pub(super) fn project_feature_geometry(&self, geometry: &FeatureGeometry) -> Geometry<f64> {
        match geometry {
            FeatureGeometry::Point(point) => {
                Geometry::Point(Point(self.project_geojson_position(point)))
            }
            FeatureGeometry::MultiPoint(points) => Geometry::MultiPoint(MultiPoint::new(
                points
                    .iter()
                    .map(|point| Point(self.project_geojson_position(point)))
                    .collect(),
            )),
            FeatureGeometry::LineString(line) => {
                Geometry::LineString(self.project_geojson_line(line))
            }
            FeatureGeometry::MultiLineString(lines) => {
                Geometry::MultiLineString(MultiLineString::new(
                    lines
                        .iter()
                        .map(|line| self.project_geojson_line(line))
                        .collect(),
                ))
            }
            FeatureGeometry::Polygon(rings) => {
                Geometry::Polygon(self.project_geojson_polygon(rings))
            }
            FeatureGeometry::MultiPolygon(polygons) => Geometry::MultiPolygon(MultiPolygon::new(
                polygons
                    .iter()
                    .map(|rings| self.project_geojson_polygon(rings))
                    .collect(),
            )),
        }
    }

    pub(super) fn unproject_line(&self, line: &LineString<f64>) -> FeatureGeometry {
        FeatureGeometry::LineString(
            line.0
                .iter()
                .map(|coordinate| self.unproject(*coordinate))
                .collect(),
        )
    }

    pub(super) fn unproject_multi_line(&self, lines: &MultiLineString<f64>) -> FeatureGeometry {
        FeatureGeometry::MultiLineString(
            lines
                .0
                .iter()
                .map(|line| {
                    line.0
                        .iter()
                        .map(|coordinate| self.unproject(*coordinate))
                        .collect()
                })
                .collect(),
        )
    }

    pub(super) fn unproject_multi_polygon(&self, polygons: &MultiPolygon<f64>) -> FeatureGeometry {
        FeatureGeometry::MultiPolygon(
            polygons
                .0
                .iter()
                .map(|polygon| {
                    std::iter::once(polygon.exterior())
                        .chain(polygon.interiors())
                        .map(|ring| {
                            ring.0
                                .iter()
                                .map(|coordinate| self.unproject(*coordinate))
                                .collect()
                        })
                        .collect()
                })
                .collect(),
        )
    }

    fn project_geojson_position(&self, position: &GeoJsonPosition) -> Coord<f64> {
        self.project(&Wgs84Position {
            longitude_deg: position.longitude_deg(),
            latitude_deg: position.latitude_deg(),
            ellipsoidal_height_m: position.ellipsoidal_height_m(),
        })
    }

    fn project_geojson_line(&self, line: &[GeoJsonPosition]) -> LineString<f64> {
        LineString::new(
            line.iter()
                .map(|position| self.project_geojson_position(position))
                .collect(),
        )
    }

    fn project_geojson_polygon(&self, rings: &[Vec<GeoJsonPosition>]) -> Polygon<f64> {
        let exterior = rings
            .first()
            .map(|ring| self.project_geojson_line(ring))
            .expect("validated polygon contains an exterior ring");
        Polygon::new(
            exterior,
            rings
                .iter()
                .skip(1)
                .map(|ring| self.project_geojson_line(ring))
                .collect(),
        )
    }
}

fn operation_positions(operation: &SpatialDerivationOperation) -> Vec<Wgs84Position> {
    match operation {
        SpatialDerivationOperation::ResampleLine { line, .. }
        | SpatialDerivationOperation::Corridor {
            centerline: line, ..
        }
        | SpatialDerivationOperation::ParallelLanes {
            centerline: line, ..
        }
        | SpatialDerivationOperation::Stations { line, .. }
        | SpatialDerivationOperation::ValidateRoute { route: line } => line.coordinates.clone(),
        SpatialDerivationOperation::OrderPoints { points, .. } => {
            points.iter().map(|point| point.position.clone()).collect()
        }
        SpatialDerivationOperation::PolygonBoundary { polygon }
        | SpatialDerivationOperation::StandoffPerimeter { polygon, .. }
        | SpatialDerivationOperation::Coverage { area: polygon, .. } => polygon
            .exterior
            .iter()
            .chain(polygon.interiors.iter().flatten())
            .cloned()
            .collect(),
        SpatialDerivationOperation::Racetrack { center, .. }
        | SpatialDerivationOperation::Ingress { target: center, .. } => vec![center.clone()],
        SpatialDerivationOperation::ConnectedComponents { geometries, .. } => geometries
            .iter()
            .flat_map(|geometry| feature_positions(&geometry.geometry))
            .collect(),
    }
}

fn feature_positions(geometry: &FeatureGeometry) -> Vec<Wgs84Position> {
    let convert = |position: &GeoJsonPosition| Wgs84Position {
        longitude_deg: position.longitude_deg(),
        latitude_deg: position.latitude_deg(),
        ellipsoidal_height_m: position.ellipsoidal_height_m(),
    };
    match geometry {
        FeatureGeometry::Point(point) => vec![convert(point)],
        FeatureGeometry::MultiPoint(points) | FeatureGeometry::LineString(points) => {
            points.iter().map(convert).collect()
        }
        FeatureGeometry::MultiLineString(lines) | FeatureGeometry::Polygon(lines) => {
            lines.iter().flatten().map(convert).collect()
        }
        FeatureGeometry::MultiPolygon(polygons) => {
            polygons.iter().flatten().flatten().map(convert).collect()
        }
    }
}
