//! Recording read-grant construction, transactional admission and scoped redemption.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use surrealdb::types::{RecordId, SurrealValue};

use super::{RecordingAccessScope, validate_text};
use crate::{
    PlatformStore, RecordingDatasetId, RecordingId, RecordingReadGrantClass, RecordingReadGrantId,
    RecordingReadGrantRecord, StoreError, primary_transaction_error,
};

const MAX_GRANT_RECORDINGS: usize = 500;

/// A bounded, normalized selection for one grant class and catalog revision.
#[derive(Clone, Debug)]
pub struct RecordingReadGrantRequest {
    dataset_id: RecordingDatasetId,
    grant_class: RecordingReadGrantClass,
    recording_ids: Vec<RecordingId>,
    catalog_revision: String,
}

impl RecordingReadGrantRequest {
    pub fn new(
        dataset_id: RecordingDatasetId,
        grant_class: RecordingReadGrantClass,
        mut recording_ids: Vec<RecordingId>,
        catalog_revision: impl Into<String>,
    ) -> Result<Self, StoreError> {
        let catalog_revision = catalog_revision.into();
        validate_text("catalog_revision", &catalog_revision, 128)?;
        if recording_ids.is_empty() || recording_ids.len() > MAX_GRANT_RECORDINGS {
            return Err(StoreError::InvalidRecordingField {
                field: "grant recordings",
                reason: "must contain 1..=500 recording UUIDs",
            });
        }
        recording_ids.sort_unstable();
        recording_ids.dedup();
        if grant_class != RecordingReadGrantClass::CatalogDataset && recording_ids.len() != 1 {
            return Err(StoreError::InvalidRecordingField {
                field: "grant recordings",
                reason: "viewer and projection grants require exactly one recording",
            });
        }
        Ok(Self {
            dataset_id,
            grant_class,
            recording_ids,
            catalog_revision,
        })
    }

    fn bindings(&self) -> RequestBindings {
        let mut digest = Sha256::new();
        digest.update(self.dataset_id.as_uuid().as_bytes());
        digest
            .update(serde_json::to_vec(&self.grant_class).expect("closed grant class serializes"));
        for recording in &self.recording_ids {
            digest.update(recording.as_uuid().as_bytes());
        }
        RequestBindings {
            dataset: self.dataset_id.record_id(),
            class: self.grant_class,
            recordings: self
                .recording_ids
                .iter()
                .copied()
                .map(RecordingId::record_id)
                .collect(),
            revision: self.catalog_revision.clone(),
            digest: hex::encode(digest.finalize()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordingReadGrantDraft {
    pub scope: RecordingAccessScope,
    pub request: RecordingReadGrantRequest,
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, SurrealValue)]
struct RequestBindings {
    dataset: RecordId,
    class: RecordingReadGrantClass,
    recordings: Vec<RecordId>,
    revision: String,
    digest: String,
}

// The request owns a nonempty, unique, bounded ID set. Selecting only IDs avoids
// decoding unrelated parent fields. Both reads and writes use this same admission.
const PARENTS_ADMITTED: &str = "$request.dataset.tenant = $scope.tenant
    AND array::len((SELECT VALUE id FROM $request.recordings
        WHERE tenant = $scope.tenant AND dataset = $request.dataset
          AND $scope.labels CONTAINSALL labels)) = array::len($request.recordings)";

impl PlatformStore {
    pub async fn create_recording_read_grant(
        &self,
        draft: RecordingReadGrantDraft,
    ) -> Result<RecordingReadGrantRecord, StoreError> {
        let id = RecordingReadGrantId::new();
        let mut response = self
            .db
            .query(format!(
                "BEGIN TRANSACTION;
             IF !({PARENTS_ADMITTED}) OR $expires <= time::now()
                OR $expires > time::now() + 1h {{
                 THROW 'recording_read_grant_admission_denied';
             }};
             CREATE ONLY $grant SET tenant = $scope.tenant, actor = $scope.actor,
                 work_context = $scope.context, policy_revision = $scope.policy,
                 dataset = $request.dataset, grant_class = $request.class,
                 recordings = $request.recordings, catalog_revision = $request.revision,
                 admitted_set_digest = $request.digest, expires_at = $expires,
                 created_at = time::now();
             COMMIT TRANSACTION;"
            ))
            .bind(("grant", id.record_id()))
            .bind(("scope", draft.scope.bindings()))
            .bind(("request", draft.request.bindings()))
            .bind(("expires", draft.expires_at))
            .await?;
        if let Some(error) = primary_transaction_error(response.take_errors()) {
            if error.is_thrown()
                && error
                    .message()
                    .contains("recording_read_grant_admission_denied")
            {
                return Err(StoreError::RecordingReadGrantConflict {
                    grant_id: "new".into(),
                });
            }
            return Err(error.into());
        }
        response
            .take::<Option<RecordingReadGrantRecord>>(2)?
            .ok_or(StoreError::MissingRecord {
                operation: "recording read grant creation",
            })
    }

    /// A client hint is reusable only under current authority and source visibility.
    pub async fn reusable_recording_read_grant(
        &self,
        scope: &RecordingAccessScope,
        request: &RecordingReadGrantRequest,
        grant_id: RecordingReadGrantId,
    ) -> Result<Option<RecordingReadGrantRecord>, StoreError> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM ONLY $grant WHERE tenant = $scope.tenant AND actor = $scope.actor
                AND work_context = $scope.context AND policy_revision = $scope.policy
                AND dataset = $request.dataset AND grant_class = $request.class
                AND recordings = $request.recordings AND catalog_revision = $request.revision
                AND admitted_set_digest = $request.digest AND expires_at > time::now()
                AND {PARENTS_ADMITTED};"
            ))
            .bind(("grant", grant_id.record_id()))
            .bind(("scope", scope.bindings()))
            .bind(("request", request.bindings()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Service-only redemption after verifying the signed, host-limited Redap token.
    /// The token subject supplies the grant identity; SQL admits the lifetime and class.
    pub async fn recording_redap_grant(
        &self,
        grant_id: RecordingReadGrantId,
    ) -> Result<Option<RecordingReadGrantRecord>, StoreError> {
        let mut response = self.db
            .query("SELECT * FROM ONLY $grant WHERE expires_at > time::now() AND grant_class IN $classes;")
            .bind(("grant", grant_id.record_id()))
            .bind(("classes", vec![RecordingReadGrantClass::ViewerSegment, RecordingReadGrantClass::CatalogDataset]))
            .await?.check()?;
        Ok(response.take(0)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_bounded_and_normalized_before_hashing() {
        let dataset = RecordingDatasetId::new();
        let a = RecordingId::new();
        let b = RecordingId::new();
        let request = |ids| {
            RecordingReadGrantRequest::new(
                dataset,
                RecordingReadGrantClass::CatalogDataset,
                ids,
                "r1",
            )
        };
        assert!(request(vec![]).is_err());
        assert!(request(vec![a; 501]).is_err());
        let first = request(vec![b, a, a]).unwrap().bindings();
        let second = request(vec![a, b]).unwrap().bindings();
        assert_eq!(first.recordings, second.recordings);
        assert_eq!(first.digest, second.digest);
        assert_ne!(first.digest, request(vec![a]).unwrap().bindings().digest);
        for class in [
            RecordingReadGrantClass::ViewerSegment,
            RecordingReadGrantClass::AppProjection,
        ] {
            assert!(RecordingReadGrantRequest::new(dataset, class, vec![a, b], "r1").is_err());
            assert!(RecordingReadGrantRequest::new(dataset, class, vec![a, a], "r1").is_ok());
        }
        for revision in ["".to_owned(), "x".repeat(129)] {
            assert!(
                RecordingReadGrantRequest::new(
                    dataset,
                    RecordingReadGrantClass::ViewerSegment,
                    vec![a],
                    revision
                )
                .is_err()
            );
        }
    }
}
