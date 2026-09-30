//! A separate Cargo resolver consumes public server libraries without hosted features.
use veoveo_artifact_mcp::contract::{ArtifactId, ArtifactReference};
use veoveo_computers_mcp::contract::Action;
use veoveo_speech_mcp::contract::TranscribeRequest;

pub fn public_requests(id: ArtifactId) -> (ArtifactReference, Action, TranscribeRequest) {
    (
        ArtifactReference { artifact_id: id },
        Action::Create,
        TranscribeRequest {
            artifact_uri: id.plane_uri(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_artifact_mcp::contract::ArtifactResource;
    #[test]
    fn server_libraries_share_the_owner_types() {
        let id = ArtifactId::new();
        let (artifact, computer, speech) = public_requests(id);
        assert_eq!(artifact.artifact_id, speech.source().unwrap());
        assert_eq!(computer, Action::Create);
        let address = ArtifactResource::Metadata(id);
        assert_eq!(
            ArtifactResource::parse(address.to_uri().as_str()).unwrap(),
            address
        );
    }

    #[test]
    fn resolved_graph_contains_only_public_contracts() {
        let output = std::process::Command::new("timeout")
            .args([
                "30s",
                env!("CARGO"),
                "tree",
                "--locked",
                "--offline",
                "--manifest-path",
            ])
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .args([
                "--edges",
                "normal,build",
                "--prefix",
                "none",
                "--format",
                "{p} features={f}",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let graph = String::from_utf8(output.stdout).unwrap();
        for line in graph.lines() {
            let name = line.split_whitespace().next().unwrap();
            if [
                "veoveo-artifact-mcp",
                "veoveo-computers-mcp",
                "veoveo-speech-mcp",
            ]
            .contains(&name)
            {
                assert_eq!(
                    line.split_once("features=")
                        .unwrap()
                        .1
                        .split_whitespace()
                        .next(),
                    Some("contract")
                );
            }
            assert!(
                !name.starts_with("veoveo-")
                    || [
                        "veoveo-server-contract-consumer",
                        "veoveo-artifact-mcp",
                        "veoveo-computers-mcp",
                        "veoveo-speech-mcp",
                        "veoveo-artifact-contract",
                        "veoveo-computers-contract",
                        "veoveo-speech-contract",
                        "veoveo-types"
                    ]
                    .contains(&name),
                "implementation dependency: {line}"
            );
            assert!(
                !name.starts_with("surreal")
                    && !name.starts_with("re_")
                    && !name.starts_with("tokio")
                    && !["rmcp", "axum", "reqwest", "duckdb", "tonic"].contains(&name),
                "runtime dependency: {line}"
            );
        }
    }
}
