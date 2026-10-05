//! Typed world mutations and their transactional policy belong to Frames.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use surrealdb::{
    IndexedResults,
    types::{RecordId, SurrealValue, Uuid as SurrealUuid, Value},
};

use super::records::{FrameWorldRecord, FrameWorldRevisionRecord};
use super::{FrameScope, FramesState, world_revision, world_summary};
use crate::contract::{
    CreateWorldRequest, FrameWorldRevisionId, FrameWorldSummary, PublishWorldOutput,
    PublishWorldRequest, ValidatedWorldTree,
};

#[derive(SurrealValue)]
struct WorldContent {
    tenant: RecordId,
    owner: RecordId,
    world_key: String,
    display_name: String,
    description: Option<String>,
    head_revision: Option<RecordId>,
    head_revision_key: Option<String>,
    revision: i64,
    classification: String,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(SurrealValue)]
struct Publication {
    world: FrameWorldRecord,
    revision: FrameWorldRevisionRecord,
    created: bool,
}

impl Publication {
    fn checked(self) -> Result<PublishWorldOutput> {
        // SQL supplies one snapshot. Check the public identity/parent model before exposing it.
        if self.revision.world != self.world.id
            || self.revision.tenant != self.world.tenant
            || self.revision.owner != self.world.owner
            || self.revision.world_key != self.world.world_key
            || self.world.head_revision.as_ref() != Some(&self.revision.id)
            || self.world.head_revision_key.as_ref() != Some(&self.revision.revision_key)
            || self.world.revision != self.revision.revision
        {
            bail!("frame world publication has inconsistent parent metadata");
        }
        Ok(PublishWorldOutput {
            world: world_summary(self.world)?,
            revision: world_revision(self.revision)?,
            created: self.created,
        })
    }
}

impl FramesState {
    pub async fn create_world(
        &self,
        scope: &FrameScope,
        request: CreateWorldRequest,
    ) -> Result<FrameWorldSummary> {
        validate_text("display_name", &request.display_name, 512)?;
        if let Some(description) = &request.description {
            validate_text("description", description, 2048)?;
        }
        let labels = scope
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for label in &labels {
            validate_text("label", label, 256)?;
        }
        let now = Utc::now();
        let content = WorldContent {
            tenant: scope.identity.tenant_id.record_id(),
            owner: scope.identity.principal_id.record_id(),
            world_key: request.world_id.to_string(),
            display_name: request.display_name.clone(),
            description: request.description.clone(),
            head_revision: None,
            head_revision_key: None,
            revision: 0,
            classification: "gateway_labels".to_owned(),
            labels: labels.clone(),
            created_at: now,
            updated_at: now,
        };
        let result = self
            .store
            .client()
            .query(include_str!("queries/worlds/create.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("world_key", request.world_id.to_string()))
            .bind(("clearance", labels))
            .bind((
                "world_record",
                RecordId::new("frame_world", SurrealUuid::from(uuid::Uuid::now_v7())),
            ))
            .bind(("content", content))
            .await
            .map_err(anyhow::Error::from)
            .and_then(transaction_result::<FrameWorldRecord>);
        match result {
            Ok(world) => world_summary(world),
            Err(error) => {
                // Concurrent creation and a lost commit acknowledgement settle only from a
                // currently visible record with the requested metadata. Never retry a write.
                if let Some(world) = self.get_world(scope, &request.world_id).await?
                    && world.display_name == request.display_name
                    && world.description == request.description
                {
                    return Ok(world);
                }
                Err(error
                    .context("frame world is unavailable or conflicts with the requested metadata"))
            }
        }
    }

    pub async fn publish_world(
        &self,
        scope: &FrameScope,
        request: PublishWorldRequest,
    ) -> Result<PublishWorldOutput> {
        let validated = ValidatedWorldTree::new(request.tree)?;
        let revision_id =
            FrameWorldRevisionId::parse(format!("revision-{}", uuid::Uuid::now_v7()))?;
        let digest = validated.spec_digest().clone();
        let result = self
            .store
            .client()
            .query(include_str!("queries/worlds/publish.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("owner", scope.identity.principal_id.record_id()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("world_key", request.world_id.to_string()))
            .bind((
                "expected_head",
                request.expected_head_revision_id.map(String::from),
            ))
            .bind((
                "revision_record",
                RecordId::new(
                    "frame_world_revision",
                    SurrealUuid::from(uuid::Uuid::now_v7()),
                ),
            ))
            .bind(("revision_key", revision_id.to_string()))
            .bind(("digest", digest.hex().to_owned()))
            .bind(("root", validated.root_frame_id().to_string()))
            .bind((
                "frame_ids",
                validated
                    .tree()
                    .frames
                    .iter()
                    .map(|frame| frame.frame_id.to_string())
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "tree",
                super::storage_codec::WorldTree(validated.into_tree()),
            ))
            .bind(("now", Utc::now()))
            .await
            .map_err(anyhow::Error::from)
            .and_then(transaction_result::<Publication>);
        match result {
            Ok(publication) => publication.checked(),
            Err(error) => {
                let response = self
                    .store
                    .client()
                    .query(include_str!("queries/worlds/replay.surql"))
                    .bind(("tenant", scope.identity.tenant_id.record_id()))
                    .bind(("owner", scope.identity.principal_id.record_id()))
                    .bind((
                        "clearance",
                        scope
                            .data_labels
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>(),
                    ))
                    .bind(("world_key", request.world_id.to_string()))
                    .bind(("digest", digest.hex().to_owned()))
                    .await?;
                if let Some(publication) = transaction_result::<Option<Publication>>(response)? {
                    return publication.checked();
                }
                Err(error.context(
                    "frame world is unavailable or conflicts with the requested publication",
                ))
            }
        }
    }
}

fn transaction_result<T: SurrealValue>(response: IndexedResults) -> Result<T> {
    let mut response = response.check()?;
    // The RETURN is evaluated inside the transaction; COMMIT is the final statement.
    let index = response
        .num_statements()
        .checked_sub(2)
        .context("missing Frames transaction result")?;
    let value: Value = response.take(index)?;
    T::from_value(value).map_err(Into::into)
}

fn validate_text(field: &str, value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        bail!(
            "frame world {field} must be nonempty, at most {max} bytes, and contain no control characters"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
