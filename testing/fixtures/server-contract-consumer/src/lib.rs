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
    #[test]
    fn map_handoff_and_computer_grant_types_need_no_host_runtime() {
        use veoveo_computers_mcp::contract::{AutomationGrantId, AutomationGrantUri, ComputerId};
        use veoveo_map_mcp::contract::*;
        let now = "2026-10-04T00:00:00Z".parse().unwrap();
        let route = MapRouteUri::new(RouteId::new());
        let handoff = MapRouteHandoffBuilder {
            schema_profile: MapRouteHandoffSchema::V1,
            route_uri: route.clone(),
            route_digest_sha256: veoveo_types::Sha256Digest::from_bytes([1; 32]),
            route_status: RouteStatus::Validated,
            mobility_profile_uri: MapMobilityProfileUri::new(
                MobilityProfileId::new(),
                MobilityProfileVersion::FIRST,
            ),
            path: vec![
                Wgs84Position::new(0., 0., None).unwrap(),
                Wgs84Position::new(1., 1., None).unwrap(),
            ],
            validation_id: ValidationId::new(),
            validated_at: now,
            operational_snapshot_id: OperationalSnapshotId::new(),
            base_release_ids: vec![DatasetReleaseId::new()],
            restriction_ids: vec![],
            prepared_at: now,
        }
        .build()
        .unwrap();
        assert_eq!(handoff.route_uri(), &route);
        let computer = ComputerId::new();
        let grant = AutomationGrantId::new();
        let uri = AutomationGrantUri::new(computer, grant);
        assert_eq!(uri.computer_id(), computer);
        assert_eq!(
            AutomationGrantUri::try_from(String::from(uri))
                .unwrap()
                .grant_id(),
            grant
        );
    }
    #[test]
    fn analytical_consumers_share_sources_and_build_checked_forecast_requests() {
        use veoveo_duckdb_mcp::contract::{DuckDbSource, DuckDbTabularSource};
        use veoveo_timeseries_mcp::contract::{
            TimeseriesFilterCombination, TimeseriesFilterPredicate, TimeseriesFilterValue,
            TimeseriesFilterValues, TimeseriesForecastHorizon, TimeseriesForecastRequest,
            TimeseriesRowFilter, TimeseriesTableMapping,
        };
        let source = DuckDbTabularSource::InlineCsv {
            csv: "value\n1\n2\n".into(),
            filename: None,
            options: Default::default(),
        };
        let column = "value".parse().unwrap();
        let request = TimeseriesForecastRequest::new(
            source.clone(),
            TimeseriesTableMapping::new(column),
            TimeseriesForecastHorizon::new(4).unwrap(),
        )
        .with_training_filter(TimeseriesRowFilter::new(
            TimeseriesFilterCombination::All,
            TimeseriesFilterPredicate::In {
                column: "value".parse().unwrap(),
                values: TimeseriesFilterValues::new(
                    TimeseriesFilterValue::I64(1),
                    [TimeseriesFilterValue::I64(2)],
                ),
            },
            [],
        ));
        assert_eq!(request.source, source);
        assert_eq!(request.horizon.get(), 4);
        assert_eq!(request.training_filter.unwrap().predicates().len(), 1);
        assert_eq!(
            DuckDbSource::from(source.clone()),
            DuckDbSource::Tabular(source)
        );
    }
    #[test]
    fn map_direct_resources_preserve_parent_identity_without_mcp() {
        use veoveo_map_mcp::contract::{
            FeatureLayerId, LayerProductId, LayerPublicationId, MapResource,
        };
        let address = MapResource::Product {
            layer: FeatureLayerId::new(),
            publication: LayerPublicationId::new(),
            product: LayerProductId::new(),
        };
        assert_eq!(
            MapResource::parse(address.to_uri().as_str()).unwrap(),
            address
        );
    }
    #[test]
    #[cfg(feature = "knowledge")]
    fn source_descriptors_are_available_without_hosted_dependencies() {
        use veoveo_map_mcp::contract::MapKnowledgeCollection;
        use veoveo_reason_mcp::contract::FindingCollection;
        for collection in MapKnowledgeCollection::ALL {
            let descriptor = collection.descriptor();
            assert!(
                descriptor
                    .required_scopes()
                    .contains(&collection.scope().into())
            );
        }
        for collection in FindingCollection::ALL {
            assert!(collection.descriptor().required_scopes().is_empty());
        }
    }

    #[test]
    fn map_knowledge_addresses_are_available_without_runtime_dependencies() {
        use veoveo_map_mcp::contract::{
            FeatureLayerId, MapFeatureId, MapKnowledgeCollection, MapKnowledgeCursor,
            MapKnowledgeMember, MapKnowledgePageUri,
        };
        let member = MapKnowledgeMember::Feature {
            layer: FeatureLayerId::new(),
            feature: MapFeatureId::new(),
        };
        assert_eq!(
            MapKnowledgeMember::parse(member.to_uri().as_str()).unwrap(),
            member
        );
        assert_ne!(member.source_uri(), member.to_uri());
        let page = MapKnowledgePageUri::new(MapKnowledgeCollection::Features)
            .with_cursor(MapKnowledgeCursor::after(member))
            .unwrap();
        assert_eq!(
            MapKnowledgePageUri::parse(page.to_uri().as_str()).unwrap(),
            page
        );
    }

    use veoveo_artifact_mcp::contract::ArtifactResource;
    #[test]
    fn computers_consumers_share_owned_ids_and_addresses() {
        use veoveo_computers_mcp::contract::{
            AccessGrantId, AutomationGrantId, CliPairingId, ComputerId, ComputerResource,
            ExecutionId, ExecutionResultUri, RevokeAccessInput,
        };
        let computer = ComputerId::new();
        let revoke = RevokeAccessInput {
            computer_id: computer,
            grant_id: AccessGrantId::new(),
        };
        assert_eq!(revoke.computer_id, computer);
        let pairing = CliPairingId::new();
        assert_eq!(
            pairing.to_string().parse::<CliPairingId>().unwrap(),
            pairing
        );
        let grant = AutomationGrantId::new();
        let execution = ExecutionId::new();
        let result = ExecutionResultUri::new(execution);
        assert_eq!(result.execution_id(), execution);
        for resource in [
            ComputerResource::Computer(computer),
            ComputerResource::Collection(Some(computer)),
            ComputerResource::Grant { computer, grant },
            ComputerResource::Execution(execution),
        ] {
            assert_eq!(
                ComputerResource::parse(resource.to_uri().as_str()).unwrap(),
                resource
            );
        }
    }
    #[test]
    fn computer_results_share_artifact_identity_without_loading_workers() {
        use veoveo_computers_mcp::contract::{
            ComputerId, ExecutionId, ExecutionOutput, ExecutionResult, RequestId, StartInput,
            TemplateId,
        };
        let request = StartInput {
            request_id: RequestId::new(),
            grant_id: None,
        };
        assert_eq!(
            request.request_id.to_string().parse::<RequestId>().unwrap(),
            request.request_id
        );
        let template: TemplateId = "development".parse().unwrap();
        assert_eq!(template.as_str(), "development");
        let stdout = ArtifactId::new();
        let execution = ExecutionId::new();
        let result = ExecutionResult::new(
            ComputerId::new(),
            execution,
            0,
            ExecutionOutput {
                artifact_id: stdout,
                byte_count: 3,
            },
            ExecutionOutput {
                artifact_id: ArtifactId::new(),
                byte_count: 0,
            },
        )
        .unwrap();
        assert_eq!(result.stdout().artifact_id, stdout);
        assert_eq!(result.result_uri().execution_id(), execution);
    }
    #[test]
    fn media_consumers_share_typed_model_and_artifact_addresses() {
        use veoveo_media_mcp::contract::{MediaArtifactUri, MediaModelUri, MediaResource, RunArgs};
        use veoveo_types::ResourceAddress;
        let request = RunArgs {
            model: "openai/gpt-image-2/edit".parse().unwrap(),
            input: Default::default(),
        };
        for resource in [
            MediaResource::Model(MediaModelUri::new(request.model)),
            MediaResource::Artifact(MediaArtifactUri::new(ArtifactId::new())),
        ] {
            assert_eq!(
                MediaResource::parse(resource.to_uri().unwrap().as_str()).unwrap(),
                resource
            );
        }
    }
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
        let native_task = veoveo_types::TaskId::new();
        let task = TranscriptionId::try_from(native_task).unwrap();
        assert_eq!(task.task_id(), native_task);
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
        let world = FrameWorldUri::new(&FrameWorldId::parse("survey").unwrap());
        let revision = world.revision(&FrameWorldRevisionId::parse("revision-1").unwrap());
        let frame = revision.frame(&FrameId::parse("camera").unwrap());
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

    fn check_scopes<T>(external: &veoveo_types::ScopeName, empty: bool)
    where
        T: veoveo_types::Vocabulary + std::fmt::Display + std::str::FromStr + std::fmt::Debug,
        for<'a> T: TryFrom<&'a veoveo_types::ScopeName>,
        <T as std::str::FromStr>::Err: std::fmt::Debug,
    {
        assert_eq!(T::ALL.is_empty(), empty);
        for scope in T::ALL {
            assert_eq!(scope.to_string().parse::<T>().unwrap(), *scope);
        }
        assert!(T::try_from(external).is_err());
    }

    #[test]
    fn all_server_scope_vocabularies_are_available_without_service_dependencies() {
        use veoveo_types::ScopeName;
        let external = ScopeName::parse("independent:read").unwrap();

        check_scopes::<veoveo_artifact_mcp::contract::ArtifactScope>(&external, true);
        check_scopes::<veoveo_computers_mcp::contract::ComputerScope>(&external, true);
        check_scopes::<veoveo_speech_mcp::contract::SpeechScope>(&external, true);
        check_scopes::<veoveo_frames_mcp::contract::FramesScope>(&external, true);
        check_scopes::<veoveo_timeseries_mcp::contract::TimeseriesScope>(&external, true);
        check_scopes::<veoveo_media_mcp::contract::MediaScope>(&external, true);
        check_scopes::<veoveo_duckdb_mcp::contract::DuckDbScope>(&external, true);
        check_scopes::<veoveo_optimization_mcp::contract::OptimizationScope>(&external, true);
        check_scopes::<veoveo_stream_mcp::contract::StreamScope>(&external, true);
        check_scopes::<veoveo_reason_mcp::contract::ReasonScope>(&external, true);

        check_scopes::<veoveo_map_mcp::contract::MapScope>(&external, false);
        check_scopes::<veoveo_time_mcp::contract::TimeScope>(&external, false);
        check_scopes::<veoveo_view_mcp::contract::ViewScope>(&external, false);
        check_scopes::<veoveo_uav_sim_mcp::contract::UavScope>(&external, false);
        check_scopes::<veoveo_recording_mcp::contract::RecordingScope>(&external, false);
        check_scopes::<veoveo_recording_mcp::contract::RecordingProducerScope>(&external, false);
        check_scopes::<veoveo_knowledge_mcp::contract::KnowledgeScope>(&external, false);
    }

    #[test]
    fn resolved_graph_contains_only_public_contracts() {
        let mut command = std::process::Command::new("timeout");
        command
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
            ]);
        if cfg!(feature = "knowledge") {
            command.args(["--features", "knowledge"]);
        }
        let output = command.output().unwrap();
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
                "veoveo-media-mcp",
                "veoveo-map-mcp",
                "veoveo-duckdb-mcp",
                "veoveo-reason-mcp",
                "veoveo-stream-mcp",
                "veoveo-recording-mcp",
                "veoveo-optimization-mcp",
                "veoveo-time-mcp",
                "veoveo-view-mcp",
                "veoveo-uav-sim-mcp",
                "veoveo-knowledge-mcp",
            ]
            .contains(&name)
            {
                assert_eq!(
                    line.split_once("features=")
                        .unwrap()
                        .1
                        .split_whitespace()
                        .next(),
                    Some(
                        if cfg!(feature = "knowledge")
                            && ["veoveo-map-mcp", "veoveo-reason-mcp"].contains(&name)
                        {
                            "contract,knowledge"
                        } else {
                            "contract"
                        }
                    )
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
                        "veoveo-media-mcp",
                        "veoveo-frames-contract",
                        "veoveo-gateway-contract",
                        "veoveo-map-mcp",
                        "veoveo-duckdb-mcp",
                        "veoveo-reason-mcp",
                        "veoveo-stream-mcp",
                        "veoveo-recording-mcp",
                        "veoveo-optimization-mcp",
                        "veoveo-time-mcp",
                        "veoveo-view-mcp",
                        "veoveo-uav-sim-mcp",
                        "veoveo-knowledge-mcp",
                        "veoveo-knowledge-contract",
                        "veoveo-embedding-contract",
                        "veoveo-recording-contract",
                        "veoveo-recording-video",
                        "veoveo-mcp-knowledge-extension",
                        "veoveo-mcp-knowledge-macros",
                        "veoveo-types",
                        "veoveo-macros"
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
