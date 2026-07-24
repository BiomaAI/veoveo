use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};
use glam::{DMat3, DMat4, DQuat, DVec3};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldRevision, FrameWorldTree,
    Wgs84Position, WorldFrameUri,
};

const MAX_WORLD_FRAMES: usize = 10_000;
const UNIT_QUATERNION_TOLERANCE: f64 = 1.0e-9;
const WGS84_A: f64 = 6_378_137.0;
const WGS84_INV_F: f64 = 298.257_223_563;
const WGS84_F: f64 = 1.0 / WGS84_INV_F;
const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);

#[derive(Clone, Debug)]
pub struct ValidatedWorldTree {
    pub tree: FrameWorldTree,
    pub root_frame_id: FrameId,
    pub spec_sha256: String,
}

pub fn validate_world_tree(mut tree: FrameWorldTree) -> Result<ValidatedWorldTree> {
    if tree.frames.is_empty() {
        bail!("a frame world requires at least one frame");
    }
    if tree.frames.len() > MAX_WORLD_FRAMES {
        bail!("a frame world supports at most {MAX_WORLD_FRAMES} frames");
    }
    tree.frames
        .sort_by(|left, right| left.frame_id.cmp(&right.frame_id));

    let mut frames = BTreeMap::new();
    for frame in &tree.frames {
        if frames.insert(frame.frame_id.clone(), frame).is_some() {
            bail!("frame `{}` appears more than once", frame.frame_id);
        }
        frame
            .basis
            .axes()
            .validate()
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("frame `{}` has invalid axes", frame.frame_id))?;
        if let Some(description) = &frame.description
            && (description.trim().is_empty() || description.len() > 1_024)
        {
            bail!(
                "frame `{}` description must be 1 to 1024 characters",
                frame.frame_id
            );
        }
    }

    let roots = tree
        .frames
        .iter()
        .filter(|frame| frame.parent_frame_id.is_none())
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        bail!(
            "a frame world requires exactly one root frame, found {}",
            roots.len()
        );
    }
    let root = roots[0];
    if root.parent_transform.is_some() {
        bail!(
            "root frame `{}` cannot have a parent transform",
            root.frame_id
        );
    }
    if root.basis != FrameBasis::EcefWgs84 {
        bail!(
            "root frame `{}` must use the ecef_wgs84 basis",
            root.frame_id
        );
    }
    let root_frame_id = root.frame_id.clone();

    for frame in &tree.frames {
        let Some(parent_id) = &frame.parent_frame_id else {
            continue;
        };
        let parent = frames.get(parent_id).ok_or_else(|| {
            anyhow!(
                "frame `{}` has unknown parent `{parent_id}`",
                frame.frame_id
            )
        })?;
        if parent_id == &frame.frame_id {
            bail!("frame `{}` cannot parent itself", frame.frame_id);
        }
        let transform = frame.parent_transform.as_ref().ok_or_else(|| {
            anyhow!(
                "non-root frame `{}` requires a parent transform",
                frame.frame_id
            )
        })?;
        validate_parent_transform(frame, parent, transform)?;
    }

    for frame in &tree.frames {
        let mut visited = BTreeSet::new();
        let mut current = frame;
        loop {
            if !visited.insert(current.frame_id.clone()) {
                bail!(
                    "frame world contains a cycle through frame `{}`",
                    current.frame_id
                );
            }
            let Some(parent_id) = &current.parent_frame_id else {
                if current.frame_id != root.frame_id {
                    bail!(
                        "frame `{}` is disconnected from root `{}`",
                        frame.frame_id,
                        root.frame_id
                    );
                }
                break;
            };
            current = frames
                .get(parent_id)
                .expect("parent existence validated above");
        }
    }

    let encoded = serde_json::to_vec(&tree).context("encoding canonical frame world tree")?;
    let spec_sha256 = hex::encode(Sha256::digest(encoded));
    Ok(ValidatedWorldTree {
        tree,
        root_frame_id,
        spec_sha256,
    })
}

fn validate_parent_transform(
    frame: &FrameNode,
    parent: &FrameNode,
    transform: &FrameParentTransform,
) -> Result<()> {
    match transform {
        FrameParentTransform::GeodeticTangent { origin } => {
            origin.validate().map_err(anyhow::Error::msg)?;
            if parent.basis != FrameBasis::EcefWgs84 {
                bail!(
                    "geodetic tangent frame `{}` requires an ecef_wgs84 parent",
                    frame.frame_id
                );
            }
            if !matches!(frame.basis, FrameBasis::Enu | FrameBasis::Ned) {
                bail!(
                    "geodetic tangent frame `{}` must use an ENU or NED basis",
                    frame.frame_id
                );
            }
        }
        FrameParentTransform::StaticRigid {
            translation_m,
            rotation_xyzw,
        } => {
            ensure_finite(translation_m, "static translation")?;
            ensure_finite(rotation_xyzw, "static quaternion")?;
            let norm_squared = rotation_xyzw.iter().map(|value| value * value).sum::<f64>();
            if (norm_squared - 1.0).abs() > UNIT_QUATERNION_TOLERANCE {
                bail!(
                    "frame `{}` static quaternion must be normalized",
                    frame.frame_id
                );
            }
        }
        FrameParentTransform::DynamicStream {
            stream_uri,
            entity_path,
        } => {
            let Some((scheme, identity)) = stream_uri.split_once("://") else {
                bail!(
                    "frame `{}` dynamic transform requires a canonical stream URI",
                    frame.frame_id
                );
            };
            if scheme.is_empty()
                || identity.is_empty()
                || stream_uri.chars().any(char::is_whitespace)
            {
                bail!(
                    "frame `{}` dynamic transform requires a canonical stream URI",
                    frame.frame_id
                );
            }
            if entity_path.trim().is_empty() || entity_path.len() > 2_048 {
                bail!(
                    "frame `{}` dynamic transform entity_path must be 1 to 2048 characters",
                    frame.frame_id
                );
            }
        }
    }
    Ok(())
}

fn ensure_finite<const N: usize>(values: &[f64; N], name: &str) -> Result<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        bail!("{name} values must be finite")
    }
}

pub fn ecef_from_frame(revision: &FrameWorldRevision, frame_uri: &WorldFrameUri) -> Result<DMat4> {
    if frame_uri.revision_uri() != revision.revision_uri {
        bail!(
            "frame `{frame_uri}` does not belong to revision `{}`",
            revision.revision_uri
        );
    }
    let frames = revision
        .tree
        .frames
        .iter()
        .map(|frame| (frame.frame_id.clone(), frame))
        .collect::<BTreeMap<_, _>>();
    let mut current = frames
        .get(&frame_uri.frame_id())
        .copied()
        .ok_or_else(|| anyhow!("unknown frame `{frame_uri}`"))?;
    let mut ecef_from_current = DMat4::IDENTITY;
    loop {
        let Some(parent_id) = &current.parent_frame_id else {
            break;
        };
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
    use veoveo_mcp_contract::{FrameAxes, FrameWorldId, FrameWorldRevisionId};

    fn new_york_tree() -> FrameWorldTree {
        FrameWorldTree {
            frames: vec![
                FrameNode {
                    frame_id: FrameId::new("follow-camera-optical").unwrap(),
                    basis: FrameBasis::OpticalRdf,
                    parent_frame_id: Some(FrameId::new("uav-body").unwrap()),
                    parent_transform: Some(FrameParentTransform::StaticRigid {
                        translation_m: [0.0, 0.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    }),
                    description: None,
                },
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
                FrameNode {
                    frame_id: FrameId::new("isaac-world").unwrap(),
                    basis: FrameBasis::Cartesian {
                        axes: FrameAxes::east_north_up(),
                    },
                    parent_frame_id: Some(FrameId::new("times-square-enu").unwrap()),
                    parent_transform: Some(FrameParentTransform::StaticRigid {
                        translation_m: [0.0, 0.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    }),
                    description: None,
                },
                FrameNode {
                    frame_id: FrameId::new("uav-body").unwrap(),
                    basis: FrameBasis::Frd,
                    parent_frame_id: Some(FrameId::new("isaac-world").unwrap()),
                    parent_transform: Some(FrameParentTransform::DynamicStream {
                        stream_uri: "uav-sim://session/showcase/vehicle/uav-1/pose".to_owned(),
                        entity_path: "world/uav/body".to_owned(),
                    }),
                    description: None,
                },
            ],
        }
    }

    #[test]
    fn validates_and_canonicalizes_a_full_world_tree() {
        let validated = validate_world_tree(new_york_tree()).unwrap();
        assert_eq!(validated.root_frame_id.as_str(), "earth-ecef");
        assert_eq!(validated.spec_sha256.len(), 64);
        assert_eq!(validated.tree.frames[0].frame_id.as_str(), "earth-ecef");
    }

    #[test]
    fn rejects_cycles_and_non_normalized_quaternions() {
        let mut cyclic = new_york_tree();
        let root = cyclic
            .frames
            .iter_mut()
            .find(|frame| frame.frame_id.as_str() == "earth-ecef")
            .unwrap();
        root.parent_frame_id = Some(FrameId::new("isaac-world").unwrap());
        root.parent_transform = Some(FrameParentTransform::StaticRigid {
            translation_m: [0.0; 3],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        });
        assert!(validate_world_tree(cyclic).is_err());

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
        assert!(validate_world_tree(invalid).is_err());
    }

    #[test]
    fn resolves_static_descendants_into_ecef() {
        let validated = validate_world_tree(new_york_tree()).unwrap();
        let world_id = FrameWorldId::new("uav-showcase-new-york").unwrap();
        let revision_id = FrameWorldRevisionId::new("revision-1").unwrap();
        let revision_uri = veoveo_mcp_contract::FrameWorldRevisionUri::new(&world_id, &revision_id);
        let revision = FrameWorldRevision {
            world_id: world_id.clone(),
            world_uri: veoveo_mcp_contract::FrameWorldUri::new(&world_id),
            revision_id,
            revision_uri: revision_uri.clone(),
            revision: 1,
            spec_sha256: validated.spec_sha256,
            root_frame_uri: WorldFrameUri::new(&revision_uri, &validated.root_frame_id),
            tree: validated.tree,
            created_at: chrono::Utc::now(),
        };
        let isaac = WorldFrameUri::new(
            &revision.revision_uri,
            &FrameId::new("isaac-world").unwrap(),
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
