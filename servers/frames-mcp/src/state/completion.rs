//! Database matching precedes the completion limit and uses typed parent identities.
use super::{FrameScope, FramesState, reads::VISIBLE_REVISION};
use crate::contract::{FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri};
use anyhow::{Result, ensure};

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
        let mut response = self.store.client()
            .query("SELECT VALUE world_key FROM frame_world WHERE tenant = $tenant AND $clearance CONTAINSALL labels AND string::contains(string::lowercase(world_key), $needle) ORDER BY world_key ASC LIMIT $limit;")
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("clearance", scope.data_labels.iter().map(ToString::to_string).collect::<Vec<_>>()))
            .bind(("needle", needle(search)?))
            .bind(("limit", LIMIT))
            .await?.check()?;
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
        let mut response = self.store.client()
            .query(format!("SELECT VALUE revision_key FROM frame_world_revision WHERE {VISIBLE_REVISION} AND string::contains(string::lowercase(revision_key), $needle) ORDER BY revision_key ASC LIMIT $limit;"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("clearance", scope.data_labels.iter().map(ToString::to_string).collect::<Vec<_>>()))
            .bind(("world_key", world.to_string()))
            .bind(("needle", needle(search)?))
            .bind(("limit", LIMIT))
            .await?.check()?;
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
        let mut response = self.store.client()
            .query(format!("SELECT VALUE frame_id FROM (SELECT VALUE definition.frames FROM frame_world_revision WHERE {VISIBLE_REVISION} AND revision_key = $revision_key LIMIT 1)[0] WHERE string::contains(string::lowercase(frame_id), $needle) ORDER BY frame_id ASC LIMIT $limit;"))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("clearance", scope.data_labels.iter().map(ToString::to_string).collect::<Vec<_>>()))
            .bind(("world_key", revision.world_id().to_string()))
            .bind(("revision_key", revision.revision_id().to_string()))
            .bind(("needle", needle(search)?))
            .bind(("limit", LIMIT))
            .await?.check()?;
        response
            .take::<Vec<String>>(0)?
            .into_iter()
            .map(|value| Ok(FrameId::parse(value)?))
            .collect()
    }
}
