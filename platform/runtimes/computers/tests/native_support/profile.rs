//! Read-only admission of exact provider artifacts before native fixture effects.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use tokio::io::AsyncReadExt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
enum BinaryName {
    #[vocabulary(rename = "openshell")]
    Openshell,
    #[vocabulary(rename = "openshell-gateway")]
    OpenshellGateway,
    #[vocabulary(rename = "openshell-driver-docker")]
    OpenshellDriverDocker,
    #[vocabulary(rename = "openshell-supervisor")]
    OpenshellSupervisor,
    #[vocabulary(rename = "openshell-sandbox")]
    OpenshellSandbox,
}
impl BinaryName {
    fn text(self) -> &'static str {
        veoveo_types::Vocabulary::as_str(self)
    }
    fn variable(self) -> &'static str {
        match self {
            Self::Openshell => "VEOVEO_COMPUTERS_NATIVE_CLI",
            Self::OpenshellGateway => "VEOVEO_COMPUTERS_NATIVE_GATEWAY",
            Self::OpenshellDriverDocker => "VEOVEO_COMPUTERS_NATIVE_DRIVER",
            Self::OpenshellSupervisor => "VEOVEO_COMPUTERS_NATIVE_SUPERVISOR",
            Self::OpenshellSandbox => "VEOVEO_COMPUTERS_NATIVE_SANDBOX",
        }
    }
    fn version(self) -> &'static str {
        if self == Self::Openshell {
            veoveo_computers_runtime::CLI_VERSION
        } else {
            veoveo_computers_runtime::GATEWAY_VERSION
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum ReceiptSchema {
    #[vocabulary(rename = "veoveo.ai/openshell-native-profile/v1")]
    V1,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binary {
    name: BinaryName,
    path: PathBuf,
    #[serde(with = "veoveo_types::sha256_hex")]
    sha256: veoveo_types::Sha256Digest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SupervisorImage {
    reference: String,
    id: veoveo_types::Sha256Digest,
    source_tree: String,
    #[serde(with = "veoveo_types::sha256_hex")]
    supervisor_sha256: veoveo_types::Sha256Digest,
    #[serde(with = "veoveo_types::sha256_hex")]
    sandbox_sha256: veoveo_types::Sha256Digest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema: ReceiptSchema,
    #[serde(with = "veoveo_types::sha256_hex")]
    manifest_sha256: veoveo_types::Sha256Digest,
    supervisor_image: SupervisorImage,
    native_gateway_ip: std::net::Ipv4Addr,
    binaries: Vec<Binary>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileBinary {
    name: BinaryName,
    source_tree: String,
}
#[derive(Deserialize)]
struct Profile {
    binaries: Vec<ProfileBinary>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageInspection {
    id: veoveo_types::Sha256Digest,
    repo_digests: Vec<String>,
    config: ImageConfig,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageConfig {
    labels: std::collections::BTreeMap<String, String>,
}
fn pinned_image(value: &str) -> bool {
    value
        .split_once("@sha256:")
        .is_some_and(|(repository, digest)| {
            !repository.is_empty()
                && repository.len() <= 512
                && repository
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-/:".contains(&b))
                && veoveo_types::Sha256Digest::from_hex(digest).is_ok()
        })
}
impl Receipt {
    fn admit(&self) {
        let ReceiptSchema::V1 = self.schema;
        let manifest = include_bytes!("../../provider-patches/manifest.json");
        assert!(
            self.manifest_sha256
                == veoveo_types::Sha256Digest::from_bytes(Sha256::digest(manifest).into()),
            "native receipt differs from source manifest"
        );
        assert!(
            pinned_image(&self.supervisor_image.reference),
            "digest-pinned supervisor image required"
        );
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_SUPERVISOR_IMAGE")
            .expect("native supervisor image required");
        assert!(
            self.supervisor_image.reference == image,
            "supervisor image differs from receipt"
        );
        let profile: Profile = serde_json::from_slice(manifest).expect("provider source profile");
        let supervisor = profile
            .binaries
            .iter()
            .find(|binary| binary.name == BinaryName::OpenshellSupervisor)
            .unwrap();
        assert!(
            supervisor.source_tree == self.supervisor_image.source_tree,
            "companion source tree differs from patched profile"
        );
        assert!(
            self.binaries.len() == 5,
            "all five profile binaries required"
        );
        let mut names = BTreeSet::new();
        for binary in &self.binaries {
            assert!(names.insert(binary.name), "duplicate native binary");
            assert!(
                binary.path.is_absolute()
                    && binary.path.is_file()
                    && binary
                        .path
                        .file_name()
                        .is_some_and(|name| name == binary.name.text()),
                "invalid native artifact input"
            );
            let expected = PathBuf::from(
                std::env::var_os(binary.name.variable()).expect("native binary input required"),
            );
            assert!(
                expected == binary.path,
                "native binary path differs from receipt"
            );
            if binary.name == BinaryName::OpenshellSupervisor {
                assert!(
                    binary.sha256 == self.supervisor_image.supervisor_sha256,
                    "companion artifact differs from native supervisor"
                );
            }
            if binary.name == BinaryName::OpenshellSandbox {
                assert!(
                    binary.sha256 == self.supervisor_image.sandbox_sha256,
                    "companion image sandbox differs from injected artifact"
                );
            }
        }
    }
}
async fn bounded_stdout(mut command: tokio::process::Command, deadline: Duration) -> Vec<u8> {
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("read-only artifact command");
    let mut stdout = child.stdout.take().unwrap().take(16385);
    let mut stderr = child.stderr.take().unwrap().take(16385);
    tokio::time::timeout(deadline, async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (outcome, errors, status) = tokio::join!(
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err),
            child.wait()
        );
        outcome.expect("bounded artifact stdout");
        errors.expect("bounded artifact stderr");
        assert!(
            out.len() <= 16384
                && err.len() <= 16384
                && status.expect("artifact command status").success(),
            "read-only artifact admission failed"
        );
        out
    })
    .await
    .expect("artifact admission deadline")
}
#[allow(dead_code)] // Each included native scenario admits its own prerequisites.
pub async fn preflight() -> std::net::Ipv4Addr {
    let path = PathBuf::from(
        std::env::var_os("VEOVEO_COMPUTERS_NATIVE_PROFILE")
            .expect("0.1.2 native profile receipt required before allocation"),
    );
    assert!(
        path.is_absolute(),
        "native profile receipt must be absolute"
    );
    let mut bytes = Vec::new();
    tokio::fs::File::open(&path)
        .await
        .expect("native profile receipt")
        .take(16385)
        .read_to_end(&mut bytes)
        .await
        .expect("bounded profile receipt");
    assert!(bytes.len() <= 16384, "native profile receipt exceeds bound");
    let receipt: Receipt = serde_json::from_slice(&bytes).expect("closed native profile receipt");
    receipt.admit();
    assert!(
        receipt.native_gateway_ip.is_private(),
        "native gateway route must use a private bridge"
    );
    let mut route = tokio::process::Command::new("docker");
    route.args([
        "--host",
        "unix:///var/run/docker.sock",
        "network",
        "inspect",
        "bridge",
        "--format",
        "{{(index .IPAM.Config 0).Gateway}}",
    ]);
    let observed = bounded_stdout(route, Duration::from_secs(5)).await;
    let observed: std::net::Ipv4Addr = std::str::from_utf8(&observed)
        .unwrap()
        .trim()
        .parse()
        .expect("private native bridge gateway");
    assert!(
        observed == receipt.native_gateway_ip,
        "native gateway route differs from frozen receipt"
    );
    let mut inspect = tokio::process::Command::new("docker");
    inspect.args([
        "--host",
        "unix:///var/run/docker.sock",
        "image",
        "inspect",
        &receipt.supervisor_image.reference,
    ]);
    let output = bounded_stdout(inspect, Duration::from_secs(10)).await;
    let mut images: Vec<ImageInspection> =
        serde_json::from_slice(&output).expect("native image inspection");
    assert!(images.len() == 1, "one companion image required");
    let image = images.pop().unwrap();
    assert!(
        image_matches_receipt(&receipt, &image),
        "local companion image differs from patched artifact/source profile"
    );
    for binary in receipt.binaries {
        let input = tokio::fs::File::open(&binary.path)
            .await
            .expect("native binary");
        assert!(
            input.metadata().await.unwrap().len() <= 536870912,
            "native binary exceeds file bound"
        );
        let mut input = input.take(536870913);
        let mut count_total = 0u64;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let count = input.read(&mut buffer).await.expect("hash native binary");
            if count == 0 {
                break;
            }
            count_total += u64::try_from(count).unwrap();
            assert!(count_total <= 536870912, "native binary exceeds read bound");
            digest.update(&buffer[..count]);
        }
        assert!(
            veoveo_types::Sha256Digest::from_bytes(digest.finalize().into()) == binary.sha256,
            "native binary differs from receipt"
        );
        let mut command = tokio::process::Command::new(&binary.path);
        command.arg("--version");
        let output = bounded_stdout(command, Duration::from_secs(5)).await;
        assert!(
            String::from_utf8_lossy(&output).trim()
                == format!("{} {}", binary.name.text(), binary.name.version()),
            "native binary has unsupported profile version"
        );
    }
    receipt.native_gateway_ip
}

fn image_matches_receipt(receipt: &Receipt, image: &ImageInspection) -> bool {
    image.id == receipt.supervisor_image.id
        && image
            .repo_digests
            .contains(&receipt.supervisor_image.reference)
        && image
            .config
            .labels
            .get("ai.veoveo.provider.manifest-sha256")
            .is_some_and(|value| value == receipt.manifest_sha256.hex())
        && image
            .config
            .labels
            .get("ai.veoveo.provider.supervisor-source-tree")
            == Some(&receipt.supervisor_image.source_tree)
        && image
            .config
            .labels
            .get("ai.veoveo.provider.profile")
            .is_some_and(|value| value == veoveo_computers_runtime::GATEWAY_VERSION)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn digest(value: u8) -> veoveo_types::Sha256Digest {
        veoveo_types::Sha256Digest::from_bytes([value; 32])
    }
    #[test]
    fn native_profile_image_admission_rejects_unpatched_or_unmatched_artifacts() {
        let reference = format!("registry.internal/provider@{}", digest(1));
        let receipt = Receipt {
            schema: ReceiptSchema::V1,
            manifest_sha256: digest(2),
            native_gateway_ip: "172.17.0.1".parse().unwrap(),
            supervisor_image: SupervisorImage {
                reference: reference.clone(),
                id: digest(3),
                source_tree: "patched-tree".into(),
                supervisor_sha256: digest(4),
                sandbox_sha256: digest(5),
            },
            binaries: Vec::new(),
        };
        let labels = std::collections::BTreeMap::from([
            (
                "ai.veoveo.provider.profile".into(),
                veoveo_computers_runtime::GATEWAY_VERSION.into(),
            ),
            (
                "ai.veoveo.provider.manifest-sha256".into(),
                digest(2).hex().into(),
            ),
            (
                "ai.veoveo.provider.supervisor-source-tree".into(),
                "patched-tree".into(),
            ),
        ]);
        let image = |labels| ImageInspection {
            id: digest(3),
            repo_digests: vec![reference.clone()],
            config: ImageConfig { labels },
        };
        assert!(image_matches_receipt(&receipt, &image(labels.clone())));
        assert!(
            !image_matches_receipt(&receipt, &image(std::collections::BTreeMap::new())),
            "unpatched official image cannot supply patched companion"
        );
        for key in [
            "ai.veoveo.provider.profile",
            "ai.veoveo.provider.manifest-sha256",
            "ai.veoveo.provider.supervisor-source-tree",
        ] {
            let mut altered = labels.clone();
            altered.insert(key.into(), "other-profile".into());
            assert!(!image_matches_receipt(&receipt, &image(altered)));
        }
        let mut different = image(labels.clone());
        different.id = digest(9);
        assert!(!image_matches_receipt(&receipt, &different));
        let mut different = image(labels);
        different.repo_digests.clear();
        assert!(!image_matches_receipt(&receipt, &different));
    }
}
