use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::ManagedGpuAllocatorInstallation;

use super::{MANAGED_NODE_LABEL, MANAGED_NODE_LABEL_VALUE, path_str};
use crate::deployment::{output_checked, status_checked};

pub(super) struct VerifiedChart {
    pub(super) archive: PathBuf,
    _directory: tempfile::TempDir,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct HelmReleaseMetadata {
    pub(super) name: String,
    pub(super) namespace: String,
    revision: HelmRevision,
    pub(super) status: String,
    pub(super) chart: String,
    pub(super) app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum HelmRevision {
    Number(u64),
    Text(String),
}

impl HelmRevision {
    fn value(&self) -> Result<u64> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Text(value) => value
                .parse()
                .with_context(|| format!("decoding Helm release revision {value:?}")),
        }
    }
}

pub(super) fn pull_and_verify_chart(
    installation: &ManagedGpuAllocatorInstallation,
) -> Result<VerifiedChart> {
    let directory = tempfile::Builder::new()
        .prefix("veoveo-nvidia-dra-")
        .tempdir()
        .context("creating NVIDIA DRA chart verification directory")?;
    let output = Command::new("helm")
        .args([
            "pull",
            installation.chart.coordinate.as_str(),
            "--version",
            installation.chart.version.as_str(),
            "--destination",
            path_str(directory.path())?,
        ])
        .output()
        .context("pulling locked NVIDIA DRA chart")?;
    ensure!(
        output.status.success(),
        "Helm failed to pull locked NVIDIA DRA chart with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        report.contains(&format!("Digest: {}", installation.chart.digest)),
        "Helm pulled NVIDIA DRA chart without the locked OCI manifest digest {}; output was:\n{report}",
        installation.chart.digest
    );
    let archive = directory.path().join(format!(
        "dra-driver-nvidia-gpu-{}.tgz",
        installation.chart.version
    ));
    let bytes = fs::read(&archive)
        .with_context(|| format!("reading pulled NVIDIA DRA chart {}", archive.display()))?;
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    ensure!(
        digest == installation.chart.content_digest,
        "NVIDIA DRA chart archive digest is {digest}, expected {}",
        installation.chart.content_digest
    );
    Ok(VerifiedChart {
        archive,
        _directory: directory,
    })
}

pub(super) fn allocator_helm_args(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
    archive: &Path,
) -> Result<Vec<String>> {
    let mut args = vec![
        "--kube-context".to_owned(),
        context.to_owned(),
        "upgrade".to_owned(),
        "--install".to_owned(),
        installation.release_name.clone(),
        path_str(archive)?.to_owned(),
        "--namespace".to_owned(),
        installation.namespace.clone(),
        "--create-namespace".to_owned(),
        "--atomic".to_owned(),
        "--wait".to_owned(),
        "--timeout".to_owned(),
        format!("{}s", installation.timeout_seconds),
    ];
    args.extend(allocator_value_args(installation)?);
    Ok(args)
}

fn allocator_value_args(installation: &ManagedGpuAllocatorInstallation) -> Result<Vec<String>> {
    Ok(vec![
        "--set-string".to_owned(),
        format!("nvidiaDriverRoot={}", installation.nvidia_driver_root),
        "--set".to_owned(),
        "gpuResourcesEnabledOverride=true".to_owned(),
        "--set-string".to_owned(),
        "resourceApiVersion=resource.k8s.io/v1".to_owned(),
        "--set".to_owned(),
        "resources.gpus.enabled=true".to_owned(),
        "--set".to_owned(),
        "resources.computeDomains.enabled=false".to_owned(),
        "--set".to_owned(),
        "featureGates.TimeSlicingSettings=true".to_owned(),
        "--set".to_owned(),
        "webhook.enabled=false".to_owned(),
        "--set-json".to_owned(),
        format!(
            "kubeletPlugin.nodeSelector={}",
            serde_json::to_string(&BTreeMap::from([(
                MANAGED_NODE_LABEL,
                MANAGED_NODE_LABEL_VALUE
            )]))?
        ),
        "--set-json".to_owned(),
        "kubeletPlugin.affinity=null".to_owned(),
        "--set-string".to_owned(),
        format!("image.repository={}", installation.image.repository),
        "--set-string".to_owned(),
        format!(
            "image.tag={}@{}",
            installation.image.tag, installation.image.digest
        ),
    ])
}

pub(super) fn verify_allocator_chart_render(
    installation: &ManagedGpuAllocatorInstallation,
    chart: &VerifiedChart,
) -> Result<()> {
    let mut args = vec![
        "template".to_owned(),
        installation.release_name.clone(),
        path_str(&chart.archive)?.to_owned(),
        "--namespace".to_owned(),
        installation.namespace.clone(),
    ];
    args.extend(allocator_value_args(installation)?);
    let output = output_checked("helm", args.iter().map(String::as_str), None)
        .context("rendering the locked NVIDIA DRA chart")?;
    let output = String::from_utf8(output).context("decoding rendered NVIDIA DRA chart")?;
    let kubelet = output
        .split("\n---")
        .find(|document| {
            document.contains("kind: DaemonSet") && document.contains("kubelet-plugin")
        })
        .context("locked NVIDIA DRA chart rendered no kubelet-plugin DaemonSet")?;
    ensure!(
        kubelet.contains(&format!(
            "{MANAGED_NODE_LABEL}: \"{MANAGED_NODE_LABEL_VALUE}\""
        )),
        "locked NVIDIA DRA chart does not render the platform-managed node selector"
    );
    ensure!(
        !kubelet.contains("requiredDuringSchedulingIgnoredDuringExecution"),
        "locked NVIDIA DRA chart retains a required discovery affinity after the platform override"
    );
    let expected_image = format!(
        "{}:{}@{}",
        installation.image.repository, installation.image.tag, installation.image.digest
    );
    ensure!(
        kubelet.contains(&format!("image: {expected_image}")),
        "locked NVIDIA DRA chart does not render image {expected_image}"
    );
    Ok(())
}

pub(super) fn install_allocator_chart(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
    chart: &VerifiedChart,
) -> Result<()> {
    let args = allocator_helm_args(context, installation, &chart.archive)?;
    status_checked("helm", args.iter().map(String::as_str), &[], None)
        .context("installing the locked NVIDIA DRA driver")
}

pub(super) fn release_metadata(
    context: &str,
    namespace: &str,
    release_name: &str,
) -> Result<Option<HelmReleaseMetadata>> {
    ensure!(
        !release_name.is_empty()
            && release_name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "Helm release name {release_name:?} is not a canonical lowercase DNS label"
    );
    let filter = format!("^{release_name}$");
    let output = output_checked(
        "helm",
        [
            "--kube-context",
            context,
            "list",
            "--namespace",
            namespace,
            "--filter",
            filter.as_str(),
            "--max",
            "2",
            "--output",
            "json",
        ],
        None,
    )
    .with_context(|| format!("listing Helm release {namespace}/{release_name}"))?;
    decode_release_metadata(&output, namespace, release_name)
}

fn decode_release_metadata(
    output: &[u8],
    namespace: &str,
    release_name: &str,
) -> Result<Option<HelmReleaseMetadata>> {
    let releases: Vec<HelmReleaseMetadata> = serde_json::from_slice(output)
        .with_context(|| format!("decoding Helm 4 release list for {namespace}/{release_name}"))?;
    ensure!(
        releases.len() <= 1,
        "Helm returned {} exact-name records for release {namespace}/{release_name}",
        releases.len()
    );
    let Some(release) = releases.into_iter().next() else {
        return Ok(None);
    };
    ensure!(
        release.name == release_name,
        "Helm release metadata names {:?}, expected {release_name:?}",
        release.name
    );
    ensure!(
        release.namespace == namespace,
        "Helm release {release_name} reports namespace {:?}, expected {namespace:?}",
        release.namespace
    );
    ensure!(
        release.revision.value()? > 0,
        "Helm release {namespace}/{release_name} reports a zero revision"
    );
    Ok(Some(release))
}

pub(super) fn verify_allocator_release_metadata(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
) -> Result<()> {
    let release = release_metadata(context, &installation.namespace, &installation.release_name)?
        .with_context(|| {
        format!(
            "NVIDIA DRA Helm release {}/{} is absent",
            installation.namespace, installation.release_name
        )
    })?;
    validate_allocator_release_metadata(&release, installation)
}

fn validate_allocator_release_metadata(
    release: &HelmReleaseMetadata,
    installation: &ManagedGpuAllocatorInstallation,
) -> Result<()> {
    let expected_chart = format!("dra-driver-nvidia-gpu-{}", installation.chart.version);
    ensure!(
        release.chart == expected_chart,
        "NVIDIA DRA Helm release {}/{} uses chart {}, expected {expected_chart}",
        installation.namespace,
        installation.release_name,
        release.chart
    );
    ensure!(
        release.app_version == installation.chart.version,
        "NVIDIA DRA Helm release {}/{} reports app version {}, expected {}",
        installation.namespace,
        installation.release_name,
        release.app_version,
        installation.chart.version
    );
    ensure!(
        release.status == "deployed",
        "NVIDIA DRA Helm release {}/{} has status {}, expected deployed",
        installation.namespace,
        installation.release_name,
        release.status
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use veoveo_deploy_contract::ManagedGpuAllocatorInstallation;

    use super::{
        HelmReleaseMetadata, allocator_helm_args, decode_release_metadata,
        validate_allocator_release_metadata,
    };

    fn qualified_installation() -> ManagedGpuAllocatorInstallation {
        serde_json::from_value(serde_json::json!({
            "releaseName": "dra-driver-nvidia-gpu",
            "namespace": "nvidia-dra-driver-gpu",
            "chart": {
                "coordinate": "oci://registry.k8s.io/dra-driver-nvidia/charts/dra-driver-nvidia-gpu",
                "version": "0.4.1",
                "digest": "sha256:7a00373fdef1025f27ebb1d353719446bbbe6ec4697e9a503c5ffd7e4f1525dd",
                "contentDigest": "sha256:c1c316f6bdcfe5fed3ff649cff1b43be50d27d0cb1aaf9d29e7bdca1eaa331ce"
            },
            "image": {
                "repository": "registry.k8s.io/dra-driver-nvidia/dra-driver-nvidia-gpu",
                "tag": "v0.4.1",
                "digest": "sha256:eefe67396dedea4df74f68a94d5883f33204888b83979babd42b91501a2de1d8",
                "platformDigests": {
                    "linux/amd64": "sha256:ad86983849542f6ef22f02e963ecbf545706e037455e0c265889ace137863556",
                    "linux/arm64": "sha256:b51290bbc1ee6745adf8ffff040d2b917d3e07dbd5cd36fd444b0e371ccc9166"
                }
            },
            "nvidiaDriverRoot": "/",
            "eligibleNodeSelector": {"node.example/gpu": "true"},
            "conflictingDevicePluginRemoval": {"mode": "require-absent"},
            "maturityAcceptance": "technology-preview",
            "timeoutSeconds": 600
        }))
        .unwrap()
    }

    #[test]
    fn helm_4_release_list_shape_retains_chart_metadata() {
        let releases: Vec<HelmReleaseMetadata> = serde_json::from_str(
            r#"[{
                "name":"gpu-allocator",
                "namespace":"gpu-system",
                "revision":"2",
                "updated":"2026-08-02 00:00:00 +0000 UTC",
                "status":"deployed",
                "chart":"dra-driver-nvidia-gpu-0.4.1",
                "app_version":"0.4.1"
            }]"#,
        )
        .unwrap();

        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].chart, "dra-driver-nvidia-gpu-0.4.1");
        assert_eq!(releases[0].revision.value().unwrap(), 2);
    }

    #[test]
    fn helm_4_numeric_revision_is_also_typed() {
        let release: HelmReleaseMetadata = serde_json::from_str(
            r#"{
                "name":"gpu-allocator",
                "namespace":"gpu-system",
                "revision":3,
                "status":"deployed",
                "chart":"dra-driver-nvidia-gpu-0.4.1",
                "app_version":"0.4.1"
            }"#,
        )
        .unwrap();

        assert_eq!(release.revision.value().unwrap(), 3);
    }

    #[test]
    fn helm_4_release_selection_rejects_ambiguous_or_wrong_metadata() {
        assert!(
            decode_release_metadata(b"[]", "gpu-system", "gpu-allocator")
                .unwrap()
                .is_none()
        );
        let duplicate = br#"[
            {"name":"gpu-allocator","namespace":"gpu-system","revision":1,"status":"deployed","chart":"driver-0.4.1","app_version":"0.4.1"},
            {"name":"gpu-allocator","namespace":"gpu-system","revision":2,"status":"deployed","chart":"driver-0.4.1","app_version":"0.4.1"}
        ]"#;
        assert!(decode_release_metadata(duplicate, "gpu-system", "gpu-allocator").is_err());
        let wrong_namespace = br#"[
            {"name":"gpu-allocator","namespace":"other","revision":1,"status":"deployed","chart":"driver-0.4.1","app_version":"0.4.1"}
        ]"#;
        assert!(decode_release_metadata(wrong_namespace, "gpu-system", "gpu-allocator").is_err());
    }

    #[test]
    fn allocator_release_metadata_rejects_stale_chart_or_status() {
        let installation = qualified_installation();
        let mut release: HelmReleaseMetadata = serde_json::from_str(
            r#"{
                "name":"dra-driver-nvidia-gpu",
                "namespace":"nvidia-dra-driver-gpu",
                "revision":1,
                "status":"deployed",
                "chart":"dra-driver-nvidia-gpu-0.4.1",
                "app_version":"0.4.1"
            }"#,
        )
        .unwrap();
        validate_allocator_release_metadata(&release, &installation).unwrap();
        release.status = "failed".to_owned();
        assert!(validate_allocator_release_metadata(&release, &installation).is_err());
        release.status = "deployed".to_owned();
        release.chart = "dra-driver-nvidia-gpu-0.4.0".to_owned();
        assert!(validate_allocator_release_metadata(&release, &installation).is_err());
    }

    #[test]
    fn allocator_values_make_the_managed_selector_authoritative() {
        let installation = qualified_installation();
        let args =
            allocator_helm_args("example", &installation, Path::new("/tmp/driver.tgz")).unwrap();
        let rendered = args.join(" ");

        assert!(rendered.contains("resourceApiVersion=resource.k8s.io/v1"));
        assert!(rendered.contains("featureGates.TimeSlicingSettings=true"));
        assert!(rendered.contains("resources.computeDomains.enabled=false"));
        assert!(rendered.contains("gpuResourcesEnabledOverride=true"));
        assert!(rendered.contains("kubeletPlugin.affinity=null"));
        assert!(rendered.contains("nvidia.com/dra-kubelet-plugin"));
        assert!(rendered.contains("@sha256:eefe6739"));
    }
}
