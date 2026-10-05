use std::collections::BTreeSet;

use crate::contract::{
    FrameWorldId, FrameWorldRevision, FrameWorldRevisionId, FrameWorldRevisionUri, WorldFrameUri,
};
use anyhow::{Context, Result, anyhow, bail};
use veoveo_platform_store::{OpenObject, PlatformIdentity, PlatformStore};

use crate::contract::FrameWorldSummary;
use records::{FrameWorldRecord, FrameWorldRevisionRecord};
use veoveo_types::DataLabelId;

mod completion;
mod operations;
mod reads;
mod records;
mod worlds;
pub use operations::FrameOperationScope;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod read_tests;

#[derive(Clone, Debug)]
pub struct FrameScope {
    identity: PlatformIdentity,
    data_labels: BTreeSet<DataLabelId>,
}

impl FrameScope {
    /// World policy consumes validated labels and resolved database identities.
    /// ```compile_fail
    /// use veoveo_frames_mcp::state::FrameScope;
    /// use veoveo_platform_store::PlatformIdentity;
    /// fn raw_labels(identity: PlatformIdentity) {
    ///     FrameScope::new(identity, std::collections::BTreeSet::from(["cui".to_owned()]));
    /// }
    /// ```
    pub fn new(identity: PlatformIdentity, data_labels: BTreeSet<DataLabelId>) -> Self {
        Self {
            identity,
            data_labels,
        }
    }
}

#[derive(Clone)]
pub struct FramesState {
    store: PlatformStore,
}

impl FramesState {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub async fn require_revision(
        &self,
        scope: &FrameScope,
        revision_uri: &FrameWorldRevisionUri,
    ) -> Result<FrameWorldRevision> {
        self.get_revision(scope, revision_uri)
            .await?
            .ok_or_else(|| anyhow!("unknown frame world revision `{revision_uri}`"))
    }

    pub async fn require_frame_revision(
        &self,
        scope: &FrameScope,
        frame_uri: &WorldFrameUri,
    ) -> Result<FrameWorldRevision> {
        let revision = self
            .require_revision(scope, &frame_uri.revision_uri())
            .await?;
        if revision.frame(frame_uri).is_none() {
            bail!("unknown world frame `{frame_uri}`");
        }
        Ok(revision)
    }
}

fn world_summary(record: FrameWorldRecord) -> Result<FrameWorldSummary> {
    let world_id = FrameWorldId::parse(record.world_key)?;
    let mut summary = FrameWorldSummary::new(world_id, record.display_name, record.created_at)
        .with_description(record.description);
    match (record.head_revision_key, u64::try_from(record.revision)?) {
        (None, 0) => {}
        (Some(id), number) if number > 0 => {
            summary = summary.with_head(FrameWorldRevisionId::parse(id)?, number.try_into()?);
        }
        _ => bail!("stored world head and publication number disagree"),
    }
    summary.updated_at = record.updated_at;
    Ok(summary)
}

fn world_revision(record: FrameWorldRevisionRecord) -> Result<FrameWorldRevision> {
    let world_id = FrameWorldId::parse(record.world_key)?;
    let revision_id = FrameWorldRevisionId::parse(record.revision_key)?;
    let revision_uri = FrameWorldRevisionUri::new(&world_id, &revision_id);
    let root_frame_id = crate::contract::FrameId::parse(record.root_frame_key)?;
    let tree: crate::contract::FrameWorldTree =
        serde_json::from_value(value_from_object(record.definition))
            .context("decoding frame world revision")?;
    anyhow::ensure!(
        tree.frames
            .iter()
            .map(|frame| frame.frame_id.as_str())
            .eq(record.frame_ids.iter().map(String::as_str)),
        "frame identities differ from revision projection"
    );
    Ok(FrameWorldRevision::from_parts(
        revision_uri.clone(),
        u64::try_from(record.revision)
            .context("negative frame world revision")?
            .try_into()?,
        tree,
        WorldFrameUri::new(&revision_uri, &root_frame_id),
        veoveo_types::Sha256Digest::from_hex(record.spec_sha256)?,
        record.created_at,
    )?)
}

fn object_from_value(value: serde_json::Value) -> Result<OpenObject> {
    match value {
        serde_json::Value::Object(values) => Ok(OpenObject::new(values.into_iter().collect())),
        _ => bail!("frame world record must serialize as an object"),
    }
}

fn value_from_object(object: OpenObject) -> serde_json::Value {
    serde_json::Value::Object(object.into_map().into_iter().collect())
}
