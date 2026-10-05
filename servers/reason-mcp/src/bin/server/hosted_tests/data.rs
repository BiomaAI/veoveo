use super::{fixture::Fixture, result_fixture};
use anyhow::Result;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata};
use veoveo_mcp_contract::{ArtifactPlane, PutArtifactRequest};
use veoveo_reason_mcp::contract::*;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRuntime};
use veoveo_types::{AccessLevel, AccessSubject, TaskId, TaskTypeDefinition};

pub struct Finding {
    pub analysis: AnalysisId,
    pub artifact: ArtifactId,
    pub annotation: ArtifactId,
}
impl Fixture {
    pub async fn finding(&self) -> Finding {
        self.finding_with_results(result_fixture::results()).await
    }
    pub async fn finding_with_results(&self, results: ReasoningResults) -> Finding {
        let data = FindingData::from_results(&results).unwrap();
        let analysis = AnalysisId::try_from(TaskId::new()).unwrap();
        let tasks = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            self.store.clone(),
            "reason",
            "fixture-publisher",
        ))
        .unwrap();
        tasks
            .create(CreateTask {
                task_id: analysis.task_id(),
                owner: crate::ownership::runtime_owner(&self.owner.identity),
                server: "reason".into(),
                task_type: ReasonTaskKind::AnalyzeRecording.name(),
                request: {
                    let mut request: serde_json::Value = serde_json::from_str(include_str!(
                        "../../../../testdata/task-request.json"
                    ))
                    .unwrap();
                    request["input"]["pipeline_id"] = results.pipeline_id.to_string().into();
                    request
                },
                recovery_class: RecoveryClass::Resume,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: Default::default(),
            })
            .await
            .unwrap();
        let result = self
            .artifacts
            .put(
                &self.owner,
                PutArtifactRequest {
                    mime_type: Some("application/vnd.veoveo.reason-results+json".into()),
                    filename: Some("finding.json".into()),
                    metadata: serde_json::to_value(ReasonArtifactMetadata {
                        provenance: ReasonArtifactProvenance::Results {
                            analysis_id: analysis,
                            recording_id: results.recording_uri.id(),
                            pipeline_id: results.pipeline_id.clone(),
                            model_id: results.model_id.clone(),
                            prompt_revision: results.prompt_revision.clone(),
                            task_kind: (&results.task).into(),
                            source_snapshot_sha256: results
                                .source_snapshot
                                .digest_sha256()
                                .unwrap(),
                        },
                    })
                    .unwrap(),
                    ..Default::default()
                },
                serde_json::to_vec(&results).unwrap(),
            )
            .await
            .unwrap();
        let annotation: ArtifactMetadata = self
            .artifacts
            .put(
                &self.owner,
                PutArtifactRequest::default(),
                b"inert annotation fixture".to_vec(),
            )
            .await
            .unwrap();
        let finding = Finding {
            analysis,
            artifact: result.artifact_id(),
            annotation: annotation.artifact_id(),
        };
        let output = AnalyzeRecordingOutput::new(
            analysis,
            data,
            ReasoningSummary {
                observed_frames: 32,
                event_count: 0,
                elapsed_ms: 100,
                decode_start_index: 0,
                requested_start_index: 0,
                requested_end_index: 100,
            },
            result,
            annotation,
        );
        tasks
            .claim(analysis.task_id(), std::time::Duration::from_secs(30))
            .await
            .unwrap();
        tasks
            .transition(
                analysis.task_id(),
                veoveo_task_runtime::TaskTransition::Succeeded {
                    message: "fixture finding".into(),
                    result: serde_json::to_value(
                        veoveo_reason_mcp::task_product::analysis_tool_result(output).unwrap(),
                    )
                    .unwrap(),
                },
            )
            .await
            .unwrap();
        finding
    }
    pub async fn grant(&self, artifact: ArtifactId, principal: &str) -> Result<()> {
        self.store
            .ensure_identity(
                "reason-fixture",
                principal,
                "https://reason.fixture",
                principal,
                veoveo_platform_store::PrincipalKind::User,
            )
            .await?;
        self.artifacts
            .grant(
                &self.owner,
                &artifact,
                AccessSubject::Principal(principal.parse()?),
                AccessLevel::Read,
            )
            .await?;
        Ok(())
    }
}
