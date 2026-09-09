//! Explicit installation policy and checked multipart layout negotiation.

use super::*;

/// Absence of a configured policy disables public uploads. There is no Default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUploadPolicy {
    pub max_object_bytes: NonZeroU64,
    pub tenant_quota_bytes: NonZeroU64,
    pub max_active_uploads_per_tenant: NonZeroU32,
    pub part_bytes: NonZeroU64,
    pub max_part_bytes: NonZeroU64,
    pub max_parts: NonZeroU32,
    pub parallel_parts: NonZeroU32,
    pub max_inflight_bytes: NonZeroU64,
    pub inactivity_seconds: NonZeroU64,
    pub lifetime_seconds: NonZeroU64,
    pub part_timeout_seconds: NonZeroU64,
    pub allowed_mime_types: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadLayout {
    pub part_bytes: NonZeroU64,
    pub max_parts: NonZeroU32,
    pub max_total_bytes: NonZeroU64,
    pub parallel_parts: NonZeroU32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveArtifactUploadPolicy {
    pub allowed: bool,
    pub explanation: String,
    pub actor: PrincipalId,
    pub work_context: WorkContextId,
    pub destination_name: String,
    pub access_description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ArtifactUploadPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
}

impl ArtifactUploadPolicy {
    pub fn validate(&self) -> Result<(), UploadErrorCode> {
        // S3 profile: 5 MiB minimum except the final part, at most 10,000 parts.
        if self.part_bytes.get() < 5 * 1024 * 1024
            || self.max_part_bytes < self.part_bytes
            || self.max_part_bytes.get() > 5 * 1024 * 1024 * 1024
            || self.max_parts.get() > 10_000
            || self.max_object_bytes.get() > MAX_UPLOAD_BYTES
            || self.tenant_quota_bytes.get() > MAX_UPLOAD_BYTES
            || self.max_object_bytes > self.tenant_quota_bytes
            || self.max_inflight_bytes.get() > MAX_UPLOAD_BYTES
            || self.max_inflight_bytes < self.max_part_bytes
            || self.lifetime_seconds.get() > 365 * 24 * 60 * 60
            || self.inactivity_seconds > self.lifetime_seconds
            || self.part_timeout_seconds > self.inactivity_seconds
            || self.part_timeout_seconds.get() > 3600
            || self.allowed_mime_types.is_empty()
            || self
                .allowed_mime_types
                .iter()
                .any(|mime| !valid_upload_mime_type(mime))
            || self
                .max_part_bytes
                .get()
                .checked_mul(u64::from(self.max_parts.get()))
                .is_none_or(|capacity| capacity < self.max_object_bytes.get())
        {
            return Err(UploadErrorCode::Malformed);
        }
        Ok(())
    }

    pub fn admit(
        &self,
        descriptor: &CreateArtifactUpload,
    ) -> Result<UploadLayout, UploadErrorCode> {
        self.validate()?;
        descriptor.validate()?;
        if !self.allowed_mime_types.contains(&descriptor.mime_type) {
            return Err(UploadErrorCode::UnsupportedType);
        }
        let total = descriptor.byte_len.unwrap_or(self.max_object_bytes.get());
        if total > self.max_object_bytes.get() {
            return Err(UploadErrorCode::TooLarge);
        }
        let part_bytes = self
            .part_bytes
            .get()
            .max(total.div_ceil(u64::from(self.max_parts.get())));
        let parallel_parts =
            u64::from(self.parallel_parts.get()).min(self.max_inflight_bytes.get() / part_bytes);
        Ok(UploadLayout {
            part_bytes: NonZeroU64::new(part_bytes).ok_or(UploadErrorCode::Malformed)?,
            max_parts: self.max_parts,
            max_total_bytes: NonZeroU64::new(total.max(1)).ok_or(UploadErrorCode::Malformed)?,
            parallel_parts: NonZeroU32::new(
                u32::try_from(parallel_parts).map_err(|_| UploadErrorCode::Malformed)?,
            )
            .ok_or(UploadErrorCode::Malformed)?,
        })
    }
}

impl UploadLayout {
    pub fn part_offset(&self, number: NonZeroU32) -> Result<u64, UploadErrorCode> {
        if number > self.max_parts {
            return Err(UploadErrorCode::TooLarge);
        }
        let offset = u64::from(number.get() - 1)
            .checked_mul(self.part_bytes.get())
            .ok_or(UploadErrorCode::TooLarge)?;
        if offset >= self.max_total_bytes.get() {
            return Err(UploadErrorCode::TooLarge);
        }
        Ok(offset)
    }

    pub fn validate_part(&self, number: NonZeroU32, byte_len: u64) -> Result<(), UploadErrorCode> {
        let offset = self.part_offset(number)?;
        if byte_len > self.part_bytes.get()
            || offset
                .checked_add(byte_len)
                .is_none_or(|end| end > self.max_total_bytes.get())
        {
            return Err(UploadErrorCode::TooLarge);
        }
        Ok(())
    }

    /// Accepted parts must be sorted by number before manifest sealing.
    pub fn validate_manifest(
        &self,
        manifest: &CompleteArtifactUpload,
        parts: &[UploadPartReceipt],
    ) -> Result<(), UploadErrorCode> {
        if manifest.byte_len > self.max_total_bytes.get()
            || manifest.part_count > self.max_parts
            || usize::try_from(manifest.part_count.get()).ok() != Some(parts.len())
        {
            return Err(UploadErrorCode::Conflict);
        }
        let mut total = 0_u64;
        for (index, part) in parts.iter().enumerate() {
            if usize::try_from(part.part_number.get()).ok() != Some(index + 1)
                || part.byte_len > self.part_bytes.get()
                || (index + 1 < parts.len() && part.byte_len != self.part_bytes.get())
                || (part.byte_len == 0 && (parts.len() != 1 || manifest.byte_len != 0))
            {
                return Err(UploadErrorCode::Conflict);
            }
            total = total
                .checked_add(part.byte_len)
                .ok_or(UploadErrorCode::TooLarge)?;
        }
        if total != manifest.byte_len {
            return Err(UploadErrorCode::Conflict);
        }
        Ok(())
    }
}
