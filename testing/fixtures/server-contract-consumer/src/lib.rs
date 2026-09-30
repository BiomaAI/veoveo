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
        use veoveo_speech_mcp::contract::{
            DictationSessionId, DictationUri, SpeechResource, TranscriptionId, TranscriptionUri,
        };
        let session = DictationSessionId::new();
        let draft = DictationUri::new(session);
        assert_eq!(
            SpeechResource::parse(&draft.to_string()).unwrap(),
            SpeechResource::Dictation(session)
        );
        let task = TranscriptionId::new();
        assert_eq!(
            SpeechResource::parse(&TranscriptionUri::new(task).to_string()).unwrap(),
            SpeechResource::Transcript(task)
        );
    }

    #[test]
    fn resource_contracts_share_typed_artifact_and_frame_owners() {
        use veoveo_frames_mcp::contract::{
            FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldUri, FramesResource,
        };
        use veoveo_timeseries_mcp::contract::{TimeseriesArtifactUri, TimeseriesResource};
        use veoveo_types::ResourceAddress;
        let id = ArtifactId::new();
        let forecast = TimeseriesArtifactUri::new(id);
        assert_eq!(forecast.artifact_id(), id);
        for resource in [
            TimeseriesResource::ForecastApp,
            TimeseriesResource::Artifact(forecast),
        ] {
            assert_eq!(
                TimeseriesResource::parse(resource.to_uri().unwrap().as_str()).unwrap(),
                resource
            );
        }
        let world = FrameWorldUri::new(&FrameWorldId::new("survey").unwrap());
        let revision = world.revision(&FrameWorldRevisionId::new("revision-1").unwrap());
        let frame = revision.frame(&FrameId::new("camera").unwrap());
        for resource in [
            FramesResource::World(world),
            FramesResource::Frame(frame),
            FramesResource::Artifact(id),
        ] {
            assert_eq!(
                FramesResource::parse(resource.to_uri().unwrap().as_str()).unwrap(),
                resource
            );
        }
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
                "veoveo-frames-mcp",
                "veoveo-timeseries-mcp",
                "veoveo-map-mcp",
                "veoveo-duckdb-mcp",
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
                        "veoveo-frames-mcp",
                        "veoveo-timeseries-mcp",
                        "veoveo-frames-contract",
                        "veoveo-map-mcp",
                        "veoveo-duckdb-mcp",
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
