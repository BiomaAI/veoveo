use crate::contract::{
    ConvertFrameOutput, ConvertFrameRequest, CoordinatePoint, DeriveLocalFrameOutput,
    DeriveLocalFrameRequest, EcefPosition, EnuPosition, NedPosition, Wgs84Position,
};
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use rayon::prelude::*;
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateOperationKind, CoordinateOperationProvenance,
    CoordinateOperationRef, CrsId, FrameId, FrameKind,
};
use veoveo_rrd::{RrdFrameDefinition, RrdFrameId, RrdViewCoordinates};

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
        Some("veoveo-frames".to_string()),
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
        Some("veoveo-frames:wgs84-ecef-local".to_string()),
        Vec::new(),
    );
    Ok(ConvertFrameOutput {
        points: output_points,
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
            CoordinateOperationKind::FrameConversion,
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
}
