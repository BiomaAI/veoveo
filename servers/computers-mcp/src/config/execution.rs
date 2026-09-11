//! Installation-owned command authority and protected payload keys.
use super::{ConfigurationError, Result};
use crate::Templates;
use serde::Deserialize;
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};
use tokio::io::AsyncReadExt;
use uuid::Uuid;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_computers::{
    automation_grants::AutomationGrantPolicy,
    secrets::{ComputerKeyRing, ComputerSealingKey},
};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExecutionConfiguration {
    policy: AutomationGrantPolicy,
    artifact_endpoint: String,
    active_key_id: Uuid,
    keys: Vec<KeyFile>,
    template_fingerprints: Vec<String>,
    file_template_fingerprints: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyFile {
    id: Uuid,
    file: PathBuf,
}
#[derive(Clone)]
pub(crate) struct PreparedExecution {
    pub policy: AutomationGrantPolicy,
    pub keys: Arc<ComputerKeyRing>,
    pub artifacts: HttpArtifactPlane,
    pub templates: BTreeSet<String>,
    pub file_templates: BTreeSet<String>,
}
impl ExecutionConfiguration {
    pub async fn prepare(self, templates: &Templates) -> Result<PreparedExecution> {
        self.policy
            .validate()
            .map_err(|_| ConfigurationError::Execution)?;
        let endpoint =
            url::Url::parse(&self.artifact_endpoint).map_err(|_| ConfigurationError::Execution)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || endpoint.port_or_known_default() == Some(0)
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.path() != "/"
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(ConfigurationError::Execution);
        }
        let fingerprints: BTreeSet<_> = self.template_fingerprints.iter().cloned().collect();
        let file_fingerprints: BTreeSet<_> =
            self.file_template_fingerprints.iter().cloned().collect();
        let admitted: BTreeSet<_> = templates
            .runtimes()
            .iter()
            .map(|t| t.fingerprint())
            .collect();
        if fingerprints.is_empty()
            || fingerprints.len() > 64
            || fingerprints.len() != self.template_fingerprints.len()
            || !fingerprints.is_subset(&admitted)
            || templates
                .default()
                .is_none_or(|t| !fingerprints.contains(&t.runtime.fingerprint()))
            || !(1..=4).contains(&self.keys.len())
            || file_fingerprints.is_empty()
            || file_fingerprints.len() > 64
            || file_fingerprints.len() != self.file_template_fingerprints.len()
            || !file_fingerprints.is_subset(&fingerprints)
            || templates
                .default()
                .is_none_or(|t| !file_fingerprints.contains(&t.runtime.fingerprint()))
        {
            return Err(ConfigurationError::Execution);
        }
        let mut keys = Vec::with_capacity(self.keys.len());
        for source in self.keys {
            if !source.file.is_absolute() {
                return Err(ConfigurationError::ExecutionKey);
            }
            let file = tokio::fs::File::open(&source.file)
                .await
                .map_err(|_| ConfigurationError::ExecutionKey)?;
            let metadata = file
                .metadata()
                .await
                .map_err(|_| ConfigurationError::ExecutionKey)?;
            if !metadata.is_file() || metadata.len() != 32 {
                return Err(ConfigurationError::ExecutionKey);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Kubernetes may mount root-owned keys readable by the worker's
                // fsGroup. Other users and group writers receive no access.
                if metadata.permissions().mode() & 0o027 != 0 {
                    return Err(ConfigurationError::ExecutionKey);
                }
            }
            let mut bytes = Zeroizing::new(Vec::with_capacity(33));
            file.take(33)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| ConfigurationError::ExecutionKey)?;
            if bytes.len() != 32 {
                return Err(ConfigurationError::ExecutionKey);
            }
            let mut key = Zeroizing::new([0; 32]);
            key.copy_from_slice(&bytes);
            keys.push(
                ComputerSealingKey::new(source.id, key)
                    .map_err(|_| ConfigurationError::ExecutionKey)?,
            );
        }
        let keys = ComputerKeyRing::new(self.active_key_id, keys)
            .map_err(|_| ConfigurationError::ExecutionKey)?;
        Ok(PreparedExecution {
            policy: self.policy,
            keys: Arc::new(keys),
            artifacts: HttpArtifactPlane::new(endpoint.as_str().trim_end_matches('/')),
            templates: fingerprints,
            file_templates: file_fingerprints,
        })
    }
}
