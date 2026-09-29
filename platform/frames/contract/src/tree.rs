//! Complete-tree admission shared by producers and contract-only consumers.
use super::{FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldTree};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_types::Sha256Digest;

const MAX_WORLD_FRAMES: usize = 10_000;
const UNIT_QUATERNION_TOLERANCE: f64 = 1.0e-9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameWorldError(String);
impl FrameWorldError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl std::fmt::Display for FrameWorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for FrameWorldError {}

macro_rules! invalid_value {
    ($($arg:tt)*) => { FrameWorldError::new(format!($($arg)*)) };
}
macro_rules! invalid {
    ($($arg:tt)*) => { return Err(invalid_value!($($arg)*)) };
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedWorldTree {
    tree: FrameWorldTree,
    root_frame_id: FrameId,
    spec_digest: Sha256Digest,
}

impl ValidatedWorldTree {
    pub fn new(mut tree: FrameWorldTree) -> Result<Self, FrameWorldError> {
        if tree.frames.is_empty() {
            invalid!("a frame world requires at least one frame");
        }
        if tree.frames.len() > MAX_WORLD_FRAMES {
            invalid!("a frame world supports at most {MAX_WORLD_FRAMES} frames");
        }
        tree.frames
            .sort_by(|left, right| left.frame_id.cmp(&right.frame_id));

        let mut frames = BTreeMap::new();
        for frame in &tree.frames {
            if frames.insert(frame.frame_id.clone(), frame).is_some() {
                invalid!("frame `{}` appears more than once", frame.frame_id);
            }
            frame.basis.axes().validate().map_err(|message| {
                FrameWorldError::new(format!("frame `{}`: {message}", frame.frame_id))
            })?;
            if let Some(description) = &frame.description
                && (description.trim().is_empty() || description.len() > 1_024)
            {
                invalid!(
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
            invalid!(
                "a frame world requires exactly one root frame, found {}",
                roots.len()
            );
        }
        let root = roots[0];
        if root.parent_transform.is_some() {
            invalid!(
                "root frame `{}` cannot have a parent transform",
                root.frame_id
            );
        }
        if root.basis != FrameBasis::EcefWgs84 {
            invalid!(
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
                invalid_value!(
                    "frame `{}` has unknown parent `{parent_id}`",
                    frame.frame_id
                )
            })?;
            if parent_id == &frame.frame_id {
                invalid!("frame `{}` cannot parent itself", frame.frame_id);
            }
            let transform = frame.parent_transform.as_ref().ok_or_else(|| {
                invalid_value!(
                    "non-root frame `{}` requires a parent transform",
                    frame.frame_id
                )
            })?;
            validate_parent_transform(frame, parent, transform)?;
        }

        let mut rooted = BTreeSet::from([root_frame_id.clone()]);
        for frame in &tree.frames {
            let mut visited = BTreeSet::new();
            let mut current = frame;
            while !rooted.contains(&current.frame_id) {
                if !visited.insert(current.frame_id.clone()) {
                    invalid!(
                        "frame world contains a cycle through frame `{}`",
                        current.frame_id
                    );
                }
                let parent_id = current
                    .parent_frame_id
                    .as_ref()
                    .expect("only the root has no parent");
                current = frames
                    .get(parent_id)
                    .expect("parent existence validated above");
            }
            rooted.extend(visited);
        }

        let encoded = serde_json::to_vec(&tree)
            .map_err(|_| FrameWorldError::new("encoding canonical frame world tree"))?;
        let spec_digest = Sha256Digest::from_hex(hex::encode(Sha256::digest(encoded)))
            .expect("SHA-256 has the declared encoding");
        Ok(ValidatedWorldTree {
            tree,
            root_frame_id,
            spec_digest,
        })
    }

    pub fn tree(&self) -> &FrameWorldTree {
        &self.tree
    }
    pub fn root_frame_id(&self) -> &FrameId {
        &self.root_frame_id
    }
    pub fn spec_digest(&self) -> &Sha256Digest {
        &self.spec_digest
    }
    pub fn into_tree(self) -> FrameWorldTree {
        self.tree
    }
}

fn validate_parent_transform(
    frame: &FrameNode,
    parent: &FrameNode,
    transform: &FrameParentTransform,
) -> Result<(), FrameWorldError> {
    match transform {
        FrameParentTransform::GeodeticTangent { origin } => {
            origin.validate().map_err(FrameWorldError::new)?;
            if parent.basis != FrameBasis::EcefWgs84 {
                invalid!(
                    "geodetic tangent frame `{}` requires an ecef_wgs84 parent",
                    frame.frame_id
                );
            }
            if !matches!(frame.basis, FrameBasis::Enu | FrameBasis::Ned) {
                invalid!(
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
                invalid!(
                    "frame `{}` static quaternion must be normalized",
                    frame.frame_id
                );
            }
        }
        FrameParentTransform::DynamicStream { .. } => {}
    }
    Ok(())
}

fn ensure_finite<const N: usize>(values: &[f64; N], name: &str) -> Result<(), FrameWorldError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        invalid!("{name} values must be finite")
    }
}
