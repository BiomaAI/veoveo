//! Helm execution over an already checked, complete release render.

use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::process::{path_str, status_checked};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChartMetadata {
    pub(crate) api_version: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) app_version: Option<String>,
}

impl ChartMetadata {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        serde_yaml_ng::from_slice(&fs::read(path)?).context("decoding compiled Helm chart metadata")
    }
}

pub(crate) struct CompiledHelmRelease {
    directory: tempfile::TempDir,
    timeout_seconds: u64,
}

/// A CRD belongs to its declared component but outlives a Helm uninstall. Its
/// retained policy is part of the compiled object digest, never added after lock.
pub(crate) fn preserve_crds(objects: &mut [Value]) -> Result<()> {
    for object in objects {
        if object["apiVersion"] == "apiextensions.k8s.io/v1"
            && object["kind"] == "CustomResourceDefinition"
        {
            let metadata = object
                .get_mut("metadata")
                .and_then(Value::as_object_mut)
                .context("CRD metadata must be an object")?;
            let annotations = metadata
                .entry("annotations")
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .context("CRD annotations must be an object")?;
            annotations.insert(
                "helm.sh/resource-policy".into(),
                Value::String("keep".into()),
            );
        }
    }
    Ok(())
}

impl CompiledHelmRelease {
    pub(crate) fn prepare(
        metadata: &ChartMetadata,
        objects: &[Value],
        timeout_seconds: u64,
    ) -> Result<Self> {
        ensure!(
            metadata.api_version == "v2",
            "compiled releases require Helm chart API v2"
        );
        ensure!(
            !objects.is_empty(),
            "compiled Helm release must have a complete object inventory"
        );
        let directory = tempfile::Builder::new()
            .prefix("veoveo-compiled-helm-")
            .tempdir()?;
        fs::create_dir(directory.path().join("templates"))?;
        fs::write(
            directory.path().join("Chart.yaml"),
            serde_yaml_ng::to_string(metadata)?,
        )?;
        // Files.Get emits these bytes without re-evaluating template syntax in
        // configuration data. Release counters, lookups, and destination API
        // capabilities cannot cause a second rendering of the original chart.
        fs::write(
            directory.path().join("templates/objects.yaml"),
            "{{ .Files.Get \"objects.yaml\" }}\n",
        )?;
        let mut documents = String::new();
        for object in objects {
            ensure!(
                object["kind"] != "Secret" || object["apiVersion"] != "v1",
                "compiled releases cannot contain Secrets"
            );
            documents.push_str("---\n");
            documents.push_str(&serde_json::to_string(object)?);
            documents.push('\n');
        }
        fs::write(directory.path().join("objects.yaml"), documents)?;
        Ok(Self {
            directory,
            timeout_seconds,
        })
    }

    pub(crate) fn install(&self, context: &str, namespace: &str, name: &str) -> Result<()> {
        status_checked(
            "helm",
            [
                "--kube-context",
                context,
                "upgrade",
                "--install",
                name,
                path_str(self.directory.path())?,
                "--namespace",
                namespace,
                "--atomic",
                "--wait",
                "--wait-for-jobs",
                "--timeout",
                &format!("{}s", self.timeout_seconds),
            ],
            &[],
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{configuration::append_yaml_bytes, process::output_checked};

    #[test]
    fn complete_prepared_objects_survive_helm_without_template_re_evaluation() {
        let metadata = ChartMetadata {
            api_version: "v2".into(),
            name: "compiled-fixture".into(),
            version: "1.0.0".into(),
            app_version: Some("1.0.0".into()),
        };
        let mut objects = vec![
            serde_json::json!({"apiVersion":"v1", "kind":"ConfigMap", "metadata":{"name":"public", "namespace":"veoveo"},
                "data":{"literal":"{{ fail \"must remain configuration data\" }}", "setting":"one"}}),
            serde_json::json!({"apiVersion":"apiextensions.k8s.io/v1", "kind":"CustomResourceDefinition", "metadata":{"name":"scenes.example.invalid"},
                "spec":{"group":"example.invalid", "names":{"kind":"Scene", "plural":"scenes"}, "scope":"Namespaced", "versions":[{"name":"v1", "served":true, "storage":true}]}}),
        ];
        preserve_crds(&mut objects).unwrap();
        let bundle = CompiledHelmRelease::prepare(&metadata, &objects, 60).unwrap();
        for mode in ["--is-upgrade", "--include-crds"] {
            let bytes = output_checked(
                "helm",
                [
                    "template",
                    "example",
                    path_str(bundle.directory.path()).unwrap(),
                    "--namespace",
                    "unrelated-default",
                    mode,
                ],
                None,
            )
            .unwrap();
            let mut rendered = Vec::new();
            append_yaml_bytes(&bytes, "compiled Helm fixture", &mut rendered).unwrap();
            assert_eq!(rendered.len(), objects.len());
            for expected in &objects {
                assert!(rendered.contains(expected));
            }
        }
        assert_eq!(
            objects[1]["metadata"]["annotations"]["helm.sh/resource-policy"],
            "keep"
        );
    }
}
