use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    verify_profile()?;
    verify_protocol()?;
    verify_terminal_protocol()?;
    generate_openshell()?;
    generate_maintenance()?;
    generate_allocation()?;
    Ok(())
}

fn verify_profile() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Profile {
        upstream_revision: String,
        upstream_tag: String,
        upstream_source_tree: String,
        cli_version: String,
        gateway_version: String,
        supervisor_version: String,
        sandbox_version: String,
        driver_version: String,
        compiler: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Input {
        file: String,
        sha256: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Provenance {
        schema: String,
        repository: String,
        revision: String,
        tag: String,
        source_tree: String,
        inputs: Vec<Input>,
    }
    println!("cargo:rerun-if-changed=provider-patches/manifest.json");
    let profile: Profile = serde_json::from_slice(&fs::read("provider-patches/manifest.json")?)?;
    let provenance: Provenance = serde_json::from_slice(&fs::read("protocol/provenance.json")?)?;
    if profile.upstream_revision != "6648bd0c290efbc41ba131ee9831ee45cd431f94"
        || profile.upstream_tag != "v0.1.2"
        || profile.cli_version != "0.1.2"
        || profile.gateway_version != "0.1.2-veoveo.1"
        || profile.compiler != "1.99.0"
        || [
            profile.supervisor_version,
            profile.sandbox_version,
            profile.driver_version,
        ]
        .iter()
        .any(|version| version != &profile.gateway_version)
        || provenance.schema != "veoveo.ai/openshell-protocol/v1"
        || provenance.repository != "https://github.com/NVIDIA/OpenShell"
        || provenance.revision != profile.upstream_revision
        || provenance.tag != profile.upstream_tag
        || provenance.source_tree != profile.upstream_source_tree
    {
        return Err(
            "unsupported OpenShell provider/protocol profile; coordinate installation upgrade"
                .into(),
        );
    }
    let mut inputs = std::collections::BTreeSet::new();
    for input in provenance.inputs {
        if ![
            "openshell.proto",
            "sandbox.proto",
            "datamodel.proto",
            "options.proto",
            "LICENSE",
        ]
        .contains(&input.file.as_str())
            || !inputs.insert(input.file.clone())
            || hex_digest(&fs::read(PathBuf::from("protocol").join(input.file))?) != input.sha256
        {
            return Err("OpenShell licensed protocol provenance mismatch".into());
        }
    }
    if inputs.len() != 5 {
        return Err("incomplete OpenShell protocol provenance".into());
    }
    println!(
        "cargo:rustc-env=VEOVEO_OPENSHELL_GATEWAY_VERSION={}",
        profile.gateway_version
    );
    println!(
        "cargo:rustc-env=VEOVEO_OPENSHELL_CLI_VERSION={}",
        profile.cli_version
    );
    Ok(())
}
fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn verify_terminal_protocol() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read("protocol/terminal/terminal_events.proto")?;
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != "913ca6e53b504dcfe8fabf0568ce7051be3330fbce0967d9897edafeff27612b" {
        return Err("vendored terminal protocol integrity mismatch".into());
    }
    Ok(())
}

fn verify_protocol() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from("protocol");
    println!("cargo:rerun-if-changed=protocol");
    for line in fs::read_to_string(root.join("SHA256SUMS"))?.lines() {
        let (hash, file) = line.split_once("  ").ok_or("invalid protocol manifest")?;
        let actual: String = Sha256::digest(fs::read(root.join(file))?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if actual != hash {
            return Err("vendored protocol integrity mismatch".into());
        }
    }
    Ok(())
}

fn generate_openshell() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from("protocol");
    let mut prost = prost_build::Config::new();
    prost.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    prost.btree_map(["."]);
    // Provider credentials and arbitrary process output must never acquire
    // generated field-by-field Debug implementations.
    prost.skip_debug(["."]);
    prost.file_descriptor_set_path(PathBuf::from(env::var("OUT_DIR")?).join("openshell.bin"));
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .generate_default_stubs(true)
        .compile_with_config(
            prost,
            &[
                root.join("openshell.proto"),
                root.join("terminal/terminal_events.proto"),
            ],
            &[root, protoc_bin_vendored::include_path()?],
        )?;
    Ok(())
}

fn generate_maintenance() -> Result<(), Box<dyn std::error::Error>> {
    let mut prost = prost_build::Config::new();
    prost.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    prost.btree_map(["."]);
    prost.skip_debug(["."]);
    prost.extern_path(".openshell", "crate::protocol");
    prost.compile_protos(
        &[PathBuf::from("protocol/maintenance.proto")],
        &[
            PathBuf::from("protocol"),
            protoc_bin_vendored::include_path()?,
        ],
    )?;
    Ok(())
}

fn generate_allocation() -> Result<(), Box<dyn std::error::Error>> {
    // The RootSchema type is inferred from Typify. Do not change the workspace's
    // schemars version to match a generator's private dependency.
    let schema = serde_json::from_slice(&fs::read("protocol/storage.json")?)?;
    let mut settings = typify::TypeSpaceSettings::default();
    settings.with_derive("PartialEq".to_owned());
    let mut types = typify::TypeSpace::new(&settings);
    types.add_root_schema(schema)?;
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("allocation.rs"),
        types.to_stream().to_string(),
    )?;
    Ok(())
}
