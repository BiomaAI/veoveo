//! Checked metadata keeps resource identity and complete-tree content together.
use std::num::NonZeroU64;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::Sha256Digest;

use super::{
    FrameNode, FrameWorldError, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri,
    FrameWorldTree, FrameWorldUri, ValidatedWorldTree, WorldFrameUri,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SummaryWire", into = "SummaryWire")]
pub struct FrameWorldSummary {
    world_uri: FrameWorldUri,
    head: Option<(FrameWorldRevisionId, NonZeroU64)>,
    pub display_name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SummaryWire {
    world_id: FrameWorldId,
    world_uri: FrameWorldUri,
    display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head_revision_id: Option<FrameWorldRevisionId>,
    revision: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl FrameWorldSummary {
    pub fn new(world_id: FrameWorldId, display_name: String, created_at: DateTime<Utc>) -> Self {
        Self {
            world_uri: FrameWorldUri::new(&world_id),
            head: None,
            display_name,
            description: None,
            created_at,
            updated_at: created_at,
        }
    }

    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self
    }

    /// The head and its positive publication number change together.
    pub fn with_head(mut self, revision_id: FrameWorldRevisionId, revision: NonZeroU64) -> Self {
        self.head = Some((revision_id, revision));
        self
    }

    pub fn world_id(&self) -> FrameWorldId {
        self.world_uri.world_id()
    }
    pub fn world_uri(&self) -> &FrameWorldUri {
        &self.world_uri
    }
    pub fn head_revision_id(&self) -> Option<&FrameWorldRevisionId> {
        self.head.as_ref().map(|(id, _)| id)
    }
    pub fn revision(&self) -> u64 {
        self.head.as_ref().map_or(0, |(_, n)| n.get())
    }
}

impl TryFrom<SummaryWire> for FrameWorldSummary {
    type Error = FrameWorldError;
    fn try_from(wire: SummaryWire) -> Result<Self, Self::Error> {
        if wire.world_uri.world_id() != wire.world_id {
            return Err(FrameWorldError::new("world_id disagrees with world_uri"));
        }
        let head = match (wire.head_revision_id, NonZeroU64::new(wire.revision)) {
            (None, None) => None,
            (Some(id), Some(revision)) => Some((id, revision)),
            _ => {
                return Err(FrameWorldError::new(
                    "world head requires a positive revision; an empty world requires revision zero",
                ));
            }
        };
        Ok(Self {
            world_uri: wire.world_uri,
            head,
            display_name: wire.display_name,
            description: wire.description,
            created_at: wire.created_at,
            updated_at: wire.updated_at,
        })
    }
}
impl From<FrameWorldSummary> for SummaryWire {
    fn from(value: FrameWorldSummary) -> Self {
        Self {
            world_id: value.world_id(),
            head_revision_id: value.head_revision_id().cloned(),
            revision: value.revision(),
            world_uri: value.world_uri,
            display_name: value.display_name,
            description: value.description,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RevisionWire", into = "RevisionWire")]
pub struct FrameWorldRevision {
    revision_uri: FrameWorldRevisionUri,
    revision: NonZeroU64,
    validated: ValidatedWorldTree,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RevisionWire {
    world_id: FrameWorldId,
    world_uri: FrameWorldUri,
    revision_id: FrameWorldRevisionId,
    revision_uri: FrameWorldRevisionUri,
    revision: u64,
    spec_digest: Sha256Digest,
    root_frame_uri: WorldFrameUri,
    tree: FrameWorldTree,
    created_at: DateTime<Utc>,
}

impl FrameWorldRevision {
    /// Only a complete admitted tree can become revision metadata.
    /// ```compile_fail
    /// use veoveo_frames_contract::FrameWorldRevision;
    /// fn corrupt(revision: &mut FrameWorldRevision) { revision.tree().frames.clear(); }
    /// ```
    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameWorldRevision, FrameWorldRevisionUri, FrameWorldTree};
    /// fn unchecked(uri: FrameWorldRevisionUri, revision: &FrameWorldRevision) {
    ///     FrameWorldRevision::new(uri, 1.try_into().unwrap(), FrameWorldTree { frames: vec![] }, revision.created_at());
    /// }
    /// ```
    pub fn new(
        revision_uri: FrameWorldRevisionUri,
        revision: NonZeroU64,
        validated: ValidatedWorldTree,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            revision_uri,
            revision,
            validated,
            created_at,
        }
    }

    /// Admit a stored/public tree while checking its separately carried digest and root.
    pub fn from_parts(
        revision_uri: FrameWorldRevisionUri,
        revision: NonZeroU64,
        tree: FrameWorldTree,
        root_frame_uri: WorldFrameUri,
        spec_digest: Sha256Digest,
        created_at: DateTime<Utc>,
    ) -> Result<Self, FrameWorldError> {
        let validated = ValidatedWorldTree::new(tree)?;
        if root_frame_uri != revision_uri.frame(validated.root_frame_id()) {
            return Err(FrameWorldError::new(
                "root_frame_uri disagrees with revision identity or tree root",
            ));
        }
        if &spec_digest != validated.spec_digest() {
            return Err(FrameWorldError::new(
                "world revision tree does not match spec_digest",
            ));
        }
        Ok(Self::new(revision_uri, revision, validated, created_at))
    }

    pub fn world_id(&self) -> FrameWorldId {
        self.revision_uri.world_id()
    }
    pub fn world_uri(&self) -> FrameWorldUri {
        FrameWorldUri::new(&self.world_id())
    }
    pub fn revision_id(&self) -> FrameWorldRevisionId {
        self.revision_uri.revision_id()
    }
    pub fn revision_uri(&self) -> &FrameWorldRevisionUri {
        &self.revision_uri
    }
    pub fn revision(&self) -> NonZeroU64 {
        self.revision
    }
    pub fn spec_digest(&self) -> &Sha256Digest {
        self.validated.spec_digest()
    }
    pub fn root_frame_uri(&self) -> WorldFrameUri {
        self.revision_uri.frame(self.validated.root_frame_id())
    }
    pub fn tree(&self) -> &FrameWorldTree {
        self.validated.tree()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn frame(&self, frame_uri: &WorldFrameUri) -> Option<&FrameNode> {
        if frame_uri.revision_uri() != self.revision_uri {
            return None;
        }
        let id = frame_uri.frame_id();
        self.tree().frames.iter().find(|frame| frame.frame_id == id)
    }
}

impl TryFrom<RevisionWire> for FrameWorldRevision {
    type Error = FrameWorldError;
    fn try_from(wire: RevisionWire) -> Result<Self, Self::Error> {
        if wire.world_id != wire.revision_uri.world_id()
            || wire.world_uri.world_id() != wire.world_id
            || wire.revision_id != wire.revision_uri.revision_id()
        {
            return Err(FrameWorldError::new(
                "world and revision identities disagree with revision_uri",
            ));
        }
        let number = NonZeroU64::new(wire.revision)
            .ok_or_else(|| FrameWorldError::new("published revision must be positive"))?;
        Self::from_parts(
            wire.revision_uri,
            number,
            wire.tree,
            wire.root_frame_uri,
            wire.spec_digest,
            wire.created_at,
        )
    }
}
impl From<FrameWorldRevision> for RevisionWire {
    fn from(value: FrameWorldRevision) -> Self {
        Self {
            world_id: value.world_id(),
            world_uri: value.world_uri(),
            revision_id: value.revision_id(),
            revision: value.revision.get(),
            spec_digest: value.spec_digest().clone(),
            root_frame_uri: value.root_frame_uri(),
            revision_uri: value.revision_uri,
            tree: value.validated.into_tree(),
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SourceWire", into = "SourceWire")]
pub struct FrameSourceReference {
    revision_uri: FrameWorldRevisionUri,
    pub digest: Sha256Digest,
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SourceWire {
    revision_uri: FrameWorldRevisionUri,
    revision_id: FrameWorldRevisionId,
    digest: Sha256Digest,
}
impl FrameSourceReference {
    pub fn new(revision_uri: FrameWorldRevisionUri, digest: Sha256Digest) -> Self {
        Self {
            revision_uri,
            digest,
        }
    }
    pub fn revision_uri(&self) -> &FrameWorldRevisionUri {
        &self.revision_uri
    }
    pub fn revision_id(&self) -> FrameWorldRevisionId {
        self.revision_uri.revision_id()
    }
}
impl From<&FrameWorldRevision> for FrameSourceReference {
    fn from(value: &FrameWorldRevision) -> Self {
        Self::new(value.revision_uri().clone(), value.spec_digest().clone())
    }
}
impl TryFrom<SourceWire> for FrameSourceReference {
    type Error = FrameWorldError;
    fn try_from(wire: SourceWire) -> Result<Self, Self::Error> {
        if wire.revision_uri.revision_id() != wire.revision_id {
            return Err(FrameWorldError::new(
                "source revision_id disagrees with revision_uri",
            ));
        }
        Ok(Self::new(wire.revision_uri, wire.digest))
    }
}
impl From<FrameSourceReference> for SourceWire {
    fn from(value: FrameSourceReference) -> Self {
        Self {
            revision_id: value.revision_id(),
            revision_uri: value.revision_uri,
            digest: value.digest,
        }
    }
}
