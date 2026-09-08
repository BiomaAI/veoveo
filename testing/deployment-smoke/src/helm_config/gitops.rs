//! One HelmRelease update selects immutable chart and values inputs together.
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::commands::{copy_fixture, run_checked};

#[derive(Debug, Deserialize)]
struct Metadata {
    name: String,
    namespace: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
enum Object {
    ConfigMap {
        metadata: Metadata,
        data: BTreeMap<String, String>,
        #[serde(default)]
        immutable: bool,
    },
    OCIRepository {
        metadata: Metadata,
        spec: SourceSpec,
    },
    HelmRelease {
        metadata: Metadata,
        spec: ReleaseSpec,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct SourceSpec {
    #[serde(rename = "ref")]
    reference: DigestReference,
}

#[derive(Debug, Deserialize)]
struct DigestReference {
    digest: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseSpec {
    chart_ref: ChartReference,
    values_from: Vec<ValuesReference>,
}

#[derive(Debug, Deserialize)]
struct ChartReference {
    kind: String,
    name: String,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValuesReference {
    kind: String,
    name: String,
    values_key: String,
}

#[derive(Debug, PartialEq)]
struct ReleaseInputs {
    chart_name: String,
    chart_digest: String,
    values: Vec<ValuesReference>,
}

fn render(root: &Path) -> Result<BTreeMap<String, ReleaseInputs>> {
    let yaml = run_checked(
        Path::new("kubectl"),
        ["kustomize".into(), root.as_os_str().to_owned()],
        [],
    )?;
    let objects = serde_yaml_ng::Deserializer::from_str(&yaml)
        .map(Object::deserialize)
        .collect::<Result<Vec<_>, _>>()?;
    let mut inputs = BTreeMap::new();
    for object in &objects {
        let Object::HelmRelease { metadata, spec } = object else {
            continue;
        };
        ensure!(
            spec.chart_ref.kind == "OCIRepository",
            "expected OCI chart reference"
        );
        let source = objects
            .iter()
            .find_map(|object| match object {
                Object::OCIRepository {
                    metadata: source,
                    spec: source_spec,
                } if source.namespace == metadata.namespace
                    && source.name == spec.chart_ref.name =>
                {
                    Some(source_spec)
                }
                _ => None,
            })
            .context("HelmRelease has a dangling chart reference")?;
        let digest = source
            .reference
            .digest
            .strip_prefix("sha256:")
            .context("chart source must select a SHA-256 manifest digest")?;
        ensure!(
            digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                && spec.chart_ref.name == format!("{}-chart-{digest}", metadata.name),
            "chart source name must bind its complete manifest digest"
        );
        let prefix = format!("bioma-{}-values-", metadata.name);
        for values in &spec.values_from {
            ensure!(
                values.kind == "ConfigMap"
                    && values.name.starts_with(&prefix)
                    && values.name.len() > prefix.len(),
                "Helm values must reference the owning release's generated content name"
            );
            let config = objects
                .iter()
                .find_map(|object| match object {
                    Object::ConfigMap {
                        metadata: config,
                        data,
                        immutable,
                    } if config.namespace == metadata.namespace && config.name == values.name => {
                        Some((config, data, immutable))
                    }
                    _ => None,
                })
                .context("HelmRelease has a dangling values reference")?;
            ensure!(*config.2, "release values ConfigMap must be immutable");
            ensure!(
                config.1.contains_key(&values.values_key),
                "missing Helm values key"
            );
            ensure!(
                config
                    .0
                    .labels
                    .get("reconcile.fluxcd.io/watch")
                    .map(String::as_str)
                    == Some("Enabled"),
                "release values must retain the Flux watch label"
            );
        }
        let values = spec
            .values_from
            .iter()
            .map(|value| ValuesReference {
                kind: value.kind.clone(),
                name: value.name.clone(),
                values_key: value.values_key.clone(),
            })
            .collect();
        ensure!(
            inputs
                .insert(
                    metadata.name.clone(),
                    ReleaseInputs {
                        chart_name: spec.chart_ref.name.clone(),
                        chart_digest: source.reference.digest.clone(),
                        values,
                    }
                )
                .is_none(),
            "duplicate HelmRelease input owner"
        );
    }
    ensure!(
        inputs.len() == 2,
        "expected the platform and UAV reference releases"
    );
    Ok(inputs)
}

pub(super) fn check() -> Result<()> {
    let fixture = Path::new("examples/bioma");
    let before = render(fixture)?;
    for (selected, other, values_path) in [
        ("veoveo", "uav-sim", "images/veoveo.lock.yaml"),
        ("uav-sim", "veoveo", "images/uav-sim.lock.yaml"),
    ] {
        let temporary = tempfile::tempdir()?;
        copy_fixture(fixture, temporary.path())?;
        let path = temporary.path().join(values_path);
        let mut values = fs::read_to_string(&path)?;
        // A valid YAML comment changes the exact public input bytes without
        // introducing a fake image publication or invalid chart values.
        values.push_str("\n# Configuration byte-identity regression.\n");
        fs::write(path, values)?;
        let after = render(temporary.path())?;
        ensure!(
            before[other] == after[other],
            "unselected release inputs changed"
        );
        ensure!(
            before[selected].values != after[selected].values,
            "values update reused its old ConfigMap name"
        );
        ensure!(
            before[selected].chart_name == after[selected].chart_name,
            "values-only update changed the chart reference"
        );

        // A chart update changes its source object and reference together;
        // the previous source object therefore cannot serve the new release.
        let old = &before[selected].chart_digest;
        let old_hash = old
            .strip_prefix("sha256:")
            .context("missing digest prefix")?;
        let changed_hash = if old_hash.starts_with('a') {
            "b".repeat(64)
        } else {
            "a".repeat(64)
        };
        for file in [
            format!("gitops/sources/{selected}.yaml"),
            format!("gitops/releases/{selected}.yaml"),
        ] {
            let path = temporary.path().join(file);
            let text = fs::read_to_string(&path)?.replace(old_hash, &changed_hash);
            fs::write(path, text)?;
        }
        let coupled = render(temporary.path())?;
        ensure!(
            before[other] == coupled[other],
            "unselected release changed during chart publication"
        );
        ensure!(
            after[selected].values == coupled[selected].values,
            "chart update changed unchanged values"
        );
        ensure!(
            after[selected].chart_name != coupled[selected].chart_name,
            "new chart reused the old source object"
        );
    }
    println!(
        "GitOps releases select immutable chart and values references; unselected release inputs stay unchanged."
    );
    Ok(())
}
