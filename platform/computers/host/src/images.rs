//! Admit the unchanged official companion image before provider startup.
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::time::Duration;
use veoveo_types::Sha256Digest;

const MAX_INSPECTION_BYTES: usize = 64 * 1024;
const SOURCE: &[u8] = include_bytes!("../../../runtimes/computers/provider/manifest.json");

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Inspection {
    pub(crate) id: Sha256Digest,
    repo_digests: Vec<String>,
}
#[derive(Deserialize)]
struct ProviderProfile {
    images: ProfileImages,
}
#[derive(Deserialize)]
struct ProfileImages {
    supervisor: ProfileImage,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileImage {
    manifest_digest: Sha256Digest,
    config_digest: Sha256Digest,
}
impl Inspection {
    pub(crate) fn admit(&self, reference: &str, authority: &str) -> Result<Sha256Digest> {
        let profile: ProviderProfile = serde_json::from_slice(SOURCE)?;
        let Some((repository, digest)) = reference.split_once('@') else {
            anyhow::bail!("digest-pinned supervisor image required")
        };
        ensure!(
            repository
                .split_once('/')
                .is_some_and(|(registry, name)| registry == authority && !name.is_empty()),
            "supervisor image must use the Computer installation registry"
        );
        ensure!(
            Sha256Digest::parse(digest)? == profile.images.supervisor.manifest_digest,
            "supervisor requires the unchanged official release manifest digest"
        );
        ensure!(
            self.repo_digests.iter().any(|value| value == reference),
            "supervisor image digest is not locally admitted"
        );
        ensure!(
            self.id == profile.images.supervisor.config_digest,
            "supervisor image config differs from the official release"
        );
        Ok(self.id.clone())
    }
}
pub(crate) fn decode(bytes: &[u8]) -> Result<Inspection> {
    ensure!(
        bytes.len() <= MAX_INSPECTION_BYTES,
        "private supervisor inspection exceeds 64 KiB"
    );
    serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("private supervisor image inspection has invalid metadata"))
}
pub(crate) fn inspection_url(reference: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse("http://localhost/v1.53/images/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("private image API path is unavailable"))?
        .pop_if_empty()
        .push(reference)
        .push("json");
    Ok(url)
}
#[allow(dead_code)] // The native fixture reuses pure admission with its owned Docker inspector.
pub(crate) async fn inspect(
    client: &reqwest::Client,
    reference: &str,
    authority: &str,
) -> Result<Sha256Digest> {
    let mut response = client
        .get(inspection_url(reference)?)
        .timeout(Duration::from_secs(2))
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "private supervisor image inventory unavailable after preload"
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_INSPECTION_BYTES as u64),
        "private supervisor inspection exceeds 64 KiB"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            chunk.len() <= MAX_INSPECTION_BYTES - bytes.len(),
            "private supervisor inspection exceeds 64 KiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    decode(&bytes)?.admit(reference, authority)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn source_image() -> (String, serde_json::Value) {
        let profile: ProviderProfile = serde_json::from_slice(SOURCE).unwrap();
        let reference = format!(
            "registry.internal/provider@{}",
            profile.images.supervisor.manifest_digest
        );
        let value = serde_json::json!({"Id":profile.images.supervisor.config_digest,
            "RepoDigests":[reference], "Config":{"Labels":null}});
        (reference, value)
    }
    #[test]
    fn official_image_admits_unchanged_mirror_without_private_labels() {
        let (reference, value) = source_image();
        let image = decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        image.admit(&reference, "registry.internal").unwrap();
        assert_ne!(image.id.to_string(), reference.split_once('@').unwrap().1);
    }
    #[test]
    fn image_admission_rejects_foreign_missing_and_changed_images() {
        let (reference, value) = source_image();
        let image = decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(image.admit(&reference, "foreign.internal").is_err());
        assert!(
            image
                .admit("registry.internal/provider:latest", "registry.internal")
                .is_err()
        );
        for key in ["Id", "RepoDigests"] {
            let mut changed = value.clone();
            changed[key] = if key == "Id" {
                serde_json::json!(format!("sha256:{}", "a".repeat(64)))
            } else {
                serde_json::json!([])
            };
            assert!(
                decode(&serde_json::to_vec(&changed).unwrap())
                    .unwrap()
                    .admit(&reference, "registry.internal")
                    .is_err()
            );
        }
        let changed = format!("registry.internal/provider@sha256:{}", "a".repeat(64));
        assert!(image.admit(&changed, "registry.internal").is_err());
    }
    #[test]
    fn inspection_bounds_redact_errors_and_encode_reference() {
        assert!(decode(&vec![b' '; MAX_INSPECTION_BYTES + 1]).is_err());
        let message = decode(b"{\"Id\":\"SECRET-fixture\"}")
            .err()
            .unwrap()
            .to_string();
        assert!(!message.contains("SECRET"));
        let (reference, _) = source_image();
        let url = inspection_url(&reference).unwrap();
        assert_eq!(url.host_str(), Some("localhost"));
        assert!(url.path().contains("registry.internal%2Fprovider@sha256:"));
        assert!(url.path().ends_with("/json"));
    }
}
