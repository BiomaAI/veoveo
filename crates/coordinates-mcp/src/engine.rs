use crate::contract::{
    ConvertFrameOutput, ConvertFrameRequest, CoordinatePoint, DeriveLocalFrameOutput,
    DeriveLocalFrameRequest, EcefPosition, EnuPosition, GeodesicDirectOutput,
    GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest, NedPosition,
    ProjectedPosition, TransformCrsOutput, TransformCrsRequest, ValidateGeofenceOutput,
    ValidateGeofenceRequest, Wgs84Position,
};
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use geo::{Contains, Covers, Intersects, Validation};
use geo_types::{Coord, LineString, Point, Polygon};
use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use proj::Proj;
use rayon::prelude::*;
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateOperationKind, CoordinateOperationProvenance,
    CoordinateOperationRef, CrsId, FrameId, FrameKind, GeofenceRule, GeofenceViolation,
};
use veoveo_rrd::{
    RrdFrameDefinition, RrdFrameId, RrdLocalLineString2, RrdLocalPolygon2, RrdViewCoordinates,
};

use crate::uris;

const WGS84_A: f64 = 6_378_137.0;
const WGS84_INV_F: f64 = 298.257_223_563;
const WGS84_F: f64 = 1.0 / WGS84_INV_F;
const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);
const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F);
const WGS84_EP2: f64 = (WGS84_A * WGS84_A - WGS84_B * WGS84_B) / (WGS84_B * WGS84_B);

#[derive(Debug, Clone, Default)]
pub struct ResolvedFrameOrigins {
    origins: HashMap<FrameId, Wgs84Position>,
}

impl ResolvedFrameOrigins {
    pub fn insert(&mut self, frame_id: FrameId, origin: Wgs84Position) -> Result<()> {
        origin.validate()?;
        if self.origins.insert(frame_id.clone(), origin).is_some() {
            bail!("origin for frame `{frame_id}` was resolved more than once");
        }
        Ok(())
    }

    fn require(&self, frame_id: &FrameId) -> Result<&Wgs84Position> {
        self.origins
            .get(frame_id)
            .ok_or_else(|| anyhow!("local frame `{frame_id}` has no resolved WGS84 origin"))
    }
}

pub fn builtin_crs_metadata() -> serde_json::Value {
    serde_json::json!([
        {
            "crs": "EPSG:4326",
            "name": "WGS 84",
            "axis_order": "longitude, latitude when used through PROJ new_known_crs",
            "unit": "degree"
        },
        {
            "crs": "EPSG:4978",
            "name": "WGS 84 geocentric",
            "axis_order": "x, y, z",
            "unit": "meter"
        },
        {
            "crs": "EPSG:3857",
            "name": "WGS 84 / Pseudo-Mercator",
            "axis_order": "easting, northing",
            "unit": "meter"
        }
    ])
}

pub fn crs_metadata(authority: &str, code: &str) -> serde_json::Value {
    let crs = format!("{authority}:{code}");
    serde_json::json!({
        "crs": crs,
        "authority": authority,
        "code": code,
        "engine": "PROJ",
        "axis_order": "normalized by proj::Proj::new_known_crs",
        "network_grid_downloads": false
    })
}

pub fn derive_local_frame(request: DeriveLocalFrameRequest) -> Result<DeriveLocalFrameOutput> {
    request.origin.validate()?;
    let (kind, view_coordinates) = match request.kind {
        FrameKind::Enu => (FrameKind::Enu, RrdViewCoordinates::east_north_up()),
        FrameKind::Ned => (FrameKind::Ned, RrdViewCoordinates::north_east_down()),
        _ => bail!("derive_local_frame supports only ENU or NED frames"),
    };
    let frame = RrdFrameDefinition {
        frame_id: rrd_frame_id(&request.frame_id)?,
        kind,
        view_coordinates: Some(view_coordinates),
        parent: Some(RrdFrameId::new("WGS84").expect("valid builtin frame id")),
        origin: Some(request.origin.to_rrd_geo_point()),
        crs: None,
        datum: Some(veoveo_mcp_contract::DatumId::new("WGS84").expect("valid datum")),
        ellipsoid: Some(veoveo_mcp_contract::EllipsoidId::new("WGS84").expect("valid ellipsoid")),
        epoch: None,
        description: request.description,
        metadata: Default::default(),
    };
    let provenance = provenance(
        CoordinateOperationKind::LocalFrameDerivation,
        Some(FrameId::new("WGS84").expect("valid builtin frame id")),
        Some(request.frame_id),
        None,
        None,
        Some("veoveo-coordinates".to_string()),
        Vec::new(),
    );
    Ok(DeriveLocalFrameOutput { frame, provenance })
}

pub fn convert_frame(
    request: ConvertFrameRequest,
    target_frame: &RrdFrameDefinition,
    source_origins: &ResolvedFrameOrigins,
) -> Result<ConvertFrameOutput> {
    if request.points.is_empty() {
        bail!("convert_frame requires at least one point");
    }
    if request.allow_approximation {
        bail!("approximate local-frame conversion is not supported");
    }
    let target_origin = target_frame
        .origin
        .clone()
        .map(Wgs84Position::try_from)
        .transpose()?;
    let output_points: Vec<_> = request
        .points
        .par_iter()
        .map(|point| {
            let ecef = point_to_ecef(point, source_origins)?;
            ecef_to_target(&ecef, target_frame, target_origin.as_ref())
        })
        .collect::<Result<Vec<_>>>()?;
    let source_frames = request
        .points
        .iter()
        .filter_map(frame_for_position)
        .collect::<HashSet<_>>();
    let source_frame = (source_frames.len() == 1)
        .then(|| source_frames.into_iter().next())
        .flatten();
    let provenance = provenance(
        CoordinateOperationKind::FrameConversion,
        source_frame,
        Some(frame_id_from_rrd(&target_frame.frame_id)?),
        None,
        target_frame.crs.clone(),
        Some("veoveo-coordinates:wgs84-ecef-local".to_string()),
        Vec::new(),
    );
    Ok(ConvertFrameOutput {
        points: output_points,
        provenance,
    })
}

pub fn transform_crs(request: TransformCrsRequest) -> Result<TransformCrsOutput> {
    if request.points.is_empty() {
        bail!("transform_crs requires at least one point");
    }
    if request.source_crs.as_str() == "EPSG:4978" || request.target_crs.as_str() == "EPSG:4978" {
        bail!(
            "transform_crs is two-dimensional and does not support geocentric EPSG:4978; use convert_frame for WGS84/ECEF conversion"
        );
    }
    if request.points.iter().any(|point| point.z.is_some()) {
        bail!(
            "transform_crs does not support Z coordinates; refusing to copy an untransformed height"
        );
    }
    let transformer = Proj::new_known_crs(
        request.source_crs.as_str(),
        request.target_crs.as_str(),
        None,
    )
    .with_context(|| {
        format!(
            "creating PROJ transform {} -> {}",
            request.source_crs, request.target_crs
        )
    })?;
    let transform_info = transformer.proj_info();
    let is_ballpark = transform_info
        .description
        .as_deref()
        .is_some_and(|description| description.to_ascii_lowercase().contains("ballpark"));
    if is_ballpark && !request.allow_approximation {
        bail!("PROJ selected a ballpark transformation; set allow_approximation=true to accept it");
    }
    let points = request
        .points
        .iter()
        .map(|point| {
            if point.crs != request.source_crs {
                bail!(
                    "point CRS `{}` does not match request source CRS `{}`",
                    point.crs,
                    request.source_crs
                );
            }
            ensure_finite(&[point.x, point.y])?;
            let converted = transformer.convert((point.x, point.y)).with_context(|| {
                format!(
                    "converting point from {} to {}",
                    request.source_crs, request.target_crs
                )
            })?;
            Ok(ProjectedPosition {
                crs: request.target_crs.clone(),
                x: converted.0,
                y: converted.1,
                z: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut warnings = Vec::new();
    if is_ballpark {
        warnings.push("PROJ selected a ballpark transformation".to_string());
    }
    let mut provenance = provenance(
        CoordinateOperationKind::CrsTransform,
        None,
        None,
        Some(request.source_crs),
        Some(request.target_crs),
        transform_info
            .description
            .clone()
            .or_else(|| Some("PROJ".to_string())),
        warnings,
    );
    provenance.approximation_used = is_ballpark;
    provenance.accuracy_m = (transform_info.accuracy >= 0.0).then_some(transform_info.accuracy);
    Ok(TransformCrsOutput { points, provenance })
}

pub fn geodesic_inverse(request: GeodesicInverseRequest) -> Result<GeodesicInverseOutput> {
    request.start.validate()?;
    request.end.validate()?;
    let geodesic = Geodesic::wgs84();
    let (distance_m, initial_azimuth_deg, final_azimuth_deg, _arc_deg): (f64, f64, f64, f64) =
        geodesic.inverse(
            request.start.latitude_deg,
            request.start.longitude_deg,
            request.end.latitude_deg,
            request.end.longitude_deg,
        );
    let provenance = provenance(
        CoordinateOperationKind::GeodesicInverse,
        Some(FrameId::new("WGS84").expect("valid builtin frame id")),
        Some(FrameId::new("WGS84").expect("valid builtin frame id")),
        Some(CrsId::new("EPSG:4326").expect("valid CRS")),
        Some(CrsId::new("EPSG:4326").expect("valid CRS")),
        Some("geographiclib-rs:wgs84".to_string()),
        Vec::new(),
    );
    Ok(GeodesicInverseOutput {
        distance_m,
        initial_azimuth_deg,
        final_azimuth_deg,
        provenance,
    })
}

pub fn geodesic_direct(request: GeodesicDirectRequest) -> Result<GeodesicDirectOutput> {
    request.start.validate()?;
    if !request.initial_azimuth_deg.is_finite() || !request.distance_m.is_finite() {
        bail!("azimuth and distance must be finite");
    }
    let geodesic = Geodesic::wgs84();
    let (latitude_deg, longitude_deg, final_azimuth_deg): (f64, f64, f64) = geodesic.direct(
        request.start.latitude_deg,
        request.start.longitude_deg,
        request.initial_azimuth_deg,
        request.distance_m,
    );
    let provenance = provenance(
        CoordinateOperationKind::GeodesicDirect,
        Some(FrameId::new("WGS84").expect("valid builtin frame id")),
        Some(FrameId::new("WGS84").expect("valid builtin frame id")),
        Some(CrsId::new("EPSG:4326").expect("valid CRS")),
        Some(CrsId::new("EPSG:4326").expect("valid CRS")),
        Some("geographiclib-rs:wgs84".to_string()),
        Vec::new(),
    );
    Ok(GeodesicDirectOutput {
        end: Wgs84Position {
            latitude_deg,
            longitude_deg,
            height_m: request.start.height_m,
        },
        final_azimuth_deg,
        provenance,
    })
}

pub fn validate_geofence(request: ValidateGeofenceRequest) -> Result<ValidateGeofenceOutput> {
    let mut violations = Vec::new();
    if let Err(err) = validate_ring(&request.geofence.polygon.exterior) {
        violations.push(GeofenceViolation {
            index: 0,
            point: [0.0, 0.0],
            reason: format!("invalid exterior ring: {err}"),
        });
    }
    if request.path.coordinates.is_empty() {
        violations.push(GeofenceViolation {
            index: 0,
            point: [0.0, 0.0],
            reason: "path must contain at least one point".to_string(),
        });
    }
    if request.geofence.polygon.frame_id != request.path.frame_id {
        bail!(
            "geofence frame `{}` does not match path frame `{}`",
            request.geofence.polygon.frame_id,
            request.path.frame_id
        );
    }
    if violations.is_empty() {
        let polygon = polygon_from_contract(&request.geofence.polygon)?;
        if !polygon.is_valid() {
            let reasons = polygon
                .validation_errors()
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            bail!("invalid geofence polygon: {reasons}");
        }
        let path = path_from_contract(&request.path)?;
        for (index, point) in request.path.coordinates.iter().enumerate() {
            let point_geo = Point::new(point[0], point[1]);
            let intersects = polygon.intersects(&point_geo);
            let contains = polygon.contains(&point_geo);
            let ok = match request.geofence.rule {
                GeofenceRule::MustStayInside => intersects || contains,
                GeofenceRule::MustStayOutside => !intersects && !contains,
            };
            if !ok {
                violations.push(GeofenceViolation {
                    index,
                    point: *point,
                    reason: match request.geofence.rule {
                        GeofenceRule::MustStayInside => "point outside required geofence",
                        GeofenceRule::MustStayOutside => "point inside forbidden geofence",
                    }
                    .to_string(),
                });
            }
        }
        if request.geofence.rule == GeofenceRule::MustStayInside
            && violations.is_empty()
            && !polygon.covers(&path)
        {
            let segment_index = path
                .lines()
                .position(|segment| !polygon.covers(&segment))
                .unwrap_or(0);
            violations.push(GeofenceViolation {
                index: segment_index,
                point: request.path.coordinates[segment_index],
                reason: "path leaves required geofence between vertices".to_string(),
            });
        }
        if request.geofence.rule == GeofenceRule::MustStayOutside && polygon.intersects(&path) {
            violations.push(GeofenceViolation {
                index: 0,
                point: request.path.coordinates[0],
                reason: "path intersects forbidden geofence".to_string(),
            });
        }
    }
    let frame_id = frame_id_from_rrd(&request.geofence.polygon.frame_id)?;
    let provenance = provenance(
        CoordinateOperationKind::GeofenceValidation,
        Some(frame_id.clone()),
        Some(frame_id),
        None,
        None,
        Some("geo".to_string()),
        Vec::new(),
    );
    Ok(ValidateGeofenceOutput {
        valid: violations.is_empty(),
        violations,
        provenance,
    })
}

fn point_to_ecef(
    point: &CoordinatePoint,
    source_origins: &ResolvedFrameOrigins,
) -> Result<EcefPosition> {
    match point {
        CoordinatePoint::Wgs84(position) => {
            position.validate()?;
            Ok(wgs84_to_ecef(position))
        }
        CoordinatePoint::Ecef(position) => {
            ensure_finite(&[position.x_m, position.y_m, position.z_m])?;
            Ok(position.clone())
        }
        CoordinatePoint::Enu(position) => {
            let origin = source_origins.require(&position.frame_id)?;
            ensure_finite(&[position.east_m, position.north_m, position.up_m])?;
            Ok(enu_to_ecef(position, origin))
        }
        CoordinatePoint::Ned(position) => {
            let origin = source_origins.require(&position.frame_id)?;
            ensure_finite(&[position.north_m, position.east_m, position.down_m])?;
            let enu = EnuPosition {
                frame_id: position.frame_id.clone(),
                east_m: position.east_m,
                north_m: position.north_m,
                up_m: -position.down_m,
            };
            Ok(enu_to_ecef(&enu, origin))
        }
        CoordinatePoint::Projected(_) => {
            bail!("convert_frame does not accept projected points; use transform_crs")
        }
    }
}

fn ecef_to_target(
    ecef: &EcefPosition,
    target_frame: &RrdFrameDefinition,
    origin: Option<&Wgs84Position>,
) -> Result<CoordinatePoint> {
    let target_frame_id = frame_id_from_rrd(&target_frame.frame_id)?;
    Ok(match target_frame.kind {
        FrameKind::Wgs84 => CoordinatePoint::Wgs84(ecef_to_wgs84(ecef)),
        FrameKind::Ecef => CoordinatePoint::Ecef(ecef.clone()),
        FrameKind::Enu => {
            let origin = origin.ok_or_else(|| anyhow!("ENU target requires a WGS84 origin"))?;
            CoordinatePoint::Enu(ecef_to_enu(ecef, origin, target_frame_id))
        }
        FrameKind::Ned => {
            let origin = origin.ok_or_else(|| anyhow!("NED target requires a WGS84 origin"))?;
            let enu = ecef_to_enu(ecef, origin, target_frame_id.clone());
            CoordinatePoint::Ned(NedPosition {
                frame_id: target_frame_id,
                north_m: enu.north_m,
                east_m: enu.east_m,
                down_m: -enu.up_m,
            })
        }
        _ => bail!(
            "target frame kind {:?} is not supported by convert_frame",
            target_frame.kind
        ),
    })
}

fn frame_for_position(point: &CoordinatePoint) -> Option<FrameId> {
    match point {
        CoordinatePoint::Wgs84(_) => FrameId::new("WGS84").ok(),
        CoordinatePoint::Ecef(_) => FrameId::new("ECEF").ok(),
        CoordinatePoint::Enu(point) => Some(point.frame_id.clone()),
        CoordinatePoint::Ned(point) => Some(point.frame_id.clone()),
        CoordinatePoint::Projected(point) => FrameId::new(point.crs.as_str()).ok(),
    }
}

fn wgs84_to_ecef(position: &Wgs84Position) -> EcefPosition {
    let lat = position.latitude_deg.to_radians();
    let lon = position.longitude_deg.to_radians();
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let sin_lon = lon.sin();
    let cos_lon = lon.cos();
    let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
    EcefPosition {
        x_m: (n + position.height_m) * cos_lat * cos_lon,
        y_m: (n + position.height_m) * cos_lat * sin_lon,
        z_m: (n * (1.0 - WGS84_E2) + position.height_m) * sin_lat,
    }
}

fn ecef_to_wgs84(position: &EcefPosition) -> Wgs84Position {
    let p = (position.x_m * position.x_m + position.y_m * position.y_m).sqrt();
    let theta = (position.z_m * WGS84_A).atan2(p * WGS84_B);
    let sin_theta = theta.sin();
    let cos_theta = theta.cos();
    let lat = (position.z_m + WGS84_EP2 * WGS84_B * sin_theta.powi(3))
        .atan2(p - WGS84_E2 * WGS84_A * cos_theta.powi(3));
    let lon = position.y_m.atan2(position.x_m);
    let sin_lat = lat.sin();
    let n = WGS84_A / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
    let height_m = p / lat.cos() - n;
    Wgs84Position {
        latitude_deg: lat.to_degrees(),
        longitude_deg: lon.to_degrees(),
        height_m,
    }
}

fn enu_to_ecef(position: &EnuPosition, origin: &Wgs84Position) -> EcefPosition {
    let origin_ecef = wgs84_to_ecef(origin);
    let lat = origin.latitude_deg.to_radians();
    let lon = origin.longitude_deg.to_radians();
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let sin_lon = lon.sin();
    let cos_lon = lon.cos();
    let x = -sin_lon * position.east_m - sin_lat * cos_lon * position.north_m
        + cos_lat * cos_lon * position.up_m;
    let y = cos_lon * position.east_m - sin_lat * sin_lon * position.north_m
        + cos_lat * sin_lon * position.up_m;
    let z = cos_lat * position.north_m + sin_lat * position.up_m;
    EcefPosition {
        x_m: origin_ecef.x_m + x,
        y_m: origin_ecef.y_m + y,
        z_m: origin_ecef.z_m + z,
    }
}

fn ecef_to_enu(ecef: &EcefPosition, origin: &Wgs84Position, frame_id: FrameId) -> EnuPosition {
    let origin_ecef = wgs84_to_ecef(origin);
    let dx = ecef.x_m - origin_ecef.x_m;
    let dy = ecef.y_m - origin_ecef.y_m;
    let dz = ecef.z_m - origin_ecef.z_m;
    let lat = origin.latitude_deg.to_radians();
    let lon = origin.longitude_deg.to_radians();
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let sin_lon = lon.sin();
    let cos_lon = lon.cos();
    EnuPosition {
        frame_id,
        east_m: -sin_lon * dx + cos_lon * dy,
        north_m: -sin_lat * cos_lon * dx - sin_lat * sin_lon * dy + cos_lat * dz,
        up_m: cos_lat * cos_lon * dx + cos_lat * sin_lon * dy + sin_lat * dz,
    }
}

fn ensure_finite(values: &[f64]) -> Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        bail!("coordinate values must be finite")
    }
}

fn polygon_from_contract(polygon: &RrdLocalPolygon2) -> Result<Polygon<f64>> {
    validate_ring(&polygon.exterior)?;
    let exterior = line_string_from_ring(&polygon.exterior);
    let holes = polygon
        .holes
        .iter()
        .map(|ring| {
            validate_ring(ring)?;
            Ok(line_string_from_ring(ring))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Polygon::new(exterior, holes))
}

fn path_from_contract(path: &RrdLocalLineString2) -> Result<LineString<f64>> {
    if path.coordinates.is_empty() {
        bail!("path must contain at least one point");
    }
    Ok(LineString::from(
        path.coordinates
            .iter()
            .map(|point| Coord {
                x: point[0],
                y: point[1],
            })
            .collect::<Vec<_>>(),
    ))
}

fn line_string_from_ring(ring: &[[f64; 2]]) -> LineString<f64> {
    LineString::from(
        ring.iter()
            .map(|point| Coord {
                x: point[0],
                y: point[1],
            })
            .collect::<Vec<_>>(),
    )
}

fn validate_ring(ring: &[[f64; 2]]) -> Result<()> {
    if ring.len() < 4 {
        bail!("ring must contain at least four coordinates");
    }
    if ring.first() != ring.last() {
        bail!("ring must be closed");
    }
    for point in ring {
        ensure_finite(point)?;
    }
    Ok(())
}

fn rrd_frame_id(frame_id: &FrameId) -> Result<RrdFrameId> {
    RrdFrameId::new(frame_id.as_str())
        .with_context(|| format!("converting frame id `{frame_id}` to RRD frame id"))
}

fn frame_id_from_rrd(frame_id: &RrdFrameId) -> Result<FrameId> {
    FrameId::new(frame_id.as_str())
        .with_context(|| format!("converting RRD frame id `{frame_id}` to coordinate frame id"))
}

fn provenance(
    kind: CoordinateOperationKind,
    source_frame: Option<FrameId>,
    target_frame: Option<FrameId>,
    source_crs: Option<CrsId>,
    target_crs: Option<CrsId>,
    engine: Option<String>,
    warnings: Vec<String>,
) -> CoordinateOperationProvenance {
    let operation_id =
        CoordinateOperationId::new(format!("op-{}", uuid::Uuid::now_v7())).expect("valid op id");
    let created_at = Utc::now();
    let operation_uri = uris::operation_uri(operation_id.as_str());
    CoordinateOperationProvenance {
        operation: CoordinateOperationRef {
            operation_id,
            operation_uri,
            source_frame,
            target_frame,
            created_at,
        },
        kind,
        source_crs,
        target_crs,
        engine,
        grid_packages: Vec::new(),
        approximation_used: false,
        accuracy_m: None,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_ids_are_uuid_v7_derived() {
        let operation = provenance(
            CoordinateOperationKind::GeodesicInverse,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
        );
        let uuid = operation
            .operation
            .operation_id
            .as_str()
            .strip_prefix("op-")
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .expect("operation id must carry a UUID");
        assert_eq!(uuid.get_version_num(), 7);
    }

    fn wgs84(lat: f64, lon: f64, height: f64) -> Wgs84Position {
        Wgs84Position {
            latitude_deg: lat,
            longitude_deg: lon,
            height_m: height,
        }
    }

    #[test]
    fn wgs84_ecef_round_trip_is_close() {
        let start = wgs84(37.6188056, -122.3754167, 4.0);
        let ecef = wgs84_to_ecef(&start);
        let back = ecef_to_wgs84(&ecef);
        assert!((start.latitude_deg - back.latitude_deg).abs() < 1e-8);
        assert!((start.longitude_deg - back.longitude_deg).abs() < 1e-8);
        assert!((start.height_m - back.height_m).abs() < 1e-4);
    }

    #[test]
    fn enu_round_trip_is_close() {
        let origin = wgs84(37.0, -122.0, 10.0);
        let frame_id = FrameId::new("ENU:test").unwrap();
        let enu = EnuPosition {
            frame_id: frame_id.clone(),
            east_m: 12.0,
            north_m: 34.0,
            up_m: 5.0,
        };
        let ecef = enu_to_ecef(&enu, &origin);
        let back = ecef_to_enu(&ecef, &origin, frame_id);
        assert!((enu.east_m - back.east_m).abs() < 1e-6);
        assert!((enu.north_m - back.north_m).abs() < 1e-6);
        assert!((enu.up_m - back.up_m).abs() < 1e-6);
    }

    #[test]
    fn mixed_local_frames_use_their_own_origins() {
        let first_frame = FrameId::new("ENU:first").unwrap();
        let second_frame = FrameId::new("ENU:second").unwrap();
        let first_origin = wgs84(37.0, -122.0, 10.0);
        let second_origin = wgs84(48.0, 2.0, 20.0);
        let mut origins = ResolvedFrameOrigins::default();
        origins
            .insert(first_frame.clone(), first_origin.clone())
            .unwrap();
        origins
            .insert(second_frame.clone(), second_origin.clone())
            .unwrap();
        let target = RrdFrameDefinition {
            frame_id: RrdFrameId::new("WGS84").unwrap(),
            kind: FrameKind::Wgs84,
            view_coordinates: None,
            parent: None,
            origin: None,
            crs: Some(CrsId::new("EPSG:4326").unwrap()),
            datum: None,
            ellipsoid: None,
            epoch: None,
            description: None,
            metadata: Default::default(),
        };
        let output = convert_frame(
            ConvertFrameRequest {
                target_frame: FrameId::new("WGS84").unwrap(),
                points: vec![
                    CoordinatePoint::Enu(EnuPosition {
                        frame_id: first_frame,
                        east_m: 0.0,
                        north_m: 0.0,
                        up_m: 0.0,
                    }),
                    CoordinatePoint::Enu(EnuPosition {
                        frame_id: second_frame,
                        east_m: 0.0,
                        north_m: 0.0,
                        up_m: 0.0,
                    }),
                ],
                allow_approximation: false,
            },
            &target,
            &origins,
        )
        .unwrap();

        for (actual, expected) in output.points.iter().zip([first_origin, second_origin]) {
            let CoordinatePoint::Wgs84(actual) = actual else {
                panic!("expected WGS84 output");
            };
            assert!((actual.latitude_deg - expected.latitude_deg).abs() < 1e-8);
            assert!((actual.longitude_deg - expected.longitude_deg).abs() < 1e-8);
            assert!((actual.height_m - expected.height_m).abs() < 1e-4);
        }
    }

    #[test]
    fn geodesic_inverse_matches_known_range() {
        let output = geodesic_inverse(GeodesicInverseRequest {
            start: wgs84(34.095925, -118.2884237, 0.0),
            end: wgs84(59.4323439, 24.7341649, 0.0),
        })
        .unwrap();
        assert!((output.distance_m - 9_094_718.727_5).abs() < 0.01);
    }

    #[test]
    fn geofence_detects_outside_point() {
        let output = validate_geofence(ValidateGeofenceRequest {
            geofence: veoveo_rrd::RrdGeofenceGeometry {
                geofence_id: None,
                rule: GeofenceRule::MustStayInside,
                polygon: veoveo_rrd::RrdLocalPolygon2 {
                    frame_id: RrdFrameId::new("ENU:test").unwrap(),
                    exterior: vec![
                        [0.0, 0.0],
                        [10.0, 0.0],
                        [10.0, 10.0],
                        [0.0, 10.0],
                        [0.0, 0.0],
                    ],
                    holes: Vec::new(),
                },
            },
            path: RrdLocalLineString2 {
                frame_id: RrdFrameId::new("ENU:test").unwrap(),
                coordinates: vec![[5.0, 5.0], [11.0, 5.0]],
            },
        })
        .unwrap();
        assert!(!output.valid);
        assert_eq!(output.violations[0].index, 1);
    }

    #[test]
    fn geofence_detects_segment_that_leaves_concave_polygon() {
        let output = validate_geofence(ValidateGeofenceRequest {
            geofence: veoveo_rrd::RrdGeofenceGeometry {
                geofence_id: None,
                rule: GeofenceRule::MustStayInside,
                polygon: veoveo_rrd::RrdLocalPolygon2 {
                    frame_id: RrdFrameId::new("ENU:test").unwrap(),
                    exterior: vec![
                        [0.0, 0.0],
                        [4.0, 0.0],
                        [4.0, 4.0],
                        [3.0, 4.0],
                        [3.0, 1.0],
                        [1.0, 1.0],
                        [1.0, 4.0],
                        [0.0, 4.0],
                        [0.0, 0.0],
                    ],
                    holes: Vec::new(),
                },
            },
            path: RrdLocalLineString2 {
                frame_id: RrdFrameId::new("ENU:test").unwrap(),
                coordinates: vec![[0.5, 3.0], [3.5, 3.0]],
            },
        })
        .unwrap();

        assert!(!output.valid);
        assert!(
            output
                .violations
                .iter()
                .any(|violation| violation.reason.contains("between vertices"))
        );
    }

    #[test]
    fn geofence_rejects_frame_mismatch() {
        let result = validate_geofence(ValidateGeofenceRequest {
            geofence: veoveo_rrd::RrdGeofenceGeometry {
                geofence_id: None,
                rule: GeofenceRule::MustStayInside,
                polygon: veoveo_rrd::RrdLocalPolygon2 {
                    frame_id: RrdFrameId::new("ENU:first").unwrap(),
                    exterior: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
                    holes: Vec::new(),
                },
            },
            path: RrdLocalLineString2 {
                frame_id: RrdFrameId::new("ENU:second").unwrap(),
                coordinates: vec![[0.5, 0.5]],
            },
        });

        assert!(result.unwrap_err().to_string().contains("does not match"));
    }

    #[test]
    fn transform_crs_rejects_untransformed_z() {
        let result = transform_crs(TransformCrsRequest {
            source_crs: CrsId::new("EPSG:4326").unwrap(),
            target_crs: CrsId::new("EPSG:3857").unwrap(),
            points: vec![ProjectedPosition {
                crs: CrsId::new("EPSG:4326").unwrap(),
                x: -122.0,
                y: 37.0,
                z: Some(10.0),
            }],
            allow_approximation: false,
        });

        assert!(result.unwrap_err().to_string().contains("Z coordinates"));
    }
}
