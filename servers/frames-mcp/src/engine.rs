use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, anyhow, bail};
use chrono::Utc;
use glam::DVec3;
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateOperationKind, CoordinateOperationProvenance,
    CoordinateOperationRef, CoordinateSpace, CrsId, FrameWorldRevision, FrameWorldRevisionUri,
    Wgs84Position, WorldFrameUri,
};

use crate::{
    contract::{
        ConvertFrameOutput, ConvertFrameRequest, CoordinatePoint, EcefPosition, WorldFramePosition,
    },
    uris, world,
};

const WGS84_A: f64 = 6_378_137.0;
const WGS84_INV_F: f64 = 298.257_223_563;
const WGS84_F: f64 = 1.0 / WGS84_INV_F;
const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);
const WGS84_B: f64 = WGS84_A * (1.0 - WGS84_F);
const WGS84_EP2: f64 = (WGS84_A * WGS84_A - WGS84_B * WGS84_B) / (WGS84_B * WGS84_B);

#[derive(Debug, Clone, Default)]
pub struct ResolvedWorlds {
    revisions: HashMap<FrameWorldRevisionUri, FrameWorldRevision>,
}

impl ResolvedWorlds {
    pub fn insert(&mut self, revision: FrameWorldRevision) -> Result<()> {
        let uri = revision.revision_uri.clone();
        if self.revisions.insert(uri.clone(), revision).is_some() {
            bail!("world revision `{uri}` was resolved more than once");
        }
        Ok(())
    }

    pub fn require_for_frame(&self, frame_uri: &WorldFrameUri) -> Result<&FrameWorldRevision> {
        let revision_uri = frame_uri.revision_uri();
        self.revisions
            .get(&revision_uri)
            .ok_or_else(|| anyhow!("world revision `{revision_uri}` was not resolved"))
    }
}

pub fn convert_frame(
    request: ConvertFrameRequest,
    worlds: &ResolvedWorlds,
) -> Result<ConvertFrameOutput> {
    if request.points.is_empty() {
        bail!("convert_frame requires at least one point");
    }
    if request.allow_approximation {
        bail!("approximate frame conversion is not supported");
    }

    let output_points = request
        .points
        .iter()
        .map(|point| point_to_ecef(point, worlds))
        .map(|ecef| ecef.and_then(|ecef| ecef_to_target(ecef, &request.target, worlds)))
        .collect::<Result<Vec<_>>>()?;
    let source_spaces = request
        .points
        .iter()
        .map(coordinate_space)
        .collect::<BTreeSet<_>>();
    let source_frame = if source_spaces.len() == 1 {
        source_spaces.into_iter().next()
    } else {
        None
    };
    let target_frame = request.target.clone();
    let provenance = provenance(
        source_frame,
        Some(target_frame.clone()),
        crs_for_space(&target_frame),
    );
    Ok(ConvertFrameOutput {
        points: output_points,
        provenance,
    })
}

fn point_to_ecef(point: &CoordinatePoint, worlds: &ResolvedWorlds) -> Result<DVec3> {
    match point {
        CoordinatePoint::Wgs84(position) => {
            position.validate().map_err(anyhow::Error::msg)?;
            Ok(world::wgs84_to_ecef(position))
        }
        CoordinatePoint::EcefWgs84(position) => {
            ensure_finite(&[position.x_m, position.y_m, position.z_m])?;
            Ok(DVec3::new(position.x_m, position.y_m, position.z_m))
        }
        CoordinatePoint::WorldFrame(position) => {
            ensure_finite(&[position.x_m, position.y_m, position.z_m])?;
            let revision = worlds.require_for_frame(&position.frame_uri)?;
            let ecef_from_frame = world::ecef_from_frame(revision, &position.frame_uri)?;
            Ok(ecef_from_frame.transform_point3(DVec3::new(
                position.x_m,
                position.y_m,
                position.z_m,
            )))
        }
    }
}

fn ecef_to_target(
    ecef: DVec3,
    target: &CoordinateSpace,
    worlds: &ResolvedWorlds,
) -> Result<CoordinatePoint> {
    match target {
        CoordinateSpace::Wgs84 => Ok(CoordinatePoint::Wgs84(ecef_to_wgs84(ecef))),
        CoordinateSpace::EcefWgs84 => Ok(CoordinatePoint::EcefWgs84(EcefPosition {
            x_m: ecef.x,
            y_m: ecef.y,
            z_m: ecef.z,
        })),
        CoordinateSpace::WorldFrame { frame_uri } => {
            let revision = worlds.require_for_frame(frame_uri)?;
            let ecef_from_frame = world::ecef_from_frame(revision, frame_uri)?;
            let determinant = ecef_from_frame.determinant();
            if !determinant.is_finite() || determinant.abs() < f64::EPSILON {
                bail!("frame `{frame_uri}` has a non-invertible transform");
            }
            let point = ecef_from_frame.inverse().transform_point3(ecef);
            Ok(CoordinatePoint::WorldFrame(WorldFramePosition {
                frame_uri: frame_uri.clone(),
                x_m: point.x,
                y_m: point.y,
                z_m: point.z,
            }))
        }
    }
}

fn coordinate_space(point: &CoordinatePoint) -> CoordinateSpace {
    match point {
        CoordinatePoint::Wgs84(_) => CoordinateSpace::Wgs84,
        CoordinatePoint::EcefWgs84(_) => CoordinateSpace::EcefWgs84,
        CoordinatePoint::WorldFrame(point) => CoordinateSpace::WorldFrame {
            frame_uri: point.frame_uri.clone(),
        },
    }
}

fn crs_for_space(space: &CoordinateSpace) -> Option<CrsId> {
    match space {
        CoordinateSpace::Wgs84 => CrsId::new("EPSG:4326").ok(),
        CoordinateSpace::EcefWgs84 => CrsId::new("EPSG:4978").ok(),
        CoordinateSpace::WorldFrame { .. } => None,
    }
}

fn ecef_to_wgs84(position: DVec3) -> Wgs84Position {
    let horizontal = (position.x * position.x + position.y * position.y).sqrt();
    let theta = (position.z * WGS84_A).atan2(horizontal * WGS84_B);
    let sin_theta = theta.sin();
    let cos_theta = theta.cos();
    let latitude = (position.z + WGS84_EP2 * WGS84_B * sin_theta.powi(3))
        .atan2(horizontal - WGS84_E2 * WGS84_A * cos_theta.powi(3));
    let longitude = position.y.atan2(position.x);
    let sin_latitude = latitude.sin();
    let radius = WGS84_A / (1.0 - WGS84_E2 * sin_latitude * sin_latitude).sqrt();
    let ellipsoid_height_m = horizontal / latitude.cos() - radius;
    Wgs84Position {
        latitude_degrees: latitude.to_degrees(),
        longitude_degrees: longitude.to_degrees(),
        ellipsoid_height_m,
    }
}

fn ensure_finite(values: &[f64]) -> Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        bail!("coordinate values must be finite")
    }
}

fn provenance(
    source_frame: Option<CoordinateSpace>,
    target_frame: Option<CoordinateSpace>,
    target_crs: Option<CrsId>,
) -> CoordinateOperationProvenance {
    let operation_id = CoordinateOperationId::new(format!("op-{}", uuid::Uuid::now_v7()))
        .expect("generated operation id is valid");
    CoordinateOperationProvenance {
        operation: CoordinateOperationRef {
            operation_uri: uris::operation_uri(operation_id.as_str()),
            operation_id,
            source_frame,
            target_frame,
            created_at: Utc::now(),
        },
        kind: CoordinateOperationKind::FrameConversion,
        source_crs: None,
        target_crs,
        engine: Some("veoveo-frames:world-tree".to_owned()),
        grid_packages: Vec::new(),
        approximation_used: false,
        accuracy_m: None,
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use veoveo_mcp_contract::{
        FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldId, FrameWorldRevisionId,
        FrameWorldRevisionUri, FrameWorldTree, FrameWorldUri,
    };

    fn revision() -> FrameWorldRevision {
        let world_id = FrameWorldId::new("new-york-showcase").unwrap();
        let revision_id = FrameWorldRevisionId::new("revision-1").unwrap();
        let revision_uri = FrameWorldRevisionUri::new(&world_id, &revision_id);
        FrameWorldRevision {
            world_id: world_id.clone(),
            world_uri: FrameWorldUri::new(&world_id),
            revision_id,
            revision_uri: revision_uri.clone(),
            revision: 1,
            spec_sha256: "a".repeat(64),
            root_frame_uri: WorldFrameUri::new(&revision_uri, &FrameId::new("earth-ecef").unwrap()),
            tree: FrameWorldTree {
                frames: vec![
                    FrameNode {
                        frame_id: FrameId::new("earth-ecef").unwrap(),
                        basis: FrameBasis::EcefWgs84,
                        parent_frame_id: None,
                        parent_transform: None,
                        description: None,
                    },
                    FrameNode {
                        frame_id: FrameId::new("times-square-enu").unwrap(),
                        basis: FrameBasis::Enu,
                        parent_frame_id: Some(FrameId::new("earth-ecef").unwrap()),
                        parent_transform: Some(FrameParentTransform::GeodeticTangent {
                            origin: Wgs84Position {
                                latitude_degrees: 40.758,
                                longitude_degrees: -73.9855,
                                ellipsoid_height_m: -17.0,
                            },
                        }),
                        description: None,
                    },
                ],
            },
            created_at: Utc::now(),
        }
    }

    #[test]
    fn world_frame_round_trips_through_wgs84() {
        let revision = revision();
        let frame_uri = WorldFrameUri::new(
            &revision.revision_uri,
            &FrameId::new("times-square-enu").unwrap(),
        );
        let mut worlds = ResolvedWorlds::default();
        worlds.insert(revision).unwrap();
        let to_wgs84 = convert_frame(
            ConvertFrameRequest {
                target: CoordinateSpace::Wgs84,
                points: vec![CoordinatePoint::WorldFrame(WorldFramePosition {
                    frame_uri: frame_uri.clone(),
                    x_m: 0.0,
                    y_m: 0.0,
                    z_m: 0.0,
                })],
                allow_approximation: false,
            },
            &worlds,
        )
        .unwrap();
        let CoordinatePoint::Wgs84(origin) = &to_wgs84.points[0] else {
            panic!("expected WGS84");
        };
        assert!((origin.latitude_degrees - 40.758).abs() < 1.0e-8);
        assert!((origin.longitude_degrees + 73.9855).abs() < 1.0e-8);

        let back = convert_frame(
            ConvertFrameRequest {
                target: CoordinateSpace::WorldFrame { frame_uri },
                points: to_wgs84.points,
                allow_approximation: false,
            },
            &worlds,
        )
        .unwrap();
        let CoordinatePoint::WorldFrame(local) = &back.points[0] else {
            panic!("expected world frame");
        };
        assert!(DVec3::new(local.x_m, local.y_m, local.z_m).length() < 1.0e-6);
    }
}
