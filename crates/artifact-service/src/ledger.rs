//! The grant ledger: durable artifact metadata + access-control list.
//!
//! Postgres is the authoritative store (matching the gateway control plane).
//! The [`GrantLedger`] trait keeps the service testable with an in-memory double.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde_json::Value;
use veoveo_mcp_contract::access::{AccessLevel, ArtifactSha256, Grant, Subject};
use veoveo_mcp_contract::gateway::{DataLabelId, GroupId, PrincipalId, TenantId};
use veoveo_mcp_contract::storage::{ArtifactMetadata, ComplianceMetadata};

/// An artifact's durable record: metadata, its tenant partition, the labels MAC
/// is evaluated against, and its grants (the ACL).
#[derive(Debug, Clone)]
pub struct StoredArtifact {
    pub metadata: ArtifactMetadata,
    pub tenant: TenantId,
    pub labels: BTreeSet<DataLabelId>,
    pub grants: Vec<Grant>,
}

#[derive(Debug)]
pub enum LedgerError {
    Conflict(String),
    Backend(String),
    Corrupt(String),
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(m) => write!(f, "ledger conflict: {m}"),
            Self::Backend(m) => write!(f, "ledger backend error: {m}"),
            Self::Corrupt(m) => write!(f, "ledger row is corrupt: {m}"),
        }
    }
}

impl std::error::Error for LedgerError {}

/// Durable metadata + grant ledger operations.
pub trait GrantLedger: Send + Sync {
    /// Insert a new artifact and its initial (owner) grants atomically. If the
    /// sha already exists in the tenant, this is a no-op success (dedup).
    fn insert_artifact(
        &self,
        stored: StoredArtifact,
    ) -> impl std::future::Future<Output = Result<(), LedgerError>> + Send;

    fn get_artifact(
        &self,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<Option<StoredArtifact>, LedgerError>> + Send;

    /// Add or raise a grant (upsert on `(sha, subject)`).
    fn upsert_grant(
        &self,
        grant: Grant,
    ) -> impl std::future::Future<Output = Result<(), LedgerError>> + Send;

    fn remove_grant(
        &self,
        sha: &ArtifactSha256,
        subject: &Subject,
    ) -> impl std::future::Future<Output = Result<(), LedgerError>> + Send;

    fn list_grants(
        &self,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<Vec<Grant>, LedgerError>> + Send;

    fn delete_artifact(
        &self,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<(), LedgerError>> + Send;
}

// ---- shared (de)serialization between domain types and columns ----

pub(crate) fn level_str(level: AccessLevel) -> &'static str {
    match level {
        AccessLevel::Read => "read",
        AccessLevel::Write => "write",
        AccessLevel::Admin => "admin",
    }
}

pub(crate) fn parse_level(s: &str) -> Result<AccessLevel, LedgerError> {
    match s {
        "read" => Ok(AccessLevel::Read),
        "write" => Ok(AccessLevel::Write),
        "admin" => Ok(AccessLevel::Admin),
        other => Err(LedgerError::Corrupt(format!("unknown access level `{other}`"))),
    }
}

pub(crate) fn subject_parts(subject: &Subject) -> (&'static str, String) {
    match subject {
        Subject::User(id) => ("user", id.as_str().to_string()),
        Subject::Group(id) => ("group", id.as_str().to_string()),
    }
}

pub(crate) fn parse_subject(kind: &str, id: &str) -> Result<Subject, LedgerError> {
    match kind {
        "user" => Ok(Subject::User(
            PrincipalId::new(id).map_err(|e| LedgerError::Corrupt(e.to_string()))?,
        )),
        "group" => Ok(Subject::Group(
            GroupId::new(id).map_err(|e| LedgerError::Corrupt(e.to_string()))?,
        )),
        other => Err(LedgerError::Corrupt(format!("unknown subject kind `{other}`"))),
    }
}

pub(crate) fn labels_to_json(labels: &BTreeSet<DataLabelId>) -> Value {
    Value::Array(
        labels
            .iter()
            .map(|l| Value::String(l.as_str().to_string()))
            .collect(),
    )
}

pub(crate) fn labels_from_json(value: &Value) -> Result<BTreeSet<DataLabelId>, LedgerError> {
    let arr = value
        .as_array()
        .ok_or_else(|| LedgerError::Corrupt("data_labels is not an array".to_string()))?;
    arr.iter()
        .map(|v| {
            let s = v
                .as_str()
                .ok_or_else(|| LedgerError::Corrupt("data label is not a string".to_string()))?;
            DataLabelId::new(s).map_err(|e| LedgerError::Corrupt(e.to_string()))
        })
        .collect()
}

/// Rebuild an [`ArtifactMetadata`] from stored columns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild_metadata(
    sha: &ArtifactSha256,
    byte_len: i64,
    mime_type: Option<String>,
    filename: Option<String>,
    classification: Option<DataLabelId>,
    tenant: &TenantId,
    owner: &PrincipalId,
    labels: &BTreeSet<DataLabelId>,
    retention_expires_at: Option<DateTime<Utc>>,
    metadata: Value,
    created_at: DateTime<Utc>,
) -> ArtifactMetadata {
    ArtifactMetadata {
        sha256: sha.as_str().to_string(),
        byte_len: byte_len as u64,
        mime_type,
        filename,
        artifact_uri: sha.plane_uri(),
        download_url: None,
        created_at,
        compliance: ComplianceMetadata {
            classification,
            tenant_id: Some(tenant.clone()),
            owner_id: Some(owner.clone()),
            data_labels: labels.clone(),
            retention_expires_at,
        },
        metadata,
    }
}

pub mod postgres;

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct InMemoryLedger {
        artifacts: Mutex<HashMap<String, StoredArtifact>>,
    }

    impl GrantLedger for InMemoryLedger {
        async fn insert_artifact(&self, stored: StoredArtifact) -> Result<(), LedgerError> {
            let mut map = self.artifacts.lock().unwrap();
            map.entry(stored.metadata.sha256.clone()).or_insert(stored);
            Ok(())
        }

        async fn get_artifact(
            &self,
            sha: &ArtifactSha256,
        ) -> Result<Option<StoredArtifact>, LedgerError> {
            Ok(self.artifacts.lock().unwrap().get(sha.as_str()).cloned())
        }

        async fn upsert_grant(&self, grant: Grant) -> Result<(), LedgerError> {
            let mut map = self.artifacts.lock().unwrap();
            let stored = map
                .get_mut(grant.artifact.as_str())
                .ok_or_else(|| LedgerError::Conflict("no such artifact".to_string()))?;
            stored.grants.retain(|g| g.subject != grant.subject);
            stored.grants.push(grant);
            Ok(())
        }

        async fn remove_grant(
            &self,
            sha: &ArtifactSha256,
            subject: &Subject,
        ) -> Result<(), LedgerError> {
            let mut map = self.artifacts.lock().unwrap();
            if let Some(stored) = map.get_mut(sha.as_str()) {
                stored.grants.retain(|g| &g.subject != subject);
            }
            Ok(())
        }

        async fn list_grants(&self, sha: &ArtifactSha256) -> Result<Vec<Grant>, LedgerError> {
            Ok(self
                .artifacts
                .lock()
                .unwrap()
                .get(sha.as_str())
                .map(|s| s.grants.clone())
                .unwrap_or_default())
        }

        async fn delete_artifact(&self, sha: &ArtifactSha256) -> Result<(), LedgerError> {
            self.artifacts.lock().unwrap().remove(sha.as_str());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_round_trips() {
        for l in [AccessLevel::Read, AccessLevel::Write, AccessLevel::Admin] {
            assert_eq!(parse_level(level_str(l)).unwrap(), l);
        }
        assert!(parse_level("root").is_err());
    }

    #[test]
    fn subject_round_trips() {
        let u = Subject::User(PrincipalId::new("alice").unwrap());
        let (k, id) = subject_parts(&u);
        assert_eq!(parse_subject(k, &id).unwrap(), u);
        let g = Subject::Group(GroupId::new("eng").unwrap());
        let (k, id) = subject_parts(&g);
        assert_eq!(parse_subject(k, &id).unwrap(), g);
    }

    #[test]
    fn labels_round_trip_through_json() {
        let labels: BTreeSet<_> = [
            DataLabelId::new("cui").unwrap(),
            DataLabelId::new("us_only").unwrap(),
        ]
        .into_iter()
        .collect();
        let json = labels_to_json(&labels);
        assert_eq!(labels_from_json(&json).unwrap(), labels);
    }
}
