use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    verify_protocol()?;
    verify_terminal_protocol()?;
    generate_openshell()?;
    generate_allocation()?;
    Ok(())
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
