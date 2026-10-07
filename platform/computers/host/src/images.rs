//! Admit the exact companion image before the private provider can start.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use veoveo_types::Sha256Digest;

const MAX_INSPECTION_BYTES: usize = 64 * 1024;
const SOURCE: &[u8] = include_bytes!("../../../runtimes/computers/provider-patches/manifest.json");

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Inspection {
    pub(crate) id: Sha256Digest,
    repo_digests: Vec<String>,
    config: ImageConfig,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageConfig {
    labels: SourceLabels,
}
#[derive(Default, Deserialize)]
struct SourceLabels {
    #[serde(default, rename = "ai.veoveo.provider.profile")]
    profile: Option<String>,
    #[serde(
        default,
        rename = "ai.veoveo.provider.manifest-sha256",
        with = "veoveo_types::sha256_hex::optional"
    )]
    manifest_sha256: Option<Sha256Digest>,
    #[serde(default, rename = "ai.veoveo.provider.supervisor-source-tree")]
    supervisor_source_tree: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfile {
    gateway_version: String,
    binaries: Vec<ProfileBinary>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBinary {
    name: BinaryName,
    version: String,
    source_tree: String,
    target: String,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum BinaryName {
    #[vocabulary(rename = "openshell")]
    Cli,
    #[vocabulary(rename = "openshell-gateway")]
    Gateway,
    #[vocabulary(rename = "openshell-driver-docker")]
    Driver,
    #[vocabulary(rename = "openshell-supervisor")]
    Supervisor,
    #[vocabulary(rename = "openshell-sandbox")]
    Sandbox,
}
impl Inspection {
    pub(crate) fn admit(&self, reference: &str, authority: &str) -> Result<Sha256Digest> {
        let (repository, digest) = reference
            .split_once("@sha256:")
            .context("digest-pinned supervisor image required")?;
        Sha256Digest::from_hex(digest)?;
        ensure!(
            repository
                .split_once('/')
                .is_some_and(|(registry, name)| registry == authority && !name.is_empty()),
            "supervisor image must use the Computer installation registry"
        );
        ensure!(
            self.repo_digests.iter().any(|value| value == reference),
            "supervisor image digest is not locally admitted"
        );
        let profile: ProviderProfile = serde_json::from_slice(SOURCE)?;
        let supervisors: Vec<_> = profile
            .binaries
            .iter()
            .filter(|binary| binary.name == BinaryName::Supervisor)
            .collect();
        let sandboxes: Vec<_> = profile
            .binaries
            .iter()
            .filter(|binary| binary.name == BinaryName::Sandbox)
            .collect();
        ensure!(
            supervisors.len() == 1 && sandboxes.len() == 1,
            "matched supervisor and sandbox source declarations required"
        );
        let supervisor = supervisors[0];
        let sandbox = sandboxes[0];
        ensure!(
            !profile.gateway_version.is_empty()
                && supervisor.version == profile.gateway_version
                && sandbox.version == profile.gateway_version
                && supervisor.source_tree.len() == 40
                && supervisor
                    .source_tree
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                && supervisor.source_tree == sandbox.source_tree
                && supervisor.target == "x86_64-unknown-linux-gnu"
                && sandbox.target == "x86_64-unknown-linux-musl",
            "matched GNU supervisor and static musl sandbox profile required"
        );
        let digest = Sha256Digest::from_bytes(Sha256::digest(SOURCE).into());
        let labels = &self.config.labels;
        ensure!(
            labels.profile.as_ref() == Some(&profile.gateway_version)
                && labels.manifest_sha256 == Some(digest)
                && labels.supervisor_source_tree.as_ref() == Some(&supervisor.source_tree),
            "supervisor image differs from the compiled provider source profile"
        );
        // Docker's local config/image ID is distinct from the registry manifest digest.
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
        let tree = profile
            .binaries
            .iter()
            .find(|binary| binary.name == BinaryName::Supervisor)
            .unwrap()
            .source_tree
            .clone();
        let manifest = Sha256Digest::from_bytes(Sha256::digest(SOURCE).into());
        let reference = format!("registry.internal/provider@sha256:{}", "a".repeat(64));
        let value = serde_json::json!({"Id":format!("sha256:{}", "b".repeat(64)),"RepoDigests":[reference],"Config":{"Labels":{
            "ai.veoveo.provider.profile":profile.gateway_version,
            "ai.veoveo.provider.manifest-sha256":manifest.hex(),
            "ai.veoveo.provider.supervisor-source-tree":tree
        }}});
        (reference, value)
    }
    #[test]
    fn exact_source_image_admits_distinct_local_id_and_registry_manifest_digest() {
        let (reference, value) = source_image();
        let image = decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        let id = image.admit(&reference, "registry.internal").unwrap();
        assert_eq!(id.hex(), "b".repeat(64));
        assert_ne!(id.hex(), reference.split_once("@sha256:").unwrap().1);
    }
    #[test]
    fn image_admission_rejects_foreign_missing_and_unpatched_source() {
        let (reference, original) = source_image();
        let image = decode(&serde_json::to_vec(&original).unwrap()).unwrap();
        assert!(image.admit(&reference, "foreign.internal").is_err());
        assert!(
            image
                .admit("registry.internal/provider:latest", "registry.internal")
                .is_err()
        );
        let mut foreign = original.clone();
        foreign["RepoDigests"] = serde_json::json!([]);
        assert!(
            decode(&serde_json::to_vec(&foreign).unwrap())
                .unwrap()
                .admit(&reference, "registry.internal")
                .is_err()
        );
        for key in [
            "ai.veoveo.provider.profile",
            "ai.veoveo.provider.manifest-sha256",
            "ai.veoveo.provider.supervisor-source-tree",
        ] {
            for missing in [false, true] {
                let mut bad = original.clone();
                if missing {
                    bad["Config"]["Labels"].as_object_mut().unwrap().remove(key);
                } else {
                    bad["Config"]["Labels"][key] = serde_json::json!("unpatched-or-foreign-source");
                }
                assert!(
                    decode(&serde_json::to_vec(&bad).unwrap())
                        .and_then(|image| image.admit(&reference, "registry.internal"))
                        .is_err()
                );
            }
        }
    }
    #[test]
    fn inspection_bounds_and_errors_do_not_expose_body_and_url_encodes_reference() {
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
