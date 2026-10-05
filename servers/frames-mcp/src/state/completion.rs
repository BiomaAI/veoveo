//! Database matching precedes the completion limit and uses typed parent identities.
use super::{FrameScope, FramesState, records::FrameWorldRevisionRecord, world_revision};
use crate::contract::{FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri};
use anyhow::{Result, ensure};
use surrealdb::types::SurrealValue;

const LIMIT: usize = 101;

fn needle(value: &str) -> Result<String> {
    ensure!(
        value.len() <= 512 && !value.chars().any(char::is_control),
        "completion search text must be at most 512 bytes without control characters"
    );
    Ok(value.to_lowercase())
}

impl FramesState {
    pub async fn complete_worlds(
        &self,
        scope: &FrameScope,
        search: &str,
    ) -> Result<Vec<FrameWorldId>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/complete_worlds.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("needle", needle(search)?))
            .bind(("limit", LIMIT))
            .await?
            .check()?;
        response
            .take::<Vec<String>>(0)?
            .into_iter()
            .map(|value| Ok(FrameWorldId::parse(value)?))
            .collect()
    }

    pub async fn complete_revisions(
        &self,
        scope: &FrameScope,
        world: &FrameWorldId,
        search: &str,
    ) -> Result<Vec<FrameWorldRevisionId>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/complete_revisions.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("world_key", world.to_string()))
            .bind(("needle", needle(search)?))
            .bind(("limit", LIMIT))
            .await?
            .check()?;
        response
            .take::<Vec<String>>(0)?
            .into_iter()
            .map(|value| Ok(FrameWorldRevisionId::parse(value)?))
            .collect()
    }

    pub async fn complete_frames(
        &self,
        scope: &FrameScope,
        revision: &FrameWorldRevisionUri,
        search: &str,
    ) -> Result<Vec<FrameId>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/complete_frames.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("world_key", revision.world_id().to_string()))
            .bind(("revision_key", revision.revision_id().to_string()))
            .bind(("needle", needle(search)?))
            .bind(("limit", LIMIT))
            .await?
            .check()?;
        #[derive(SurrealValue)]
        struct Selection {
            revision: FrameWorldRevisionRecord,
            ids: Vec<String>,
        }
        let Some(selected) = response.take::<Option<Selection>>(1)? else {
            return Ok(Vec::new());
        };
        let revision = world_revision(selected.revision)?;
        selected
            .ids
            .into_iter()
            .map(|value| {
                let id = FrameId::parse(value)?;
                ensure!(
                    revision
                        .tree()
                        .frames
                        .iter()
                        .any(|node| node.frame_id == id),
                    "frame completion differs from revision"
                );
                Ok(id)
            })
            .collect()
    }
}
