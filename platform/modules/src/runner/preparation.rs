//! Generation-fenced database installation primitives; compositions calculate identity.
use super::{PreparedInstallation, RunnerError, executor::transaction};
use crate::{InstallationGeneration, PREPARATION_TABLE, PreparationKey};
use serde::{Deserialize, Serialize};
use surrealdb::{
    Connection, Surreal,
    types::{RecordId, SurrealValue},
};

/// Narrow database-editor credentials; no arbitrary statement can enter execution.
pub struct DatabaseEditorCredentials {
    username: String,
    password: String,
}
impl std::fmt::Debug for DatabaseEditorCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseEditorCredentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
impl DatabaseEditorCredentials {
    pub fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, RunnerError> {
        let username = username.into();
        let password = password.into();
        if username.is_empty()
            || username.len() > 64
            || !username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || password.is_empty()
        {
            return Err(RunnerError::new("invalid database-editor credentials"));
        }
        Ok(Self { username, password })
    }
    pub(crate) fn username(&self) -> &str {
        &self.username
    }
    pub(crate) fn statement(&self) -> String {
        let literal =
            serde_json::to_string(&self.password).expect("string serialization cannot fail");
        include_str!("../../queries/preparation_editor.surql").replace("{literal}", &literal)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, SurrealValue)]
pub(crate) struct PreparationRecord {
    pub id: RecordId,
    pub generation: String,
    pub identity: String,
    pub complete: bool,
}
impl PreparationRecord {
    pub(crate) fn new(key: &PreparationKey, complete: bool) -> Self {
        Self {
            id: record_id(),
            generation: key.generation().to_string(),
            identity: key.identity().to_owned(),
            complete,
        }
    }
    pub(crate) fn matches(&self, key: &PreparationKey) -> bool {
        self.id == record_id()
            && self.generation == key.generation().to_string()
            && self.identity == key.identity()
    }
    pub(crate) fn check_advance(&self, key: &PreparationKey) -> Result<(), RunnerError> {
        if self.identity.len() != 64
            || !self
                .identity
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(RunnerError::new("invalid persisted preparation identity"));
        }
        let generation: InstallationGeneration = self
            .generation
            .parse()
            .map_err(|_| RunnerError::new("invalid persisted preparation generation"))?;
        if self.id != record_id() || generation > key.generation() {
            return Err(RunnerError::new(
                "stale preparation generation; a newer installation preparation owns this database",
            ));
        }
        if generation == key.generation() && self.identity != key.identity() {
            return Err(RunnerError::new(
                "installation generation conflicts with a different preparation identity; advance generation for a changed plan or credential revision",
            ));
        }
        Ok(())
    }
}
pub(crate) fn record_id() -> RecordId {
    RecordId::new(PREPARATION_TABLE, "current")
}
impl PreparedInstallation<'_> {
    /// Claim the epoch before legacy schema effects or credential rotation.
    pub async fn claim_preparation<C: Connection>(
        &self,
        db: &Surreal<C>,
        key: &PreparationKey,
    ) -> Result<(), RunnerError> {
        self.initialize(db).await?;
        let result = transaction::execute(
            db,
            transaction::Operation::PreparationFence(key.clone()),
            Default::default(),
        )
        .await;
        if let Err(error) = result {
            if !error.disposition.may_observe_winner() {
                return Err(error.error);
            }
            let existing = read(db).await?;
            if existing.is_none_or(|record| !record.matches(key)) {
                return Err(error.error);
            }
        }
        Ok(())
    }
    /// Rotate the editor and record completion in the same native transaction.
    pub async fn complete_preparation<C: Connection>(
        &self,
        db: &Surreal<C>,
        key: &PreparationKey,
        credentials: DatabaseEditorCredentials,
    ) -> Result<(), RunnerError> {
        let result = transaction::execute(
            db,
            transaction::Operation::PreparationComplete {
                key: key.clone(),
                credentials,
            },
            Default::default(),
        )
        .await;
        if let Err(error) = result {
            if !error.disposition.may_observe_winner() {
                return Err(error.error);
            }
            let existing = read(db).await?;
            if existing.is_none_or(|record| !record.matches(key) || !record.complete) {
                return Err(error.error);
            }
        }
        Ok(())
    }
    /// False means the requested generation has not completed yet. A newer epoch
    /// or conflicting identity is a hard error, not a readiness retry.
    pub async fn preparation_ready<C: Connection>(
        &self,
        db: &Surreal<C>,
        key: &PreparationKey,
    ) -> Result<bool, RunnerError> {
        let Some(record) = read(db).await? else {
            return Ok(false);
        };
        record.check_advance(key)?;
        Ok(record.matches(key) && record.complete)
    }
    pub async fn require_preparation<C: Connection>(
        &self,
        db: &Surreal<C>,
        key: &PreparationKey,
    ) -> Result<(), RunnerError> {
        if !self.preparation_ready(db, key).await? {
            return Err(RunnerError::new(
                "current installation preparation is incomplete",
            ));
        }
        Ok(())
    }
}
async fn read<C: Connection>(db: &Surreal<C>) -> Result<Option<PreparationRecord>, RunnerError> {
    use surrealdb::types::{ErrorDetails, NotFoundError};
    fn absent(error: &surrealdb::Error) -> bool {
        matches!(
            error.details(),
            ErrorDetails::NotFound(Some(
                NotFoundError::Namespace { .. } | NotFoundError::Database { .. }
            ))
        ) || matches!(error.details(), ErrorDetails::NotFound(Some(NotFoundError::Table { name })) if name == PREPARATION_TABLE)
    }
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        db.query(include_str!("../../queries/preparation_read.surql"))
            .bind(("record", record_id())),
    )
    .await
    .map_err(|_| RunnerError::new("preparation inspection timed out"))?;
    let mut response = match response {
        Ok(response) => response,
        Err(error) if absent(&error) => return Ok(None),
        Err(_) => return Err(RunnerError::new("preparation inspection failed")),
    };
    match response.take(0) {
        Ok(record) => Ok(record),
        Err(error) if absent(&error) => Ok(None),
        Err(_) => Err(RunnerError::new("invalid preparation marker")),
    }
}
