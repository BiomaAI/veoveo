//! Reject obsolete deployment formats before interpreting their removed fields.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{DEPLOYMENT_LOCK_SCHEMA, DeploymentLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    schema_version: String,
}

impl DeploymentLock {
    /// Decode the current lock format and validate its complete contents.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let header: Header =
            serde_json::from_slice(bytes).context("decoding deployment lock header")?;
        ensure!(
            header.schema_version == DEPLOYMENT_LOCK_SCHEMA,
            "deployment lock schemaVersion must be {DEPLOYMENT_LOCK_SCHEMA}; regenerate the profile and lock from the fork checkout, remove extension release metadata, and use repository-local workloads"
        );
        let lock: Self = serde_json::from_slice(bytes).context("decoding deployment lock")?;
        lock.validate()?;
        Ok(lock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsolete_lock_rejects_with_regeneration_guidance_before_removed_fields() {
        let old = br#"{"schemaVersion":"veoveo.io/deployment-lock/v7","sources":[{"role":"extension","extensionRelease":{}}]}"#;
        let error = DeploymentLock::decode(old).unwrap_err().to_string();
        assert!(error.contains(DEPLOYMENT_LOCK_SCHEMA));
        assert!(error.contains("regenerate"));
    }

    #[test]
    fn current_lock_decodes_and_validates() {
        DeploymentLock::decode(include_bytes!("../tests/fixtures/deployment-lock.json")).unwrap();
    }
}
