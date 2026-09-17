//! Bounded private observation; native MCP remains authoritative for Task state.
use chrono::{DateTime, Utc};
use futures::{StreamExt, stream::BoxStream};
use surrealdb::{
    Notification,
    types::{RecordId, SurrealValue},
};

use super::{
    Result, WorkspaceAuthority, WorkspaceError, WorkspaceInvitation, WorkspaceOperationPhase,
};
use crate::PlatformStore;

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspacePersonalOperation {
    pub id: RecordId,
    pub phase: WorkspaceOperationPhase,
    pub task_id: Option<String>,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspacePersonalState {
    pub operations: Vec<WorkspacePersonalOperation>,
    pub invitations: Vec<WorkspaceInvitation>,
}

#[derive(Clone, Debug, SurrealValue)]
pub struct WorkspacePersonalHint {
    pub principal: RecordId,
}

impl PlatformStore {
    /// Contains no arguments, tool results, input prompts or continuation state.
    pub async fn workspace_personal_state(
        &self,
        authority: &WorkspaceAuthority,
        profile: &str,
    ) -> Result<WorkspacePersonalState> {
        self.workspace_query(
            authority,
            profile.to_owned(),
            include_str!("queries/personal.surql"),
        )
        .await
    }

    /// Two projected LIVE queries per process, shared by all personal watches.
    /// A missed hint is recovered by a fresh authorized bounded state read.
    pub async fn workspace_personal_wakes(
        &self,
    ) -> Result<BoxStream<'static, Result<WorkspacePersonalHint>>> {
        let mut response = self
            .client()
            .query(
                "LIVE SELECT owner AS principal FROM workspace_operation; \
             LIVE SELECT invitee AS principal FROM workspace_invitation;",
            )
            .await
            .map_err(|_| WorkspaceError::Unavailable)?
            .check()
            .map_err(|_| WorkspaceError::Unavailable)?;
        let stream = response
            .stream::<Notification<WorkspacePersonalHint>>(())
            .map_err(|_| WorkspaceError::Unavailable)?;
        Ok(Box::pin(stream.map(|event| {
            event
                .map(|event| event.data)
                .map_err(|_| WorkspaceError::Unavailable)
        })))
    }
}
