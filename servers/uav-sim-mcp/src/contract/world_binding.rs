//! Immutable simulator binding built from the Frames-owned publication contract.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_frames_mcp::contract::{FrameParentTransform, FrameWorldRevision, WorldFrameUri};

use super::{SessionId, SimulationWorldBinding};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorldBindingError {
    #[error("simulation frame must belong to its immutable world revision")]
    DifferentRevision,
    #[error("simulation frame does not identify a published frame")]
    MissingFrame,
    #[error("world revision contains a cycle")]
    Cycle,
    #[error("simulation frame has no geodetic tangent ancestor")]
    MissingGeodeticAncestor,
    #[error("simulation frame cannot descend through a dynamic transform")]
    DynamicTransform,
    #[error("non-root simulation ancestor has no parent transform")]
    MissingTransform,
    #[error("simulation frame has an unknown parent")]
    MissingParent,
    #[error("world binding spec_sha256 must be 64 lowercase hexadecimal characters")]
    InvalidDigest,
    #[error("world latitude must be finite and within [-90, 90]")]
    InvalidLatitude,
    #[error("world longitude must be finite and within [-180, 180]")]
    InvalidLongitude,
    #[error("world ellipsoid height must be finite and within [-1000, 100000] metres")]
    InvalidHeight,
}

impl super::SimulationWorldBindingValue {
    /// Select a static frame from an already validated, immutable Frames publication.
    pub fn from_revision(
        revision: &FrameWorldRevision,
        simulation_frame: &WorldFrameUri,
    ) -> Result<SimulationWorldBinding, WorldBindingError> {
        if simulation_frame.revision_uri() != *revision.revision_uri() {
            return Err(WorldBindingError::DifferentRevision);
        }
        let frames = revision
            .tree()
            .frames
            .iter()
            .map(|frame| (frame.frame_id.clone(), frame))
            .collect::<BTreeMap<_, _>>();
        let mut current = frames
            .get(&simulation_frame.frame_id())
            .copied()
            .ok_or(WorldBindingError::MissingFrame)?;
        let mut visited = BTreeSet::new();
        let georeference_origin = loop {
            if !visited.insert(current.frame_id.clone()) {
                return Err(WorldBindingError::Cycle);
            }
            let parent = current
                .parent_frame_id
                .as_ref()
                .ok_or(WorldBindingError::MissingGeodeticAncestor)?;
            match current.parent_transform.as_ref() {
                Some(FrameParentTransform::GeodeticTangent { origin }) => break origin.clone(),
                Some(FrameParentTransform::StaticRigid { .. }) => {}
                Some(FrameParentTransform::DynamicStream { .. }) => {
                    return Err(WorldBindingError::DynamicTransform);
                }
                None => return Err(WorldBindingError::MissingTransform),
            }
            current = frames
                .get(parent)
                .copied()
                .ok_or(WorldBindingError::MissingParent)?;
        };
        let binding = Self {
            revision_uri: revision.revision_uri().clone(),
            spec_sha256: veoveo_artifact_contract::UploadSha256::parse(
                revision.spec_digest().hex(),
            )
            .map_err(|_| WorldBindingError::InvalidDigest)?,
            simulation_frame_uri: simulation_frame.clone(),
            georeference_origin,
        };
        binding.validate()?;
        binding.build()
    }

    pub fn validate(&self) -> Result<(), WorldBindingError> {
        if self.simulation_frame_uri.revision_uri() != self.revision_uri {
            return Err(WorldBindingError::DifferentRevision);
        }
        let origin = &self.georeference_origin;
        if !origin.latitude_degrees.is_finite()
            || !(-90.0..=90.0).contains(&origin.latitude_degrees)
        {
            return Err(WorldBindingError::InvalidLatitude);
        }
        if !origin.longitude_degrees.is_finite()
            || !(-180.0..=180.0).contains(&origin.longitude_degrees)
        {
            return Err(WorldBindingError::InvalidLongitude);
        }
        if !origin.ellipsoid_height_m.is_finite()
            || !(-1_000.0..=100_000.0).contains(&origin.ellipsoid_height_m)
        {
            return Err(WorldBindingError::InvalidHeight);
        }
        Ok(())
    }
}

/// Reviewed startup input. Decoding and construction both validate the complete binding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    try_from = "InstallationWorldBindingWire",
    into = "InstallationWorldBindingWire"
)]
pub struct InstallationWorldBinding(veoveo_types::Checked<InstallationWorldBindingWire>);

impl InstallationWorldBinding {
    pub fn new(
        session_id: SessionId,
        world: SimulationWorldBinding,
    ) -> Result<Self, WorldBindingError> {
        veoveo_types::Checked::new(InstallationWorldBindingWire { session_id, world }).map(Self)
    }
    pub fn session_id(&self) -> &SessionId {
        &self.0.session_id
    }
    pub fn world(&self) -> &SimulationWorldBinding {
        &self.0.world
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct InstallationWorldBindingWire {
    session_id: SessionId,
    world: SimulationWorldBinding,
}

impl veoveo_types::Check for InstallationWorldBindingWire {
    type Error = WorldBindingError;
    fn check(&self) -> Result<(), Self::Error> {
        self.world.validate()?;

        Ok(())
    }
}
impl TryFrom<InstallationWorldBindingWire> for InstallationWorldBinding {
    type Error = WorldBindingError;
    fn try_from(value: InstallationWorldBindingWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<InstallationWorldBinding> for InstallationWorldBindingWire {
    fn from(value: InstallationWorldBinding) -> Self {
        value.0.into_inner()
    }
}

impl SimulationWorldBinding {
    pub fn from_revision(
        revision: &FrameWorldRevision,
        frame: &WorldFrameUri,
    ) -> Result<Self, WorldBindingError> {
        super::SimulationWorldBindingValue::from_revision(revision, frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ConfigureWorldRequest, Wgs84Position};
    use chrono::{TimeZone, Utc};
    use veoveo_frames_mcp::contract::{
        FrameAxes, FrameBasis, FrameId, FrameNode, FrameWorldId, FrameWorldRevision,
        FrameWorldRevisionId, FrameWorldRevisionUri, FrameWorldTree, ValidatedWorldTree,
        WorldFrameUri,
    };

    fn request() -> ConfigureWorldRequest {
        let world_id = FrameWorldId::parse("uav-showcase-new-york").unwrap();
        let revision_id = FrameWorldRevisionId::parse("revision-1").unwrap();
        let revision_uri = FrameWorldRevisionUri::new(&world_id, &revision_id);
        let tree = FrameWorldTree {
            frames: vec![
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
                        translation_m: [0.0; 3],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    }),
                    description: None,
                },
            ],
        };
        ConfigureWorldRequest {
            session_id: crate::contract::SessionId::parse("showcase").unwrap(),
            simulation_frame_uri: WorldFrameUri::new(
                &revision_uri,
                &FrameId::parse("isaac-world").unwrap(),
            ),
            world_revision: FrameWorldRevision::new(
                revision_uri,
                1.try_into().unwrap(),
                ValidatedWorldTree::new(tree).unwrap(),
                Utc.timestamp_opt(1_790_000_000, 0).unwrap(),
            ),
        }
    }

    #[test]
    fn binds_simulation_to_a_static_descendant_of_geodetic_anchor() {
        let request = request();
        let binding = SimulationWorldBinding::from_revision(
            &request.world_revision,
            &request.simulation_frame_uri,
        )
        .unwrap();
        assert_eq!(binding.georeference_origin.latitude_degrees, 40.758);
    }

    #[test]
    fn rejects_frames_outside_the_selected_revision_or_tree() {
        let request = request();
        let other_revision = FrameWorldRevisionUri::new(
            &FrameWorldId::parse("other-world").unwrap(),
            &FrameWorldRevisionId::parse("revision-1").unwrap(),
        );
        for (frame, expected) in [
            (
                WorldFrameUri::new(&other_revision, &FrameId::parse("isaac-world").unwrap()),
                WorldBindingError::DifferentRevision,
            ),
            (
                WorldFrameUri::new(
                    request.world_revision.revision_uri(),
                    &FrameId::parse("missing").unwrap(),
                ),
                WorldBindingError::MissingFrame,
            ),
            (
                WorldFrameUri::new(
                    request.world_revision.revision_uri(),
                    &FrameId::parse("earth-ecef").unwrap(),
                ),
                WorldBindingError::MissingGeodeticAncestor,
            ),
        ] {
            assert_eq!(
                SimulationWorldBinding::from_revision(&request.world_revision, &frame),
                Err(expected)
            );
        }
    }

    #[test]
    fn startup_document_round_trips_and_checks_construction_and_decoding() {
        let request = request();
        let world = SimulationWorldBinding::from_revision(
            &request.world_revision,
            &request.simulation_frame_uri,
        )
        .unwrap();
        let binding =
            InstallationWorldBinding::new(request.session_id.clone(), world.clone()).unwrap();
        let document = serde_json::to_value(&binding).unwrap();
        assert_eq!(
            serde_json::from_value::<InstallationWorldBinding>(document.clone()).unwrap(),
            binding
        );
        for (pointer, invalid) in [
            ("/world/spec_sha256", serde_json::json!("A".repeat(64))),
            (
                "/world/simulation_frame_uri",
                serde_json::json!("frames://world/other/revision/revision-1/frame/isaac-world"),
            ),
            (
                "/world/georeference_origin/latitude_degrees",
                serde_json::json!(91),
            ),
            (
                "/world/georeference_origin/ellipsoid_height_m",
                serde_json::json!(100_001),
            ),
        ] {
            let mut invalid_document = document.clone();
            *invalid_document.pointer_mut(pointer).unwrap() = invalid;
            assert!(
                serde_json::from_value::<InstallationWorldBinding>(invalid_document).is_err(),
                "{pointer}"
            );
        }
        let mut unknown = document;
        unknown["retry"] = serde_json::json!(true);
        assert!(serde_json::from_value::<InstallationWorldBinding>(unknown).is_err());
        let mut invalid_world = serde_json::to_value(world).unwrap();
        invalid_world["spec_sha256"] = serde_json::json!("");
        assert!(serde_json::from_value::<SimulationWorldBinding>(invalid_world).is_err());
    }
}
