use std::collections::BTreeMap;

use crate::contract::{
    FrameBasis, FrameNode, FrameParentTransform, FrameWorldRevision, Wgs84Position, WorldFrameUri,
};
use anyhow::{Result, anyhow, bail};
use glam::{DMat3, DMat4, DQuat, DVec3};

const WGS84_A: f64 = 6_378_137.0;
const WGS84_INV_F: f64 = 298.257_223_563;
const WGS84_F: f64 = 1.0 / WGS84_INV_F;
const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);

pub fn ecef_from_frame(revision: &FrameWorldRevision, frame_uri: &WorldFrameUri) -> Result<DMat4> {
    if frame_uri.revision_uri() != *revision.revision_uri() {
        bail!(
            "frame `{frame_uri}` does not belong to revision `{}`",
            revision.revision_uri()
        );
    }
    let frames = revision
        .tree()
        .frames
        .iter()
        .map(|frame| (frame.frame_id.clone(), frame))
        .collect::<BTreeMap<_, _>>();
    let mut current = frames
        .get(&frame_uri.frame_id())
        .copied()
        .ok_or_else(|| anyhow!("unknown frame `{frame_uri}`"))?;
    let mut ecef_from_current = DMat4::IDENTITY;
    while let Some(parent_id) = &current.parent_frame_id {
        let parent_from_current = parent_from_child(current)?;
        ecef_from_current = parent_from_current * ecef_from_current;
        current = frames
            .get(parent_id)
            .copied()
            .expect("published world parent exists");
    }
    Ok(ecef_from_current)
}

fn parent_from_child(frame: &FrameNode) -> Result<DMat4> {
    match frame
        .parent_transform
        .as_ref()
        .ok_or_else(|| anyhow!("frame `{}` has no parent transform", frame.frame_id))?
    {
        FrameParentTransform::GeodeticTangent { origin } => {
            Ok(ecef_from_tangent(origin, &frame.basis))
        }
        FrameParentTransform::StaticRigid {
            translation_m,
            rotation_xyzw,
        } => Ok(DMat4::from_rotation_translation(
            DQuat::from_xyzw(
                rotation_xyzw[0],
                rotation_xyzw[1],
                rotation_xyzw[2],
                rotation_xyzw[3],
            ),
            DVec3::from_array(*translation_m),
        )),
        FrameParentTransform::DynamicStream { .. } => bail!(
            "frame `{}` uses a dynamic transform stream and requires a timestamped recording query",
            frame.frame_id
        ),
    }
}

fn ecef_from_tangent(origin: &Wgs84Position, basis: &FrameBasis) -> DMat4 {
    let latitude = origin.latitude_degrees.to_radians();
    let longitude = origin.longitude_degrees.to_radians();
    let sin_latitude = latitude.sin();
    let cos_latitude = latitude.cos();
    let sin_longitude = longitude.sin();
    let cos_longitude = longitude.cos();
    let east = DVec3::new(-sin_longitude, cos_longitude, 0.0);
    let north = DVec3::new(
        -sin_latitude * cos_longitude,
        -sin_latitude * sin_longitude,
        cos_latitude,
    );
    let up = DVec3::new(
        cos_latitude * cos_longitude,
        cos_latitude * sin_longitude,
        sin_latitude,
    );
    let rotation = match basis {
        FrameBasis::Enu => DMat3::from_cols(east, north, up),
        FrameBasis::Ned => DMat3::from_cols(north, east, -up),
        _ => unreachable!("geodetic tangent basis validated before publication"),
    };
    DMat4::from_rotation_translation(DQuat::from_mat3(&rotation), wgs84_to_ecef(origin))
}

pub fn wgs84_to_ecef(position: &Wgs84Position) -> DVec3 {
    let latitude = position.latitude_degrees.to_radians();
    let longitude = position.longitude_degrees.to_radians();
    let sin_latitude = latitude.sin();
    let cos_latitude = latitude.cos();
    let sin_longitude = longitude.sin();
    let cos_longitude = longitude.cos();
    let radius = WGS84_A / (1.0 - WGS84_E2 * sin_latitude * sin_latitude).sqrt();
    DVec3::new(
        (radius + position.ellipsoid_height_m) * cos_latitude * cos_longitude,
        (radius + position.ellipsoid_height_m) * cos_latitude * sin_longitude,
        (radius * (1.0 - WGS84_E2) + position.ellipsoid_height_m) * sin_latitude,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        FrameAxes, FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldTree, ValidatedWorldTree,
    };

    fn new_york_tree() -> FrameWorldTree {
        FrameWorldTree {
            frames: vec![
                FrameNode {
                    frame_id: FrameId::parse("follow-camera-optical").unwrap(),
                    basis: FrameBasis::OpticalRdf,
                    parent_frame_id: Some(FrameId::parse("uav-body").unwrap()),
                    parent_transform: Some(FrameParentTransform::StaticRigid {
                        translation_m: [0.0, 0.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    }),
                    description: None,
                },
                FrameNode {
                    frame_id: FrameId::parse("earth-ecef").unwrap(),
                    basis: FrameBasis::EcefWgs84,
                    parent_frame_id: None,
                    parent_transform: None,
                    description: None,
                },
                FrameNode {
                    frame_id: FrameId::parse("times-square-enu").unwrap(),
                    basis: FrameBasis::Enu,
                    parent_frame_id: Some(FrameId::parse("earth-ecef").unwrap()),
                    parent_transform: Some(FrameParentTransform::GeodeticTangent {
                        origin: Wgs84Position {
                            latitude_degrees: 40.758,
                            longitude_degrees: -73.9855,
                            ellipsoid_height_m: -17.0,
                        },
                    }),
                    description: None,
                },
                FrameNode {
                    frame_id: FrameId::parse("isaac-world").unwrap(),
                    basis: FrameBasis::Cartesian {
                        axes: FrameAxes::east_north_up(),
                    },
                    parent_frame_id: Some(FrameId::parse("times-square-enu").unwrap()),
                    parent_transform: Some(FrameParentTransform::StaticRigid {
                        translation_m: [0.0, 0.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    }),
                    description: None,
                },
                FrameNode {
                    frame_id: FrameId::parse("uav-body").unwrap(),
                    basis: FrameBasis::Frd,
                    parent_frame_id: Some(FrameId::parse("isaac-world").unwrap()),
                    parent_transform: Some(FrameParentTransform::DynamicStream {
                        stream_uri: "uav-sim://session/showcase/vehicle/uav-1/pose"
                            .parse()
                            .unwrap(),
                        entity_path: "world/uav/body".parse().unwrap(),
                    }),
                    description: None,
                },
            ],
        }
    }

    #[test]
    fn validates_and_canonicalizes_a_full_world_tree() {
        let validated = ValidatedWorldTree::new(new_york_tree()).unwrap();
        assert_eq!(validated.root_frame_id().as_str(), "earth-ecef");
        assert_eq!(validated.spec_digest().hex().len(), 64);
        assert_eq!(validated.tree().frames[0].frame_id.as_str(), "earth-ecef");
    }

    #[test]
    fn rejects_cycles_and_non_normalized_quaternions() {
        let mut cyclic = new_york_tree();
        let root = cyclic
            .frames
            .iter_mut()
            .find(|frame| frame.frame_id.as_str() == "earth-ecef")
            .unwrap();
        root.parent_frame_id = Some(FrameId::parse("isaac-world").unwrap());
        root.parent_transform = Some(FrameParentTransform::StaticRigid {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        });
        assert!(ValidatedWorldTree::new(cyclic).is_err());

        let mut invalid = new_york_tree();
        let camera = invalid
            .frames
            .iter_mut()
            .find(|frame| frame.frame_id.as_str() == "follow-camera-optical")
            .unwrap();
        camera.parent_transform = Some(FrameParentTransform::StaticRigid {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 2.0],
        });
        assert!(ValidatedWorldTree::new(invalid).is_err());
    }

    #[test]
    fn resolves_static_descendants_into_ecef() {
        let validated = ValidatedWorldTree::new(new_york_tree()).unwrap();
        let world_id = FrameWorldId::parse("uav-showcase-new-york").unwrap();
        let revision_id = FrameWorldRevisionId::parse("revision-1").unwrap();
        let revision_uri = crate::contract::FrameWorldRevisionUri::new(&world_id, &revision_id);
        let revision = FrameWorldRevision::new(
            revision_uri,
            1.try_into().unwrap(),
            validated,
            chrono::Utc::now(),
        );
        let isaac = WorldFrameUri::new(
            revision.revision_uri(),
            &FrameId::parse("isaac-world").unwrap(),
        );
        let transform = ecef_from_frame(&revision, &isaac).unwrap();
        let expected = wgs84_to_ecef(&Wgs84Position {
            latitude_degrees: 40.758,
            longitude_degrees: -73.9855,
            ellipsoid_height_m: -17.0,
        });
        assert!((transform.w_axis.truncate() - expected).length() < 1.0e-6);
    }
}
