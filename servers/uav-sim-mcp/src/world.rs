use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::FrameParentTransform;

use crate::contract::{ConfigureWorldRequest, SimulationWorldBinding, Wgs84Position};

pub fn world_binding(request: &ConfigureWorldRequest) -> Result<SimulationWorldBinding> {
    let revision = &request.world_revision;
    if request.simulation_frame_uri.revision_uri() != revision.revision_uri {
        bail!("simulation_frame_uri must belong to world_revision");
    }
    let encoded = serde_json::to_vec(&revision.tree)
        .context("encoding frame world tree for integrity verification")?;
    let actual_sha256 = hex::encode(Sha256::digest(encoded));
    if actual_sha256 != revision.spec_sha256 {
        bail!("world revision tree does not match spec_sha256");
    }
    let frames = revision
        .tree
        .frames
        .iter()
        .map(|frame| (frame.frame_id.clone(), frame))
        .collect::<BTreeMap<_, _>>();
    let mut current = frames
        .get(&request.simulation_frame_uri.frame_id())
        .copied()
        .ok_or_else(|| anyhow!("simulation_frame_uri does not identify a published frame"))?;
    let mut visited = BTreeSet::new();
    let georeference_origin = loop {
        if !visited.insert(current.frame_id.clone()) {
            bail!("world revision contains a cycle");
        }
        let Some(parent_id) = &current.parent_frame_id else {
            bail!("simulation frame has no geodetic tangent ancestor");
        };
        match current.parent_transform.as_ref() {
            Some(FrameParentTransform::GeodeticTangent { origin }) => {
                origin.validate().map_err(anyhow::Error::msg)?;
                break Wgs84Position {
                    latitude_degrees: origin.latitude_degrees,
                    longitude_degrees: origin.longitude_degrees,
                    ellipsoid_height_m: origin.ellipsoid_height_m,
                };
            }
            Some(FrameParentTransform::StaticRigid { .. }) => {}
            Some(FrameParentTransform::DynamicStream { .. }) => {
                bail!("simulation frame cannot descend through a dynamic transform")
            }
            None => bail!("non-root simulation ancestor has no parent transform"),
        }
        current = frames
            .get(parent_id)
            .copied()
            .ok_or_else(|| anyhow!("simulation frame has an unknown parent"))?;
    };
    Ok(SimulationWorldBinding {
        revision_uri: revision.revision_uri.clone(),
        spec_sha256: revision.spec_sha256.clone(),
        simulation_frame_uri: request.simulation_frame_uri.clone(),
        georeference_origin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use veoveo_mcp_contract::{
        FrameAxes, FrameBasis, FrameId, FrameNode, FrameWorldId, FrameWorldRevision,
        FrameWorldRevisionId, FrameWorldRevisionUri, FrameWorldTree, FrameWorldUri, WorldFrameUri,
    };

    #[test]
    fn binds_simulation_to_a_static_descendant_of_geodetic_anchor() {
        let world_id = FrameWorldId::new("uav-showcase-new-york").unwrap();
        let revision_id = FrameWorldRevisionId::new("revision-1").unwrap();
        let revision_uri = FrameWorldRevisionUri::new(&world_id, &revision_id);
        let tree = FrameWorldTree {
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
                FrameNode {
                    frame_id: FrameId::new("isaac-world").unwrap(),
                    basis: FrameBasis::Cartesian {
                        axes: FrameAxes::east_north_up(),
                    },
                    parent_frame_id: Some(FrameId::new("times-square-enu").unwrap()),
                    parent_transform: Some(FrameParentTransform::StaticRigid {
                        translation_m: [0.0; 3],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    }),
                    description: None,
                },
            ],
        };
        let spec_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(&tree).unwrap()));
        let request = ConfigureWorldRequest {
            session_id: crate::contract::SessionId::new("showcase").unwrap(),
            simulation_frame_uri: WorldFrameUri::new(
                &revision_uri,
                &FrameId::new("isaac-world").unwrap(),
            ),
            world_revision: FrameWorldRevision {
                world_id: world_id.clone(),
                world_uri: FrameWorldUri::new(&world_id),
                revision_id,
                revision_uri,
                revision: 1,
                spec_sha256,
                root_frame_uri: WorldFrameUri::new(
                    &FrameWorldRevisionUri::new(
                        &world_id,
                        &FrameWorldRevisionId::new("revision-1").unwrap(),
                    ),
                    &FrameId::new("earth-ecef").unwrap(),
                ),
                tree,
                created_at: Utc::now(),
            },
        };
        let binding = world_binding(&request).unwrap();
        assert_eq!(binding.georeference_origin.latitude_degrees, 40.758);
    }
}
