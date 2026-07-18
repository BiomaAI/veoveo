use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use veoveo_mcp_contract::{
    DataLabelId, GatewayInternalIdentity, PrincipalId, PrincipalKind, TenantId, TokenIssuer,
    TokenSubject,
};
use veoveo_platform_store::{
    PrincipalKind as StorePrincipalKind, RecordingId, RecordingState, SegmentId, SegmentState,
};

use super::{MAX_SEGMENTS, RecordingService, labels_visible, record_uuid};

/// Stable identity and clearance needed to reopen an authorized recording.
///
/// Unlike a gateway assertion this value has no bearer or expiry. It can be
/// reconstructed from a durable task owner after restart, while the recording
/// policy is evaluated again against current catalog state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReadAuthority {
    principal_id: PrincipalId,
    principal_kind: PrincipalKind,
    issuer: TokenIssuer,
    subject: TokenSubject,
    tenant: Option<TenantId>,
    data_labels: BTreeSet<DataLabelId>,
}

impl RecordingReadAuthority {
    pub fn from_gateway(identity: &GatewayInternalIdentity) -> Self {
        Self {
            principal_id: identity.principal.id.clone(),
            principal_kind: identity.principal.kind,
            issuer: identity.principal.issuer.clone(),
            subject: identity.principal.subject.clone(),
            tenant: identity.principal.tenant.clone(),
            data_labels: identity.principal.data_labels.clone(),
        }
    }

    pub fn new(
        principal_id: PrincipalId,
        principal_kind: PrincipalKind,
        issuer: TokenIssuer,
        subject: TokenSubject,
        tenant: Option<TenantId>,
        data_labels: BTreeSet<DataLabelId>,
    ) -> Self {
        Self {
            principal_id,
            principal_kind,
            issuer,
            subject,
            tenant,
            data_labels,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReadSegment {
    pub segment_id: SegmentId,
    pub ordinal: i64,
    pub state: SegmentState,
    pub byte_len: u64,
    pub sha256: Option<String>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReadPlan {
    pub recording_id: RecordingId,
    pub dataset: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub classification: String,
    pub labels: Vec<String>,
    pub segments: Vec<RecordingReadSegment>,
}

impl RecordingReadPlan {
    /// Physical segment paths that are immutable enough for a deterministic
    /// analysis task. A writing segment is deliberately never projected here.
    pub fn stable_segment_paths(&self) -> Vec<PathBuf> {
        self.segments
            .iter()
            .filter(|segment| matches!(segment.state, SegmentState::Frozen | SegmentState::Sealed))
            .map(|segment| segment.path.clone())
            .collect()
    }
}

impl RecordingService {
    /// Resolve one governed recording into a local, typed read plan.
    ///
    /// Callers persist the recording identity, not these filesystem paths, and
    /// call this method again when a resumable task is reclaimed.
    pub async fn read_plan(
        &self,
        authority: &RecordingReadAuthority,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingReadPlan>> {
        let tenant_key = authority
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        let platform_identity = self
            .store
            .ensure_identity(
                &tenant_key,
                authority.principal_id.as_str(),
                authority.issuer.as_str(),
                authority.subject.as_str(),
                match authority.principal_kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        let Some(recording) = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?
        else {
            return Ok(None);
        };
        if !labels_visible(
            &recording,
            authority.data_labels.iter().map(|label| label.as_str()),
        ) {
            return Ok(None);
        }
        let segments = self
            .store
            .recording_segments(platform_identity.tenant_id, recording_id, MAX_SEGMENTS)
            .await?
            .into_iter()
            .map(|segment| {
                ensure!(
                    segment.byte_len >= 0,
                    "recording segment has negative byte_len"
                );
                Ok(RecordingReadSegment {
                    segment_id: SegmentId::from_uuid(record_uuid(&segment.id, "segment")?),
                    ordinal: segment.ordinal,
                    state: segment.state,
                    byte_len: u64::try_from(segment.byte_len)
                        .context("recording segment byte_len exceeds u64")?,
                    sha256: segment.sha256,
                    path: match segment.state {
                        SegmentState::Writing => self.live_segment_path(&segment.relative_path)?,
                        SegmentState::Frozen | SegmentState::Sealed => {
                            self.segment_path(&segment.relative_path)?
                        }
                        SegmentState::Failed => {
                            super::confined_segment_path(&self.spool_root, &segment.relative_path)?
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(RecordingReadPlan {
            recording_id,
            dataset: recording.dataset,
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: recording.state,
            classification: recording.classification,
            labels: recording.labels,
            segments,
        }))
    }
}
