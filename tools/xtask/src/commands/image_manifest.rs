use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::process;

const OCI_IMAGE_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
const IN_TOTO_STATEMENT: &str = "application/vnd.in-toto+json";
const PREDICATE_ANNOTATION: &str = "in-toto.io/predicate-type";
const SPDX_PREDICATE: &str = "https://spdx.dev/Document";
const SLSA_PROVENANCE_PREDICATE: &str = "https://slsa.dev/provenance/v1";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PublishedImageDigests {
    pub(crate) runtime: String,
    pub(crate) publication: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ManifestRecord {
    descriptor: Descriptor,
    #[serde(default, rename = "OCIManifest")]
    oci_manifest: OciManifest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Descriptor {
    media_type: String,
    digest: String,
    platform: Platform,
}

#[derive(Debug, Deserialize)]
struct Platform {
    architecture: String,
    os: String,
}

#[derive(Debug, Default, Deserialize)]
struct OciManifest {
    #[serde(default)]
    layers: Vec<Layer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layer {
    media_type: String,
    #[serde(default)]
    annotations: std::collections::BTreeMap<String, String>,
}

pub(crate) fn inspect(
    repository: &str,
    publication_digest: &str,
    platform: &str,
    allow_insecure_registry: bool,
) -> Result<PublishedImageDigests> {
    let reference = format!("{repository}@{publication_digest}");
    let mut arguments = vec!["manifest", "inspect"];
    if allow_insecure_registry {
        arguments.push("--insecure");
    }
    arguments.extend(["--verbose", &reference]);
    let output = process::output("docker", arguments, None)
        .with_context(|| format!("inspecting immutable OCI publication {reference}"))?;
    parse(&output.stdout, publication_digest, platform, &reference)
}

fn parse(
    bytes: &[u8],
    publication_digest: &str,
    platform: &str,
    reference: &str,
) -> Result<PublishedImageDigests> {
    validate_digest(publication_digest, "publication")?;
    let (expected_os, expected_architecture) = platform
        .split_once('/')
        .context("expected OCI platform must use os/architecture")?;
    let records = serde_json::from_slice::<Vec<ManifestRecord>>(bytes)
        .with_context(|| format!("decoding OCI publication inspection for {reference}"))?;

    let runtime = records
        .iter()
        .filter(|record| {
            record.descriptor.platform.os == expected_os
                && record.descriptor.platform.architecture == expected_architecture
        })
        .collect::<Vec<_>>();
    ensure!(
        runtime.len() == 1,
        "OCI publication {reference} must contain exactly one {platform} runtime manifest, found {}",
        runtime.len()
    );
    let runtime = runtime[0];
    ensure!(
        runtime.descriptor.media_type == OCI_IMAGE_MANIFEST,
        "OCI publication {reference} runtime descriptor uses unsupported media type {}",
        runtime.descriptor.media_type
    );
    validate_digest(&runtime.descriptor.digest, "runtime")?;

    let attestations = records
        .iter()
        .filter(|record| {
            record.descriptor.platform.os == "unknown"
                && record.descriptor.platform.architecture == "unknown"
        })
        .collect::<Vec<_>>();
    ensure!(
        attestations.len() == 1,
        "OCI publication {reference} must contain exactly one BuildKit attestation manifest, found {}",
        attestations.len()
    );
    let attestation = attestations[0];
    ensure!(
        attestation.descriptor.media_type == OCI_IMAGE_MANIFEST,
        "OCI publication {reference} attestation descriptor uses unsupported media type {}",
        attestation.descriptor.media_type
    );
    let predicates = attestation
        .oci_manifest
        .layers
        .iter()
        .map(|layer| {
            ensure!(
                layer.media_type == IN_TOTO_STATEMENT,
                "OCI publication {reference} attestation layer uses unsupported media type {}",
                layer.media_type
            );
            layer
                .annotations
                .get(PREDICATE_ANNOTATION)
                .cloned()
                .with_context(|| {
                    format!(
                        "OCI publication {reference} attestation layer omits {PREDICATE_ANNOTATION}"
                    )
                })
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        predicates
            == BTreeSet::from([
                SPDX_PREDICATE.to_owned(),
                SLSA_PROVENANCE_PREDICATE.to_owned(),
            ]),
        "OCI publication {reference} must carry SPDX SBOM and SLSA provenance attestations"
    );
    ensure!(
        records.len() == 2,
        "OCI publication {reference} contains unexpected platform descriptors"
    );

    Ok(PublishedImageDigests {
        runtime: runtime.descriptor.digest.clone(),
        publication: publication_digest.to_owned(),
    })
}

fn validate_digest(digest: &str, kind: &str) -> Result<()> {
    let value = digest
        .strip_prefix("sha256:")
        .with_context(|| format!("{kind} OCI digest must start with sha256:"))?;
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{kind} OCI digest must contain 64 lowercase hexadecimal digits"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PublishedImageDigests, parse};

    const PUBLICATION: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const RUNTIME: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn inspection(attestation_layers: &str) -> Vec<u8> {
        format!(
            r#"[
              {{
                "Ref": "registry.example/image@{RUNTIME}",
                "Descriptor": {{
                  "mediaType": "application/vnd.oci.image.manifest.v1+json",
                  "digest": "{RUNTIME}",
                  "size": 100,
                  "platform": {{"architecture": "amd64", "os": "linux"}}
                }},
                "OCIManifest": {{
                  "layers": [
                    {{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip"}}
                  ]
                }}
              }},
              {{
                "Ref": "registry.example/image@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "Descriptor": {{
                  "mediaType": "application/vnd.oci.image.manifest.v1+json",
                  "digest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                  "size": 100,
                  "platform": {{"architecture": "unknown", "os": "unknown"}}
                }},
                "OCIManifest": {{"layers": [{attestation_layers}]}}
              }}
            ]"#
        )
        .into_bytes()
    }

    #[test]
    fn selects_runtime_manifest_and_retains_attested_publication() {
        let bytes = inspection(
            r#"{
              "mediaType": "application/vnd.in-toto+json",
              "annotations": {"in-toto.io/predicate-type": "https://spdx.dev/Document"}
            }, {
              "mediaType": "application/vnd.in-toto+json",
              "annotations": {"in-toto.io/predicate-type": "https://slsa.dev/provenance/v1"}
            }"#,
        );

        let actual = parse(
            &bytes,
            PUBLICATION,
            "linux/amd64",
            "registry.example/image@publication",
        )
        .unwrap();

        assert_eq!(
            actual,
            PublishedImageDigests {
                runtime: RUNTIME.to_owned(),
                publication: PUBLICATION.to_owned(),
            }
        );
    }

    #[test]
    fn rejects_a_publication_without_both_attestations() {
        let bytes = inspection(
            r#"{
              "mediaType": "application/vnd.in-toto+json",
              "annotations": {"in-toto.io/predicate-type": "https://spdx.dev/Document"}
            }"#,
        );

        let error = parse(
            &bytes,
            PUBLICATION,
            "linux/amd64",
            "registry.example/image@publication",
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("must carry SPDX SBOM and SLSA provenance")
        );
    }
}
