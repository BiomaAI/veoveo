//! Durable shared-chat operations. Policy admission is server-owned; membership,
//! current context and message order are enforced within each transaction.
mod membership;
mod participation;
pub use participation::*;
mod operations;
pub use operations::*;
mod people;
mod personal;
pub use personal::*;
mod records;
mod runs;
pub use runs::*;

pub use membership::WorkspaceSettings;
pub use people::{
    WorkspaceContext, WorkspaceIdentity, WorkspaceInvitationSummary, WorkspacePerson,
};
pub use records::*;

use std::time::Duration;

use surrealdb::types::{RecordId, SurrealValue, ToSql, Value};
use uuid::Uuid;

use crate::{PlatformStore, WorkspaceChatId, WorkspaceMemberId, store::primary_transaction_error};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkspaceError {
    #[error("invalid Workspace field: {0}")]
    Invalid(&'static str),
    #[error("chat or related record is not available")]
    NotFound,
    #[error("operation is not permitted")]
    Forbidden,
    #[error("request conflicts with current chat state")]
    Conflict,
    #[error("Workspace persistence is unavailable")]
    Unavailable,
}

type Result<T> = std::result::Result<T, WorkspaceError>;

#[derive(Clone, SurrealValue)]
struct ChatQuery {
    chat: RecordId,
    after: i64,
    limit: i64,
}

#[derive(Clone, SurrealValue)]
struct CreateChat {
    chat: RecordId,
    member: RecordId,
    title: String,
}

impl PlatformStore {
    pub async fn workspace_head(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
    ) -> Result<i64> {
        self.workspace_query(
            authority,
            chat.record_id(),
            concat!(
                include_str!("runs/reconcile.surql"),
                "RETURN $command.sequence;"
            ),
        )
        .await
    }

    /// A latency hint only. Authorized consumers read the durable chat head.
    pub async fn workspace_wakes(&self) -> Result<crate::LiveStream<WorkspaceEvent>> {
        self.client()
            .select("workspace_event")
            .live()
            .await
            .map_err(|_| WorkspaceError::Unavailable)
    }

    pub async fn workspace_recent_snapshot(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        before: Option<i64>,
        limit: u32,
    ) -> Result<WorkspaceSnapshot> {
        let before = before.unwrap_or(i64::MAX);
        validate_page(before, limit)?;
        let mut snapshot: WorkspaceSnapshot = self
            .workspace_query(
                authority,
                ChatQuery {
                    chat: chat.record_id(),
                    after: before,
                    limit: i64::from(limit),
                },
                include_str!("queries/recent.surql"),
            )
            .await?;
        snapshot.messages.reverse();
        Ok(snapshot)
    }

    pub async fn create_workspace_chat(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        title: &str,
    ) -> Result<WorkspaceChat> {
        validate_text(title, "title", 200)?;
        let command = CreateChat {
            chat: chat.record_id(),
            member: human_member(chat, &authority.principal).record_id(),
            title: title.to_owned(),
        };
        self.workspace_query(authority, command, include_str!("queries/create.surql"))
            .await
    }

    /// Bounded recent-chat navigation; never returns the installation inventory.
    pub async fn list_workspace_chats(
        &self,
        authority: &WorkspaceAuthority,
    ) -> Result<Vec<WorkspaceChat>> {
        self.workspace_query(
            authority,
            false,
            "RETURN SELECT * FROM workspace_chat WHERE tenant = $authority.tenant \
             AND work_context = $authority.work_context AND id IN \
             (SELECT VALUE chat FROM workspace_member WHERE principal = $authority.principal \
             AND active = true) ORDER BY updated_at DESC LIMIT 100;",
        )
        .await
    }

    pub async fn workspace_snapshot(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        after: i64,
        limit: u32,
    ) -> Result<WorkspaceSnapshot> {
        validate_page(after, limit)?;
        self.workspace_query(
            authority,
            ChatQuery {
                chat: chat.record_id(),
                after,
                limit: i64::from(limit),
            },
            include_str!("queries/snapshot.surql"),
        )
        .await
    }

    pub async fn workspace_events(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        after: i64,
        limit: u32,
    ) -> Result<WorkspaceEventPage> {
        validate_page(after, limit)?;
        self.workspace_query(
            authority,
            ChatQuery {
                chat: chat.record_id(),
                after,
                limit: i64::from(limit),
            },
            include_str!("queries/events.surql"),
        )
        .await
    }

    async fn workspace_query<C: SurrealValue + Clone, R: SurrealValue>(
        &self,
        authority: &WorkspaceAuthority,
        command: C,
        sql: &'static str,
    ) -> Result<R> {
        let query = format!(
            "BEGIN TRANSACTION; \
             IF !fn::workspace_authority($authority) {{ THROW 'workspace_forbidden'; }}; \
             {sql} COMMIT TRANSACTION;"
        );
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(query.clone())
                .bind(("authority", authority.clone()))
                .bind(("command", command.clone()))
                .await
                .map_err(|_| WorkspaceError::Unavailable)?;
            if let Some(error) = primary_transaction_error(response.take_errors()) {
                if retryable(&error) && attempt < 7 {
                    tokio::time::sleep(Duration::from_millis(1 << attempt)).await;
                    continue;
                }
                return Err(classify(&error));
            }
            // The final statement is COMMIT; the preceding RETURN was evaluated
            // within that transaction, including its authorization snapshot.
            let last = response
                .num_statements()
                .checked_sub(2)
                .ok_or(WorkspaceError::Unavailable)?;
            let value: Value = response
                .take(last)
                .map_err(|_| WorkspaceError::Unavailable)?;
            return R::from_value(value).map_err(|_| WorkspaceError::Unavailable);
        }
        Err(WorkspaceError::Unavailable)
    }
}

fn human_member(chat: WorkspaceChatId, principal: &RecordId) -> WorkspaceMemberId {
    WorkspaceMemberId::from_uuid(Uuid::new_v5(&chat.as_uuid(), principal.to_sql().as_bytes()))
}

fn validate_text(text: &str, field: &'static str, max: usize) -> Result<()> {
    if text.trim().is_empty() || text.len() > max || text.contains('\0') {
        return Err(WorkspaceError::Invalid(field));
    }
    Ok(())
}

fn validate_page(after: i64, limit: u32) -> Result<()> {
    if after < 0 || !(1..=200).contains(&limit) {
        return Err(WorkspaceError::Invalid("page"));
    }
    Ok(())
}

fn retryable(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) || error.message().starts_with("Transaction conflict:")
}

fn classify(error: &surrealdb::Error) -> WorkspaceError {
    let message = error.message();
    if message.contains("workspace_not_found") {
        WorkspaceError::NotFound
    } else if message.contains("workspace_forbidden") {
        WorkspaceError::Forbidden
    } else if message.contains("workspace_conflict") || retryable(error) {
        WorkspaceError::Conflict
    } else {
        WorkspaceError::Unavailable
    }
}
