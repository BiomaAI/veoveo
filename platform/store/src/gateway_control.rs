//! Current authority readers share one checked pointer/revision read.
use crate::{GatewayControlActiveRecord, GatewayControlRevisionRecord, PlatformStore, StoreError};
use serde::Deserialize;
use surrealdb::types::SurrealValue;

#[derive(Deserialize, SurrealValue)]
struct Selection {
    head: Option<GatewayControlActiveRecord>,
    revision: Option<GatewayControlRevisionRecord>,
}
impl PlatformStore {
    /// Read one active pointer and its exact retained revision in one round trip.
    /// A dangling/mismatched pointer is an authority failure, not unconfigured state.
    /// Callers validate the document and account for elapsed read time before renewal.
    pub async fn active_gateway_control_revision(
        &self,
    ) -> Result<Option<GatewayControlRevisionRecord>, StoreError> {
        let mut response = self
            .client()
            .query(include_str!("gateway_control/active.surql"))
            .await?
            .check()?;
        let selected: Option<Selection> = response.take(2)?;
        match selected.ok_or(StoreError::InvalidGatewayControlRevision)? {
            Selection {
                head: None,
                revision: None,
            } => Ok(None),
            Selection {
                head: Some(head),
                revision: Some(revision),
            } if head.revision == revision.id && head.revision_id == revision.revision_id => {
                Ok(Some(revision))
            }
            _ => Err(StoreError::InvalidGatewayControlRevision),
        }
    }
}
