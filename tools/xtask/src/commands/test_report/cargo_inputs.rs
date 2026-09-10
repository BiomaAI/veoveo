//! Conservative local Cargo closure, including development and build edges.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::catalog::relative;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
}
#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
struct Node {
    id: String,
    dependencies: Vec<String>,
}
#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
    source: Option<String>,
    #[serde(default)]
    metadata: Option<PackageMetadata>,
}
#[derive(Default, Deserialize)]
struct PackageMetadata {
    #[serde(default)]
    veoveo: VeoveoMetadata,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct VeoveoMetadata {
    #[serde(default)]
    image_build_inputs: Vec<String>,
    #[serde(default)]
    image_asset_inputs: Vec<String>,
}

pub(super) fn roots(root: &Path, selected: &[String]) -> Result<Vec<String>> {
    let compiler = crate::process::output_text("rustc", ["-vV"], Some(root))?;
    let target = compiler
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .context("Rust compiler did not identify its host target")?;
    let output = crate::process::output(
        "cargo",
        [
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--offline",
            "--all-features",
            "--filter-platform",
            target,
        ],
        Some(root),
    )?;
    roots_from_metadata(root, selected, &output.stdout)
}

fn roots_from_metadata(root: &Path, selected: &[String], bytes: &[u8]) -> Result<Vec<String>> {
    let metadata: Metadata =
        serde_json::from_slice(bytes).context("decoding test Cargo closure")?;
    let packages: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect();
    let edges: BTreeMap<_, _> = metadata
        .resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), &node.dependencies))
        .collect();
    let mut pending = Vec::new();
    for name in selected {
        let matches: Vec<_> = metadata
            .packages
            .iter()
            .filter(|package| package.name == *name && package.source.is_none())
            .collect();
        ensure!(
            matches.len() == 1,
            "test package must resolve to one local package: {name}"
        );
        pending.push(matches[0].id.as_str());
    }
    let mut visited = BTreeSet::new();
    let mut roots = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let package = packages
            .get(id)
            .context("Cargo dependency has no package")?;
        if package.source.is_none() {
            let directory = package
                .manifest_path
                .parent()
                .context("Cargo manifest has no parent")?;
            let path = directory.strip_prefix(root).context("test dependency escapes repository; external path dependencies require a qualified adapter")?;
            let path = path.to_str().context("Cargo package path is not UTF-8")?;
            relative(path)?;
            roots.insert(path.to_owned());
            if let Some(metadata) = &package.metadata {
                for extra in metadata
                    .veoveo
                    .image_build_inputs
                    .iter()
                    .chain(&metadata.veoveo.image_asset_inputs)
                {
                    relative(extra)?;
                    roots.insert(extra.to_owned());
                }
            }
        }
        for dependency in edges
            .get(id)
            .context("Cargo dependency has no resolution node")?
            .iter()
        {
            pending.push(dependency);
        }
    }
    Ok(roots.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_closure_includes_dev_build_and_declared_external_inputs() {
        let root = Path::new("/fixture");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "packages": [
                {"id":"service", "name":"service", "manifest_path":"/fixture/service/Cargo.toml", "source":null, "metadata":null},
                {"id":"shared", "name":"shared", "manifest_path":"/fixture/shared/Cargo.toml", "source":null, "metadata":{"veoveo":{"image-build-inputs":["schemas"]}}},
                {"id":"dev", "name":"dev", "manifest_path":"/fixture/testing/helper/Cargo.toml", "source":null, "metadata":{}},
                {"id":"build", "name":"build", "manifest_path":"/fixture/tools/generator/Cargo.toml", "source":null, "metadata":{}},
                {"id":"unrelated", "name":"unrelated", "manifest_path":"/fixture/unrelated/Cargo.toml", "source":null, "metadata":{}},
                {"id":"external", "name":"external", "manifest_path":"/registry/external/Cargo.toml", "source":"registry", "metadata":{}}
            ],
            "resolve":{"nodes":[
                {"id":"service", "dependencies":["shared", "dev", "build", "external"]},
                {"id":"shared", "dependencies":[]}, {"id":"dev", "dependencies":["shared"]},
                {"id":"build", "dependencies":[]}, {"id":"unrelated", "dependencies":[]},
                {"id":"external", "dependencies":[]}
            ]}
        })).unwrap();
        let roots = roots_from_metadata(root, &["service".to_owned()], &bytes).unwrap();
        assert_eq!(
            roots,
            [
                "schemas",
                "service",
                "shared",
                "testing/helper",
                "tools/generator"
            ]
        );
        assert!(roots_from_metadata(root, &["missing".to_owned()], &bytes).is_err());
        let escaped = String::from_utf8(bytes)
            .unwrap()
            .replace("/fixture/shared/Cargo.toml", "/outside/shared/Cargo.toml");
        assert!(roots_from_metadata(root, &["service".to_owned()], escaped.as_bytes()).is_err());
    }
}
