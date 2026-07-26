use std::{fs, path::PathBuf};

use veoveo_mcp_conformance::HostedServerConformanceProfile;

#[test]
fn anonymous_hosted_server_example_matches_the_generated_schema() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("profiles/hosted-server.example.json")).unwrap(),
    )
    .unwrap();
    let profile: HostedServerConformanceProfile = serde_json::from_value(value.clone()).unwrap();
    profile.validate().unwrap();

    let schema =
        serde_json::to_value(schemars::schema_for!(HostedServerConformanceProfile)).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&value)
        .unwrap();
}
