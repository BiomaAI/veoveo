use std::collections::BTreeSet;

use crate::contract::{
    FrameWorldId, FrameWorldRevision, FrameWorldRevisionId, FrameWorldRevisionUri, WorldFrameUri,
};
use anyhow::{Context, Result, anyhow, bail};
use veoveo_platform_store::{
    FrameWorldDraft, FrameWorldRecord, FrameWorldRevisionDraft, FrameWorldRevisionRecord,
    OpenObject, PlatformIdentity, PlatformStore,
};

use crate::{
    contract::ValidatedWorldTree,
    contract::{CreateWorldRequest, FrameWorldSummary, PublishWorldOutput, PublishWorldRequest},
};

mod completion;
mod operations;
mod reads;
pub use operations::FrameOperationScope;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod read_tests;

#[derive(Clone, Debug)]
pub struct FrameScope {
    pub identity: PlatformIdentity,
    pub data_labels: BTreeSet<String>,
}

#[derive(Clone)]
pub struct FramesState {
    store: PlatformStore,
}

impl FramesState {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub async fn create_world(
        &self,
        scope: &FrameScope,
        request: CreateWorldRequest,
    ) -> Result<FrameWorldSummary> {
        if request.display_name.trim().is_empty() {
            bail!("display_name must not be blank");
        }
        if let Some(existing) = self.get_world(scope, &request.world_id).await? {
            if existing.display_name == request.display_name
                && existing.description == request.description
            {
                return Ok(existing);
            }
            bail!(
                "frame world `{}` already exists with different metadata",
                request.world_id
            );
        }
        let world = self
            .store
            .create_frame_world(FrameWorldDraft {
                identity: scope.identity.clone(),
                world_key: request.world_id.to_string(),
                display_name: request.display_name,
                description: request.description,
                classification: "gateway_labels".to_owned(),
                labels: scope.data_labels.iter().cloned().collect(),
            })
            .await?;
        world_summary(world)
    }

    pub async fn publish_world(
        &self,
        scope: &FrameScope,
        request: PublishWorldRequest,
    ) -> Result<PublishWorldOutput> {
        let validated = ValidatedWorldTree::new(request.tree)?;
        let revision_id = FrameWorldRevisionId::new(format!("revision-{}", uuid::Uuid::now_v7()))?;
        let publication = self
            .store
            .publish_frame_world_revision(FrameWorldRevisionDraft {
                identity: scope.identity.clone(),
                world_key: request.world_id.to_string(),
                expected_head_revision_key: request.expected_head_revision_id.map(String::from),
                revision_key: revision_id.to_string(),
                spec_sha256: validated.spec_digest().hex().to_owned(),
                root_frame_key: validated.root_frame_id().to_string(),
                definition: object_from_value(serde_json::to_value(validated.into_tree())?)?,
            })
            .await?;
        Ok(PublishWorldOutput {
            world: world_summary(publication.world)?,
            revision: world_revision(publication.revision)?,
            created: publication.created,
        })
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
    let world_id = FrameWorldId::new(record.world_key)?;
    let mut summary = FrameWorldSummary::new(world_id, record.display_name, record.created_at)
        .with_description(record.description);
    match (record.head_revision_key, u64::try_from(record.revision)?) {
        (None, 0) => {}
        (Some(id), number) if number > 0 => {
            summary = summary.with_head(FrameWorldRevisionId::new(id)?, number.try_into()?);
        }
        _ => bail!("stored world head and publication number disagree"),
    }
    summary.updated_at = record.updated_at;
    Ok(summary)
}

fn world_revision(record: FrameWorldRevisionRecord) -> Result<FrameWorldRevision> {
    let world_id = FrameWorldId::new(record.world_key)?;
    let revision_id = FrameWorldRevisionId::new(record.revision_key)?;
    let revision_uri = FrameWorldRevisionUri::new(&world_id, &revision_id);
    let root_frame_id = crate::contract::FrameId::new(record.root_frame_key)?;
    Ok(FrameWorldRevision::from_parts(
        revision_uri.clone(),
        u64::try_from(record.revision)
            .context("negative frame world revision")?
            .try_into()?,
        serde_json::from_value(value_from_object(record.definition))
            .context("decoding frame world revision")?,
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
