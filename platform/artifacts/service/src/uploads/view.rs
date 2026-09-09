//! Checked, bounded public projections never contain object keys or provider IDs.

use super::*;
use std::num::NonZeroU64;

pub(super) fn uuid(id: &platform::RecordId) -> Result<uuid::Uuid, UploadFault> {
    match &id.key {
        platform::RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(UploadFault::unavailable()),
    }
}
fn bytes(value: i64) -> Result<u64, UploadFault> {
    u64::try_from(value)
        .ok()
        .filter(|n| *n <= contract::MAX_UPLOAD_BYTES)
        .ok_or_else(UploadFault::unavailable)
}
fn count(value: i64) -> Result<u32, UploadFault> {
    u32::try_from(value).map_err(|_| UploadFault::unavailable())
}
fn sha(value: String) -> Result<contract::UploadSha256, UploadFault> {
    contract::UploadSha256::parse(value).map_err(|_| UploadFault::unavailable())
}

pub(super) fn layout(
    row: &platform::ArtifactUploadLayout,
) -> Result<contract::UploadLayout, UploadFault> {
    Ok(contract::UploadLayout {
        part_bytes: NonZeroU64::new(bytes(row.part_bytes)?).ok_or_else(UploadFault::unavailable)?,
        max_parts: NonZeroU32::new(count(row.max_parts)?).ok_or_else(UploadFault::unavailable)?,
        max_total_bytes: NonZeroU64::new(bytes(row.max_total_bytes)?)
            .ok_or_else(UploadFault::unavailable)?,
        parallel_parts: NonZeroU32::new(count(row.parallel_parts)?)
            .ok_or_else(UploadFault::unavailable)?,
    })
}
pub(super) fn part(
    row: &platform::ArtifactUploadPartRecord,
) -> Result<contract::UploadPartReceipt, UploadFault> {
    if row.state != platform::ArtifactUploadPartState::Accepted {
        return Err(UploadFault::unavailable());
    }
    Ok(contract::UploadPartReceipt {
        part_number: NonZeroU32::new(count(row.part_number)?)
            .ok_or_else(UploadFault::unavailable)?,
        byte_len: bytes(row.byte_len)?,
        sha256: sha(row.sha256.clone())?,
    })
}

pub(super) fn session(
    row: platform::ArtifactUploadRecord,
    mut parts: Vec<platform::ArtifactUploadPartRecord>,
) -> Result<contract::ArtifactUploadSession, UploadFault> {
    let more = parts.len() > contract::UPLOAD_PART_PAGE_LIMIT;
    parts.truncate(contract::UPLOAD_PART_PAGE_LIMIT);
    let parts = parts.iter().map(part).collect::<Result<Vec<_>, _>>()?;
    let upload_id = contract::ArtifactUploadId::parse(uuid(&row.id)?.to_string())
        .map_err(|_| UploadFault::unavailable())?;
    let state = match row.state {
        platform::ArtifactUploadState::Open => contract::ArtifactUploadState::Open,
        platform::ArtifactUploadState::Finalizing => contract::ArtifactUploadState::Finalizing,
        platform::ArtifactUploadState::Verifying => contract::ArtifactUploadState::Verifying,
        platform::ArtifactUploadState::Completed => contract::ArtifactUploadState::Completed,
        platform::ArtifactUploadState::Cancelled => contract::ArtifactUploadState::Cancelled,
        platform::ArtifactUploadState::Expired => contract::ArtifactUploadState::Expired,
        platform::ArtifactUploadState::Failed => contract::ArtifactUploadState::Failed,
    };
    let receipt = if state == contract::ArtifactUploadState::Completed {
        let artifact_id = contract::ArtifactId::parse(uuid(&row.artifact)?.to_string())
            .map_err(|_| UploadFault::unavailable())?;
        Some(contract::ArtifactUploadReceipt {
            upload_id,
            artifact_id,
            artifact_uri: artifact_id.plane_uri(),
            sha256: sha(row.verified_sha256.ok_or_else(UploadFault::unavailable)?)?,
            byte_len: bytes(row.manifest.ok_or_else(UploadFault::unavailable)?.byte_len)?,
            mime_type: row.descriptor.mime_type.clone(),
            filename: row.descriptor.filename.clone(),
            created_at: row.completed_at.ok_or_else(UploadFault::unavailable)?,
        })
    } else {
        None
    };
    Ok(contract::ArtifactUploadSession {
        upload_id,
        state,
        layout: layout(&row.layout)?,
        descriptor: contract::CreateArtifactUpload {
            filename: row.descriptor.filename,
            mime_type: row.descriptor.mime_type,
            byte_len: row.descriptor.byte_len.map(bytes).transpose()?,
            sha256: row.descriptor.sha256.map(sha).transpose()?,
        },
        accepted_bytes: bytes(row.accepted_bytes)?,
        accepted_part_count: count(row.accepted_part_count)?,
        next_part_cursor: if more {
            parts.last().map(|p| p.part_number)
        } else {
            None
        },
        parts,
        created_at: row.created_at,
        expires_at: row.expires_at,
        receipt,
        failure: row.failure.map(|failure| match failure {
            platform::ArtifactUploadFailure::Integrity => contract::UploadErrorCode::Integrity,
            platform::ArtifactUploadFailure::AuthorityChanged
            | platform::ArtifactUploadFailure::PolicyChanged => contract::UploadErrorCode::Denied,
            platform::ArtifactUploadFailure::Storage => contract::UploadErrorCode::Unavailable,
            platform::ArtifactUploadFailure::Expired => contract::UploadErrorCode::Expired,
        }),
    })
}
