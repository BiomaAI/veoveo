//! Admission through the actual SDK executable; no GStreamer graph executes.
use super::*;
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "requires VEOVEO_STREAM_GST_RUNNER built with the owning DeepStream SDK; pure parser validation"]
async fn actual_sdk_parser_admits_current_requests_and_refuses_old_or_mixed_before_effects() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let runner = PathBuf::from(
            std::env::var_os("VEOVEO_STREAM_GST_RUNNER").expect("owning SDK executable required"),
        );
        assert!(runner.is_absolute());
        let workspace = tempfile::tempdir().unwrap();
        let input = workspace.path().join("input.mp4");
        let engine = workspace.path().join("model.engine");
        let config = workspace.path().join("inference.txt");
        for path in [&input, &engine, &config] {
            std::fs::write(path, []).unwrap();
        }
        let response = workspace.path().join("response.json");
        let request = RunnerRequest {
            schema: RUNNER_REQUEST_SCHEMA.into(),
            task_id: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
            input_mp4: input,
            input_width: 32,
            input_height: 32,
            response_json: response.clone(),
            pipeline: RunnerPipeline {
                pipeline_id: "detector".parse().unwrap(),
                graph: GStreamerGraphConfig {
                    launch: "filesrc name=source ! fakesink name=results".into(),
                    source_element: Some("source".into()),
                    stream_muxer_element: Some("mux".into()),
                    inference_element: Some("infer".into()),
                    tracker_element: None,
                    results_element: Some("results".into()),
                    encoded_output_element: None,
                },
                profile: RunnerPerceptionProfile {
                    operation: crate::contract::PerceptionOperation::ObjectDetection,
                    inference_config_path: config,
                    tracker: None,
                },
            },
            model: RunnerModel {
                model_id: "detector".parse().unwrap(),
                model_path: engine,
                format: crate::contract::ModelFormat::TensorRtEngine,
            },
            requested_range: IndexRange::new(-1, 1).unwrap(),
            decode_start_index: -2,
            sampling: SamplingPolicy::EveryFrame,
            max_output_frames: 2,
            max_detections_per_frame: 2,
            max_response_bytes: 1024,
        };
        let current = serde_json::to_value(request).unwrap();
        let request_path = workspace.path().join("request.json");
        async fn validate(
            runner: &Path,
            request_path: &Path,
            response: &Path,
            wire: &Value,
            admitted: bool,
        ) {
            std::fs::write(request_path, serde_json::to_vec(wire).unwrap()).unwrap();
            let output = Command::new(runner)
                .kill_on_drop(true)
                .arg("--validate-request")
                .arg("--request-json")
                .arg(request_path)
                .arg("--response-json")
                .arg(response)
                .output()
                .await
                .unwrap();
            assert_eq!(
                output.status.success(),
                admitted,
                "bounded parser status: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stdout.is_empty());
            assert!(output.stderr.len() <= 1024);
            assert!(!response.exists());
            assert!(!response.with_extension("json.tmp").exists());
        }
        validate(&runner, &request_path, &response, &current, true).await;
        for (pointer, field, retired) in [
            ("", "taskId", "task_id"),
            ("", "inputMp4", "input_mp4"),
            ("", "inputWidth", "input_width"),
            ("", "inputHeight", "input_height"),
            ("", "responseJson", "response_json"),
            ("", "requestedRange", "requested_range"),
            ("", "decodeStartIndex", "decode_start_index"),
            ("", "maxOutputFrames", "max_output_frames"),
            ("", "maxDetectionsPerFrame", "max_detections_per_frame"),
            ("", "maxResponseBytes", "max_response_bytes"),
            ("/pipeline", "pipelineId", "pipeline_id"),
            ("/pipeline/graph", "sourceElement", "source_element"),
            (
                "/pipeline/graph",
                "streamMuxerElement",
                "stream_muxer_element",
            ),
            ("/pipeline/graph", "inferenceElement", "inference_element"),
            ("/pipeline/graph", "resultsElement", "results_element"),
            (
                "/pipeline/profile",
                "inferenceConfigPath",
                "inference_config_path",
            ),
            ("/model", "modelId", "model_id"),
            ("/model", "modelPath", "model_path"),
        ] {
            for mode in 0..3 {
                let mut wire = current.clone();
                let object = wire.pointer_mut(pointer).unwrap().as_object_mut().unwrap();
                let value = if mode == 2 {
                    json!("retired-conflict")
                } else {
                    object[field].clone()
                };
                if mode == 0 {
                    object.remove(field);
                }
                object.insert(retired.into(), value);
                validate(&runner, &request_path, &response, &wire, false).await;
            }
        }
        for (pointer, value) in [
            (
                "/schema",
                json!("veoveo.stream-recording-runner-request/v1"),
            ),
            ("/taskId", json!("01983da0-0000-4000-8000-000000000001")),
            ("/pipeline/pipelineId", json!("bad/id")),
            ("/model/format", json!("onnx")),
            ("/pipeline/profile/kind", json!("unknown")),
            ("/pipeline/profile/operation", json!("unknown")),
            ("/inputWidth", json!("32")),
            ("/requestedRange/start", json!(u64::MAX)),
            ("/sampling/mode", json!("unknown")),
        ] {
            let mut wire = current.clone();
            *wire.pointer_mut(pointer).unwrap() = value;
            validate(&runner, &request_path, &response, &wire, false).await;
        }
    })
    .await
    .expect("SDK parser controls exceeded sixty seconds");
}
