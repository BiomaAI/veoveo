//! Frames owns its read policy and typed query inputs. Store owns the connection
//! and driver records; it does not import the Frames runtime or domain contract.
use super::records::{FrameWorldRecord, FrameWorldRevisionRecord};
use super::{FrameScope, FramesState, world_revision, world_summary};
use crate::contract::{
    FRAME_WORLD_PAGE_SIZE, FrameNode, FrameWorldCursor, FrameWorldId, FrameWorldPage,
    FrameWorldRevision, FrameWorldRevisionUri, FrameWorldSummary, WorldFrameUri,
};
use anyhow::{Result, bail};

// Checking the linked parent in the same query prevents deleted, cross-tenant,
// or mismatched world records from authorizing a revision through its copied key.

impl FramesState {
    pub async fn worlds_page(
        &self,
        scope: &FrameScope,
        after: Option<&FrameWorldCursor>,
    ) -> Result<FrameWorldPage> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/world_page.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("after", after.map(|cursor| cursor.after().to_string())))
            .bind(("limit", FRAME_WORLD_PAGE_SIZE + 1))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        let mut worlds: Vec<FrameWorldRecord> = response.take(0)?;
        let has_more = worlds.len() > FRAME_WORLD_PAGE_SIZE;
        worlds.truncate(FRAME_WORLD_PAGE_SIZE);
        let items = worlds
            .into_iter()
            .map(world_summary)
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = has_more.then(|| {
            FrameWorldCursor::new(&items.last().expect("overfull page has items").world_id())
        });
        Ok(FrameWorldPage {
            items,
            limit: FRAME_WORLD_PAGE_SIZE,
            next_cursor,
        })
    }

    pub async fn get_world(
        &self,
        scope: &FrameScope,
        world_id: &FrameWorldId,
    ) -> Result<Option<FrameWorldSummary>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/read_world.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("world_key", world_id.to_string()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        let worlds: Vec<FrameWorldRecord> = response.take(0)?;
        worlds.into_iter().next().map(world_summary).transpose()
    }

    pub async fn get_revision(
        &self,
        scope: &FrameScope,
        revision_uri: &FrameWorldRevisionUri,
    ) -> Result<Option<FrameWorldRevision>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/read_revision.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("world_key", revision_uri.world_id().to_string()))
            .bind(("revision_key", revision_uri.revision_id().to_string()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        let revisions: Vec<FrameWorldRevisionRecord> = response.take(0)?;
        revisions.into_iter().next().map(world_revision).transpose()
    }

    pub async fn get_head_revision(
        &self,
        scope: &FrameScope,
        world_id: &FrameWorldId,
    ) -> Result<Option<FrameWorldRevision>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/read_head_revision.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("world_key", world_id.to_string()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        let revisions: Vec<FrameWorldRevisionRecord> = response.take(0)?;
        revisions.into_iter().next().map(world_revision).transpose()
    }

    /// Select a single resource node in SQL. Coordinate conversion separately
    /// reads the authorized complete revision to resolve its transform chain.
    pub async fn get_frame(
        &self,
        scope: &FrameScope,
        frame_uri: &WorldFrameUri,
    ) -> Result<Option<FrameNode>> {
        let revision_uri = frame_uri.revision_uri();
        let mut response = self
            .store
            .client()
            .query(include_str!("queries/read_frame.surql"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("world_key", revision_uri.world_id().to_string()))
            .bind(("revision_key", revision_uri.revision_id().to_string()))
            .bind(("frame_key", frame_uri.frame_id().to_string()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        let matches: Vec<FrameWorldRevisionRecord> = response.take(0)?;
        let mut frames = Vec::new();
        for revision in matches {
            let revision = world_revision(revision)?;
            frames.extend(
                revision
                    .tree()
                    .frames
                    .iter()
                    .filter(|frame| frame.frame_id == frame_uri.frame_id())
                    .cloned(),
            );
        }
        let mut frames = frames.into_iter();
        let frame = frames.next();
        if frames.next().is_some() {
            bail!("frame world revision contains duplicate frame identities");
        }
        Ok(frame)
    }
}
