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
            "artifactId": id,
            "artifactUri": uri,
            "byteLen": 42,
            "mimeType": "image/png",
            "createdAt": "2026-09-28T00:00:00Z",
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

/// Adversarial wire controls over owned fields, preserving all other producer data.
pub fn retired_spellings(wire: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    let mut cases = Vec::new();
    let fields = [
        ("", "taskId", "task_id"),
        ("", "resultUri", "result_uri"),
        ("/prediction", "modelId", "model_id"),
        ("/prediction", "createdAt", "created_at"),
        ("/prediction", "executionMs", "execution_ms"),
        ("/prediction", "outputCount", "output_count"),
        ("/artifacts/0/metadata", "taskId", "task_id"),
        ("/artifacts/0/metadata", "jobId", "job_id"),
        ("/artifacts/0/metadata", "modelId", "model_id"),
        ("/artifacts/0/metadata", "outputIndex", "output_index"),
        ("/artifacts/1/metadata", "taskId", "task_id"),
        ("/artifacts/1/metadata", "jobId", "job_id"),
        ("/artifacts/1/metadata", "modelId", "model_id"),
        ("/artifacts/1/metadata", "outputIndex", "output_index"),
    ];
    for (path, current, retired) in fields {
        for mode in ["replacement", "mixed", "conflicting"] {
            let mut bad = wire.clone();
            let object = bad.pointer_mut(path).unwrap().as_object_mut().unwrap();
            let value = object
                .get(current)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            if mode == "replacement" {
                object.remove(current);
            }
            object.insert(
                retired.into(),
                if mode == "conflicting" {
                    json!("retired-conflict")
                } else {
                    value
                },
            );
            cases.push((format!("{path}/{retired}:{mode}"), bad));
        }
    }
    for tag in ["veoveo.ai/media-generation/v1", "unexpected"] {
        let mut bad = wire.clone();
        bad["schema"] = tag.into();
        cases.push((format!("schema:{tag}"), bad));
    }
    cases
}
