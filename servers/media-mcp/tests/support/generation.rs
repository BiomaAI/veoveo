use serde_json::json;
use veoveo_media_mcp::contract::{
    GenerationPredictionSummary, MediaGenerationResult, MediaOutputArtifactMetadata,
    MediaPredictionId,
};
use veoveo_types::TaskId;

pub fn generation(task: TaskId, prediction: MediaPredictionId) -> MediaGenerationResult {
    let artifacts = [
        (
            "0195dabe-8888-7abc-8def-000000000001",
            "media://artifact/0195dabe-8888-7abc-8def-000000000001",
        ),
        (
            "0195dabe-8888-7abc-8def-000000000002",
            "media://artifact/0195dabe-8888-7abc-8def-000000000002",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, uri))| {
        serde_json::from_value(json!({
            "artifact_id": id,
            "artifact_uri": uri,
            "byte_len": 42,
            "mime_type": "image/png",
            "created_at": "2026-09-28T00:00:00Z",
            "metadata": MediaOutputArtifactMetadata {
                task_id: task,
                job_id: prediction.clone(),
                model_id: "test/image".parse().unwrap(),
                output_index: index,
            },
        }))
        .unwrap()
    })
    .collect();
    MediaGenerationResult::new(
        task,
        GenerationPredictionSummary {
            id: prediction,
            model_id: "test/image".parse().unwrap(),
            status: "completed".into(),
            created_at: None,
            error: None,
            execution_ms: Some(10.),
            timings: None,
            output_count: 2,
        },
        artifacts,
    )
    .unwrap()
}
