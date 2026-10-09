use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    verify_profile()?;
    verify_protocol()?;
    generate_openshell()?;
    generate_maintenance()?;
    generate_allocation()?;
    Ok(())
}

fn verify_profile() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Profile {
        schema: String,
        upstream_repository: String,
        upstream_revision: String,
        upstream_tag: String,
        upstream_source_tree: String,
        cli_version: String,
        gateway_version: String,
        supervisor_version: String,
        sandbox_version: String,
        driver: String,
        driver_integration: String,
        platform: String,
        binaries: Vec<Binary>,
        images: Images,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Binary {
        name: BinaryName,
        version: String,
        source_tree: String,
        target: String,
        image_path: String,
        release_asset: ReleaseAsset,
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    enum BinaryName {
        #[serde(rename = "openshell")]
        Cli,
        #[serde(rename = "openshell-gateway")]
        Gateway,
        #[serde(rename = "openshell-supervisor")]
        Supervisor,
        #[serde(rename = "openshell-sandbox")]
        Sandbox,
    }
    impl BinaryName {
        fn spelling(self) -> &'static str {
            match self {
                Self::Cli => "openshell",
                Self::Gateway => "openshell-gateway",
                Self::Supervisor => "openshell-supervisor",
                Self::Sandbox => "openshell-sandbox",
            }
        }
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ReleaseAsset {
        url: String,
        archive_sha256: String,
        executable_sha256: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Images {
        gateway: Image,
        supervisor: Image,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Image {
        repository: String,
        index_digest: String,
        manifest_digest: String,
        config_digest: String,
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
    println!("cargo:rerun-if-changed=provider/manifest.json");
    let profile: Profile = serde_json::from_slice(&fs::read("provider/manifest.json")?)?;
    let provenance: Provenance = serde_json::from_slice(&fs::read("protocol/provenance.json")?)?;
    if profile.schema != "veoveo.ai/openshell-release-profile/v1"
        || profile.upstream_repository != "https://github.com/NVIDIA/OpenShell"
        || profile.driver != "docker"
        || profile.driver_integration != "gateway_in_process"
        || profile.platform != "linux/amd64"
        || profile.upstream_source_tree != "26f142d5c3be825e72f4d1a705ecf6aa5d4d6b10"
        || profile.upstream_revision != "6648bd0c290efbc41ba131ee9831ee45cd431f94"
        || profile.upstream_tag != "v0.1.2"
        || profile.cli_version != "0.1.2"
        || profile.gateway_version != "0.1.2"
        || [profile.supervisor_version, profile.sandbox_version]
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
    let sha256 = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    let mut products = std::collections::BTreeSet::new();
    for binary in profile.binaries {
        let target = match binary.name {
            BinaryName::Cli | BinaryName::Sandbox => "x86_64-unknown-linux-musl",
            BinaryName::Gateway | BinaryName::Supervisor => "x86_64-unknown-linux-gnu",
        };
        if !products.insert(binary.name)
            || binary.version != "0.1.2"
            || binary.source_tree != profile.upstream_source_tree
            || binary.target != target
            || binary.image_path != format!("/usr/local/bin/{}", binary.name.spelling())
            || binary.release_asset.url
                != format!(
                    "https://github.com/NVIDIA/OpenShell/releases/download/v0.1.2/{}-{target}.tar.gz",
                    binary.name.spelling()
                )
            || !sha256(&binary.release_asset.archive_sha256)
            || !sha256(&binary.release_asset.executable_sha256)
        {
            return Err("invalid OpenShell released product identity".into());
        }
    }
    if products.len() != 4 {
        return Err("incomplete OpenShell released product profile".into());
    }
    for (image, name) in [
        (profile.images.gateway, "gateway"),
        (profile.images.supervisor, "supervisor"),
    ] {
        if image.repository != format!("ghcr.io/nvidia/openshell/{name}")
            || [
                &image.index_digest,
                &image.manifest_digest,
                &image.config_digest,
            ]
            .iter()
            .any(|digest| !digest.strip_prefix("sha256:").is_some_and(sha256))
        {
            return Err("invalid OpenShell official image identity".into());
        }
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
            &[root.join("openshell.proto")],
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
